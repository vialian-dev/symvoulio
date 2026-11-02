// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{atomic::Ordering, Arc, Mutex},
    thread::JoinHandle,
    time::Duration,
};
use symvoulio_core::{
    detect::{self, Detection},
    engine::{self, RunConfig, RunControl, RunEvent, RunSnapshot, RunSummary},
    provider::{ClaudeCli, CodexCli, ProviderClient},
    *,
};
use tauri::{ipc::Channel, Emitter, Manager, State};
#[derive(Serialize)]
pub struct DecisionResult {
    #[serde(flatten)]
    snapshot: RunSnapshot,
    publication: &'static str,
}
impl std::ops::Deref for DecisionResult {
    type Target = RunSnapshot;
    fn deref(&self) -> &Self::Target { &self.snapshot }
}

type Result<T> = std::result::Result<T, String>;
fn failure<T>(_: T) -> String {
    "Operation failed. Check the workspace and provider connection, then retry.".into()
}
fn lock<T>(m: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>> {
    m.lock().map_err(failure)
}
#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Shell {
    claude: Option<String>,
    codex: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub workspace: PathBuf,
    shell: Shell,
    author: Provider,
    policy: Policy,
}
pub struct RunHandle {
    stopping: bool,
    control: Arc<RunControl>,
    worker: Option<JoinHandle<()>>,
    subscriber: Arc<Mutex<Option<Channel<RunEventDto>>>>,
    terminal_error: Arc<Mutex<Option<String>>>,
}
pub struct AppState {
    pub settings: Mutex<Settings>,
    pub runs: Mutex<HashMap<String, RunHandle>>,
    config: PathBuf,
    raw_output: std::sync::atomic::AtomicBool,
}
#[derive(Clone, Serialize)]
#[serde(
    rename_all = "camelCase",
    tag = "kind",
    rename_all_fields = "camelCase"
)]
pub enum RunEventDto {
    Line {
        provider: Provider,
        role: Role,
        at: String,
        summary: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        raw: Option<String>,
    },
    Phase {
        round: u32,
        role: Role,
        provider: Provider,
        phase: String,
    },
    RoundCompleted {
        round: u32,
        gate: Gate,
        new_text_pct: f64,
        contribution_exceeded: bool,
    },
    State {
        state: RunState,
        stop_reason: Option<StopReason>,
    },
    ProviderError {
        provider: Provider,
        role: Role,
        error_kind: String,
        message: String,
    },
}
impl From<RunEvent> for RunEventDto {
    fn from(e: RunEvent) -> Self {
        match e {
            RunEvent::Line {
                provider,
                role,
                line,
            } => Self::Line {
                provider,
                role,
                at: now_utc_rfc3339(),
                summary: line.readable.unwrap_or_else(|| "Provider event".into()),
                raw: (!line.raw.is_empty()).then_some(line.raw),
            },
            RunEvent::Phase {
                round,
                role,
                provider,
                phase,
            } => Self::Phase {
                round,
                role,
                provider,
                phase,
            },
            RunEvent::RoundCompleted {
                round,
                gate,
                new_text_pct,
                contribution_exceeded,
            } => Self::RoundCompleted {
                round,
                gate,
                new_text_pct,
                contribution_exceeded,
            },
            RunEvent::State { state, stop_reason } => Self::State { state, stop_reason },
            RunEvent::ProviderError {
                provider,
                role,
                kind,
                message,
            } => Self::ProviderError {
                provider,
                role,
                message: if matches!(
                    kind.as_str(),
                    "input_too_large" | "malformed" | "storage" | "interrupted" | "publication"
                ) {
                    message
                } else {
                    "Provider call failed. Check the provider connection and retry.".into()
                },
                error_kind: kind,
            },
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartInput {
    name: String,
    objective: String,
    criteria: Vec<String>,
    policy: Policy,
    author: Provider,
    primary_token: String,
    supporting_tokens: Vec<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StagedFile {
    token: String,
    name: String,
    size: usize,
    kind: String,
    chars: usize,
    error: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    path: PathBuf,
    tree: Vec<String>,
}
pub fn identifier() -> String {
    let utc = symvoulio_core::now_utc_rfc3339();
    let date = format!(
        "{}{}{}-{}{}{}",
        &utc[..4],
        &utc[5..7],
        &utc[8..10],
        &utc[11..13],
        &utc[14..16],
        &utc[17..19]
    );
    format!(
        "{date}-{:06x}",
        std::hash::BuildHasher::hash_one(&std::collections::hash_map::RandomState::new(), &utc)
            & 0xffffff
    )
}
pub fn sanitize(name: &str) -> String {
    let leaf = name.rsplit(['/', '\\']).next().unwrap_or("");
    let clean: String = leaf
        .chars()
        .filter(|c| !c.is_control() && !"<>:\"/\\|?*".contains(*c))
        .take(180)
        .collect();
    let clean = clean.trim_matches([' ', '.']);
    let base = clean
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_ascii_uppercase();
    if clean.is_empty() {
        "document".into()
    } else if matches!(
        base.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || (base.len() == 4
        && (base.starts_with("COM") || base.starts_with("LPT"))
        && base.ends_with(['1', '2', '3', '4', '5', '6', '7', '8', '9']))
    {
        format!("_{clean}")
    } else {
        clean.into()
    }
}
pub fn contained(root: &Path, path: &Path) -> Result<PathBuf> {
    let root = root.canonicalize().map_err(failure)?;
    let path = path.canonicalize().map_err(failure)?;
    if path.starts_with(root) {
        Ok(path)
    } else {
        Err("Path must be inside the workspace".into())
    }
}
fn component(s: &str) -> Result<()> {
    if s.is_empty() || !s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-') {
        Err("Invalid identifier".into())
    } else {
        Ok(())
    }
}
fn clean_staging(root: &Path, owner: Option<&str>, now: std::time::SystemTime) -> Result<()> {
    let staging = root.join("staging");
    if !staging.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(&staging).map_err(failure)? {
        let path = entry.map_err(failure)?.path();
        let metadata = fs::symlink_metadata(&path).map_err(failure)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            continue;
        }
        let owned =
            owner.is_some_and(|id| fs::read_to_string(path.join(".owner")).is_ok_and(|s| s == id));
        let abandoned = owner.is_none()
            && metadata
                .modified()
                .ok()
                .and_then(|t| now.duration_since(t).ok())
                .is_some_and(|age| age > Duration::from_secs(24 * 60 * 60));
        if owned || abandoned {
            fs::remove_dir_all(contained(&staging, &path)?).map_err(failure)?;
        }
    }
    Ok(())
}
pub fn stage(root: &Path, name: String, bytes: Vec<u8>) -> Result<StagedFile> {
    if bytes.len() > 50 * 1024 * 1024 {
        return Err("Files must be at most 50 MB".into());
    }
    let token = identifier();
    let name = sanitize(&name);
    fs::create_dir_all(root.join("staging")).map_err(failure)?;
    let staging = contained(root, &root.join("staging"))?;
    let dir = staging.join(&token);
    fs::create_dir_all(&dir).map_err(failure)?;
    let path = dir.join(&name);
    if let Err(error) = fs::write(&path, &bytes) {
        let _ = fs::remove_dir_all(&dir);
        return Err(failure(error));
    }
    let kind = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    let (chars, error) = match documents::extract(&path) {
        Ok(e) => (e.text.chars().count(), None),
        Err(documents::DocumentError::Io(_)) => (0, Some("Cannot read staged file".into())),
        Err(e) => (0, Some(e.to_string())),
    };
    Ok(StagedFile {
        token,
        name,
        size: bytes.len(),
        kind,
        chars,
        error,
    })
}
fn snapshot(path: &Path) -> Result<RunSnapshot> {
    let v: serde_json::Value =
        serde_json::from_slice(&fs::read(path.join("run_state.json")).map_err(failure)?)
            .map_err(failure)?;
    let s: RunSnapshot = serde_json::from_value(v["snapshot"].clone()).map_err(failure)?;
    Ok(scrub(s))
}
fn scrub(mut s: RunSnapshot) -> RunSnapshot {
    if let Some(e) = &mut s.last_error {
        // The engine already controls these descriptions; provider diagnostics stay generic.
        if !matches!(
            e.kind.as_str(),
            "input_too_large" | "malformed" | "storage" | "interrupted" | "publication"
        ) {
            e.message = "Provider call failed. Check connection and retry.".into();
        }
    }
    s
}
fn detections() -> Result<Vec<Detection>> {
    std::thread::scope(|s| {
        let a = s.spawn(|| detect::detect(Provider::Claude));
        let b = s.spawn(|| detect::detect(Provider::Codex));
        Ok(vec![a.join().map_err(failure)?, b.join().map_err(failure)?])
    })
}
fn clients(author: Provider) -> Result<(Arc<dyn ProviderClient>, Arc<dyn ProviderClient>)> {
    if detections()?
        .iter()
        .any(|d| !d.installed || d.connected != Some(true))
    {
        return Err("Connect both providers before starting a run".into());
    }
    let a: Arc<dyn ProviderClient> = Arc::new(ClaudeCli);
    let b: Arc<dyn ProviderClient> = Arc::new(CodexCli);
    Ok(if author == Provider::Claude {
        (a, b)
    } else {
        (b, a)
    })
}
impl AppState {
    pub fn initialize(app: &tauri::App) -> Result<Self> {
        // Test and portable seam: SYMVOULIO_DATA_DIR relocates the settings file and the default workspace, so a
        // test or a portable copy never touches the installed app's data. Unset in normal use.
        let data_dir = std::env::var_os("SYMVOULIO_DATA_DIR").map(PathBuf::from);
        let config = match &data_dir {
            Some(d) => d.join("settings.json"),
            None => app
                .path()
                .app_config_dir()
                .map_err(failure)?
                .join("settings.json"),
        };
        let settings = if config.exists() {
            serde_json::from_slice(&fs::read(&config).map_err(failure)?).map_err(failure)?
        } else {
            Settings {
                // %USERPROFILE%\Symvoulio, not Documents: measured 30.10, the app could not write a provider
                // schema file under OneDrive\Documents ("Access is denied", os error 5) while the same run in a
                // plain folder completed. Documents is often OneDrive-synced or folder-protected on Windows 11.
                workspace: match &data_dir {
                    Some(d) => d.join("workspace"),
                    None => app.path().home_dir().map_err(failure)?.join("Symvoulio"),
                },
                shell: Shell::default(),
                author: Provider::Claude,
                policy: Policy::default(),
            }
        };
        settings.policy.validate()?;
        fs::create_dir_all(&settings.workspace).map_err(failure)?;
        clean_staging(&settings.workspace, None, std::time::SystemTime::now())?;
        let state = Self {
            settings: Mutex::new(settings),
            runs: Mutex::new(HashMap::new()),
            config,
            raw_output: std::sync::atomic::AtomicBool::new(false),
        };
        state.persist(&*lock(&state.settings)?)?;
        Ok(state)
    }
    fn persist(&self, s: &Settings) -> Result<()> {
        fs::create_dir_all(self.config.parent().ok_or("Invalid config directory")?)
            .map_err(failure)?;
        let tmp = self.config.with_extension("tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(s).map_err(failure)?).map_err(failure)?;
        fs::rename(tmp, &self.config).map_err(failure)
    }
    fn root(&self) -> Result<PathBuf> {
        Ok(lock(&self.settings)?.workspace.clone())
    }
    fn dir(&self, id: &str) -> Result<PathBuf> {
        component(id)?;
        let root = self.root()?;
        contained(&root, &root.join("runs").join(id))
    }
    fn idle(&self, id: &str) -> Result<std::sync::MutexGuard<'_, HashMap<String, RunHandle>>> {
        let runs = lock(&self.runs)?;
        if runs
            .get(id)
            .is_some_and(|h| h.stopping || h.worker.as_ref().is_some_and(|w| !w.is_finished()))
        {
            Err("Pause or finish the run first".into())
        } else {
            Ok(runs)
        }
    }
    pub fn stop_all(&self) {
        let workers = if let Ok(mut runs) = self.runs.lock() {
            for h in runs.values() {
                h.control.stop();
            }
            runs.values_mut()
                .filter_map(|h| h.worker.take())
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        // Give the engine's cancellation loop time to kill and reap children
        // before Tauri exits and terminates the worker threads.
        for worker in workers {
            let _ = worker.join();
        }
    }
    fn stop(&self, run_id: &str, app: Option<tauri::AppHandle>) -> Result<RunSnapshot> {
        let worker = {
            let mut runs = lock(&self.runs)?;
            self.dir(run_id)?;
            let handle = runs.entry(run_id.to_owned()).or_insert_with(|| RunHandle {
                stopping: false,
                control: Arc::default(),
                worker: None,
                subscriber: Arc::default(),
                terminal_error: Arc::default(),
            });
            {
                if handle.stopping {
                    return Err("Run is stopping".into());
                }
                handle.stopping = true;
                handle.control.stop();
                handle.worker.take()
            }
        };
        // The per-run guard excludes mutations while other runs remain accessible.
        let result = (|| {
            if let Some(worker) = worker {
                worker.join().map_err(failure)?;
            }
            let path = self.dir(run_id)?;
            let current = snapshot(&path)?;
            if matches!(current.state, RunState::Paused | RunState::Returned) {
                engine::stop_idle(&path).map_err(failure)
            } else {
                Ok(current)
            }
        })();
        let mut runs = lock(&self.runs)?;
        if let Some(handle) = runs.get_mut(run_id) {
            handle.stopping = false;
            *lock(&handle.terminal_error)? = None;
        }
        drop(runs);
        let stopped = result?;
        if let Some(app) = app {
            app.emit("run-updated", run_id).map_err(failure)?;
        }
        Ok(scrub(stopped))
    }
    fn delete(&self, run_id: &str) -> Result<()> {
        let mut runs = self.idle(run_id)?;
        let dir = self.dir(run_id)?;
        if dir.join("run_state.json").exists() && snapshot(&dir)?.state == RunState::Running {
            return Err("Run is running".into());
        }
        clean_staging(&self.root()?, Some(run_id), std::time::SystemTime::now())?;
        fs::remove_dir_all(dir).map_err(failure)?;
        runs.remove(run_id);
        Ok(())
    }
    fn discard(&self, tokens: &[String]) -> Result<()> {
        let root = self.root()?;
        let mut first_error = None;
        for token in tokens {
            // Unrecognized tokens, including malformed names, never name an owned entry.
            if component(token).is_err() {
                continue;
            }
            let result = (|| {
                let path = root.join("staging").join(token);
                if path.exists() {
                    let path = contained(&root.join("staging"), &path)?;
                    fs::remove_dir_all(path).map_err(failure)?;
                }
                Ok(())
            })();
            if let Err(error) = result {
                first_error.get_or_insert(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }
    fn restart(&self, run_id: &str, revision: u32) -> Result<RestartInput> {
        // Committed revisions and original inputs are immutable, even while a later round runs.
        let _runs = lock(&self.runs)?;
        let dir = self.dir(run_id)?;
        let saved = snapshot(&dir)?;
        if revision > saved.rounds_done {
            return Err("Revision is not committed".into());
        }
        let primary_name = if revision == 0 {
            saved.primary_name.clone()
        } else {
            format!("revision_{revision:02}.md")
        };
        let primary = if revision == 0 {
            dir.join("inputs").join(&primary_name)
        } else {
            dir.join("output/revisions").join(&primary_name)
        };
        let root = self.root()?;
        let mut tokens = Vec::new();
        let result = (|| {
            let p = stage(
                &root,
                primary_name.clone(),
                fs::read(contained(&dir, &primary)?).map_err(failure)?,
            )?;
            tokens.push(p.token.clone());
            if let Some(error) = p.error {
                return Err(error);
            }
            let mut supporting_tokens = Vec::new();
            for name in &saved.supporting_names {
                let path = contained(&dir, &dir.join("inputs/supporting").join(name))?;
                let file = stage(&root, name.clone(), fs::read(path).map_err(failure)?)?;
                tokens.push(file.token.clone());
                if let Some(error) = file.error {
                    return Err(error);
                }
                supporting_tokens.push(file.token);
            }
            Ok(RestartInput {
                primary_token: p.token,
                primary_name,
                supporting_tokens,
                supporting_names: saved.supporting_names,
            })
        })();
        if result.is_err() {
            let _ = self.discard(&tokens);
        }
        result
    }
    fn staged(&self, token: &str) -> Result<PathBuf> {
        component(token)?;
        let root = self.root()?;
        let dir = contained(&root, &root.join("staging").join(token))?;
        let mut files = fs::read_dir(dir)
            .map_err(failure)?
            .filter(|entry| !entry.as_ref().is_ok_and(|e| e.file_name() == ".owner"));
        let file = files
            .next()
            .ok_or("Staged file missing")?
            .map_err(failure)?
            .path();
        contained(&root, &file)
    }
    fn launch(
        &self,
        id: String,
        cfg: Option<RunConfig>,
        channel: Option<Channel<RunEventDto>>,
        app: Option<tauri::AppHandle>,
    ) -> Result<RunSnapshot> {
        let mut runs = lock(&self.runs)?;
        if runs
            .get(&id)
            .is_some_and(|h| h.stopping || h.worker.as_ref().is_some_and(|w| !w.is_finished()))
        {
            return Err("Run is already active".into());
        }
        let path = if let Some(c) = &cfg {
            c.run_dir.clone()
        } else {
            self.dir(&id)?
        };
        let author = if let Some(c) = &cfg {
            c.author
        } else {
            let saved = snapshot(&path)?;
            if !matches!(
                saved.state,
                RunState::Paused | RunState::Running | RunState::Returned
            ) {
                return Err("Only paused, interrupted, or returned runs can resume".into());
            }
            saved.author
        };
        let clients = clients(author)?;
        let subscriber = runs
            .get(&id)
            .map(|h| h.subscriber.clone())
            .unwrap_or_default();
        if channel.is_some() {
            *lock(&subscriber)? = channel;
        }
        let sink = subscriber.clone();
        let control = Arc::new(RunControl::default());
        control
            .raw_output
            .store(self.raw_output.load(Ordering::SeqCst), Ordering::SeqCst);
        let ctl = control.clone();
        let event_id = id.clone();
        let terminal_error = Arc::new(Mutex::new(None));
        let worker_error = terminal_error.clone();
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let read_path = path.clone();
        let worker = std::thread::spawn(move || {
            let storage_notified = std::sync::atomic::AtomicBool::new(false);
            let on = |event: RunEvent| {
                if matches!(&event, RunEvent::ProviderError { kind, .. } if kind == "storage") {
                    storage_notified.store(true, Ordering::SeqCst);
                }
                let refresh = matches!(
                    event,
                    RunEvent::State { .. }
                        | RunEvent::RoundCompleted { .. }
                        | RunEvent::Phase { .. }
                );
                if refresh {
                    let _ = tx.try_send(snapshot(&read_path));
                    if let Some(app) = &app {
                        let _ = app.emit("run-updated", &event_id);
                    }
                }
                if let Ok(s) = sink.lock() {
                    if let Some(ch) = s.as_ref() {
                        let _ = ch.send(RunEventDto::from(event));
                    }
                }
            };
            let result = if let Some(cfg) = cfg {
                engine::start(cfg, clients, ctl, on)
            } else {
                engine::resume(path, clients, ctl, on)
            };
            if let Err(e) = result {
                // Only the engine's explicit storage notice establishes this overlay.
                if storage_notified.load(Ordering::SeqCst) {
                    if let Ok(mut error) = worker_error.lock() {
                        *error =
                            Some("Storage operation failed. Check the workspace and retry.".into());
                    }
                }
                let message = e.to_string();
                let _ = tx.try_send(Err(
                    if message.starts_with("Extracted inputs exceed 8 MB:")
                        || matches!(e.downcast_ref::<documents::DocumentError>(), Some(documents::DocumentError::EmptyPrimary))
                    {
                        message
                    } else {
                        failure(e)
                    },
                ));
            }
            // Completion notification is independent of the one-shot startup receiver.
            if let Some(app) = &app {
                let _ = app.emit("run-updated", &event_id);
            }
        });
        runs.insert(
            id,
            RunHandle {
                stopping: false,
                control,
                worker: Some(worker),
                subscriber,
                terminal_error,
            },
        );
        drop(runs);
        rx.recv_timeout(Duration::from_secs(60)).map_err(failure)?
    }
    fn decide(
        &self,
        id: String,
        kind: DecisionKind,
        name: String,
        note: String,
        app: Option<tauri::AppHandle>,
    ) -> Result<DecisionResult> {
        if name.trim().is_empty() {
            return Err("Decider name is required".into());
        }
        // A human decides on a finished run only (S9 is enabled when stopped or completed, or to revise a decision).
        let state = snapshot(&self.dir(&id)?)?.state;
        if !matches!(
            state,
            RunState::Stopped
                | RunState::Completed
                | RunState::Approved
                | RunState::Rejected
                | RunState::Returned
        ) {
            return Err("A decision needs a stopped or completed run".into());
        }
        let runs = self.idle(&id)?;
        let (saved, publication) = match engine::decide(self.dir(&id)?, kind, name, note) {
            Ok(saved) => (saved, "complete"),
            Err(error) => {
                if let Some(incomplete) = error.downcast_ref::<engine::PublicationIncomplete>() {
                    let saved = incomplete.snapshot.clone();
                    if let Some(app) = &app {
                        let _ = app.emit("run-snapshot", scrub(saved.clone()));
                    }
                    (saved, "incomplete")
                } else {
                    return Err(failure(error));
                }
            }
        };
        if let Some(handle) = runs.get(&id) {
            *lock(&handle.terminal_error)? = None;
        }
        if let Some(app) = &app {
            let _ = app.emit("run-updated", &id);
        }
        if kind == DecisionKind::Returned {
            drop(runs);
            // A launch failure cannot undo the human decision already committed above.
            let snapshot = match self.launch(id.clone(), None, None, app) {
                Ok(snapshot) => snapshot,
                Err(_) => {
                    let mut snapshot = self.dir(&id).and_then(|path| snapshot(&path)).unwrap_or(saved);
                    snapshot.last_error = Some(engine::LastError {
                        provider: None, kind: "storage".into(),
                        message: "Decision saved; could not start the next round. Check the workspace and resume.".into(),
                    });
                    scrub(snapshot)
                }
            };
            Ok(DecisionResult { snapshot, publication })
        } else {
            Ok(DecisionResult { snapshot: scrub(saved), publication })
        }
    }
    fn start(
        &self,
        input: StartInput,
        ch: Option<Channel<RunEventDto>>,
        app: Option<tauri::AppHandle>,
    ) -> Result<RunSnapshot> {
        let tokens: Vec<_> = std::iter::once(input.primary_token.clone())
            .chain(input.supporting_tokens.iter().cloned())
            .collect();
        let result = (|| {
            for token in &tokens {
                component(token)?;
            }
            input.policy.validate()?;
            if input.criteria.is_empty() || input.criteria.iter().any(|c| c.trim().is_empty()) {
                return Err("Approve non-empty criteria before starting".into());
            }
            let root = self.root()?;
            fs::create_dir_all(root.join("runs")).map_err(failure)?;
            let runs_root = contained(&root, &root.join("runs"))?;
            let id = identifier();
            let cfg = RunConfig {
                run_id: id.clone(),
                name: input.name,
                run_dir: runs_root.join(&id),
                author: input.author,
                objective: input.objective,
                criteria: input.criteria,
                policy: input.policy,
                primary: self.staged(&input.primary_token)?,
                supporting: input
                    .supporting_tokens
                    .iter()
                    .map(|s| self.staged(s))
                    .collect::<Result<_>>()?,
                call_timeout: Duration::from_secs(600),
                author_model: None,
                reviewer_model: None,
            };
            for path in std::iter::once(&cfg.primary).chain(cfg.supporting.iter()) {
                fs::write(
                    path.parent().ok_or("Invalid staged file")?.join(".owner"),
                    &id,
                )
                .map_err(failure)?;
            }
            self.launch(id, Some(cfg), ch, app)
        })();
        let cleanup = self.discard(&tokens);
        if result.is_ok() {
            cleanup?;
        }
        result
    }
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestartInput {
    pub primary_token: String,
    pub primary_name: String,
    pub supporting_tokens: Vec<String>,
    pub supporting_names: Vec<String>,
}
#[tauri::command]
pub fn stage_discard(state: State<'_, AppState>, tokens: Vec<String>) -> Result<()> {
    state.discard(&tokens)
}
#[tauri::command]
pub fn run_restart(
    state: State<'_, AppState>,
    run_id: String,
    revision: u32,
) -> Result<RestartInput> {
    state.restart(&run_id, revision)
}
#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> Result<Settings> {
    Ok(lock(&state.settings)?.clone())
}
#[tauri::command]
pub fn settings_set(state: State<'_, AppState>, settings: Settings) -> Result<Settings> {
    settings.policy.validate()?;
    if lock(&state.runs)?
        .values()
        .any(|h| h.worker.as_ref().is_some_and(|w| !w.is_finished()))
    {
        return Err("Finish active runs before changing settings".into());
    }
    fs::create_dir_all(&settings.workspace).map_err(failure)?;
    let mut current = lock(&state.settings)?;
    state.persist(&settings)?;
    *current = settings.clone();
    Ok(settings)
}
#[tauri::command]
pub async fn detect_providers() -> Result<Vec<Detection>> {
    tauri::async_runtime::spawn_blocking(detections)
        .await
        .map_err(failure)?
}
#[tauri::command]
pub async fn stage_file(
    state: State<'_, AppState>,
    name: String,
    bytes: Vec<u8>,
) -> Result<StagedFile> {
    let root = state.root()?;
    tauri::async_runtime::spawn_blocking(move || stage(&root, name, bytes))
        .await
        .map_err(failure)?
}
#[tauri::command]
pub async fn run_start(
    app: tauri::AppHandle,
    input: StartInput,
    on_event: Channel<RunEventDto>,
) -> Result<RunSnapshot> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>()
            .start(input, Some(on_event), Some(app.clone()))
    })
    .await
    .map_err(failure)?
}
#[tauri::command]
pub fn run_subscribe(
    state: State<'_, AppState>,
    run_id: String,
    on_event: Channel<RunEventDto>,
) -> Result<()> {
    state.dir(&run_id)?;
    let mut runs = lock(&state.runs)?;
    if let Some(h) = runs.get_mut(&run_id) {
        *lock(&h.subscriber)? = Some(on_event);
    } else {
        let control = Arc::new(RunControl::default());
        runs.insert(
            run_id,
            RunHandle {
                stopping: false,
                control,
                worker: None,
                subscriber: Arc::new(Mutex::new(Some(on_event))),
                terminal_error: Arc::default(),
            },
        );
    }
    Ok(())
}
#[tauri::command]
pub fn run_pause(state: State<'_, AppState>, run_id: String) -> Result<()> {
    lock(&state.runs)?
        .get(&run_id)
        .ok_or("Run is not active")?
        .control
        .pause
        .store(true, Ordering::SeqCst);
    Ok(())
}
#[tauri::command]
pub async fn run_stop(app: tauri::AppHandle, run_id: String) -> Result<RunSnapshot> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>().stop(&run_id, Some(app.clone()))
    })
    .await
    .map_err(failure)?
}
#[tauri::command]
pub async fn run_resume(app: tauri::AppHandle, run_id: String) -> Result<RunSnapshot> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>()
            .launch(run_id, None, None, Some(app.clone()))
    })
    .await
    .map_err(failure)?
}
#[tauri::command]
pub async fn run_decide(
    app: tauri::AppHandle,
    run_id: String,
    kind: DecisionKind,
    name: String,
    note: String,
) -> Result<DecisionResult> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>()
            .decide(run_id, kind, name, note, Some(app.clone()))
    })
    .await
    .map_err(failure)?
}
#[tauri::command]
pub fn run_decide_issue(
    state: State<'_, AppState>,
    run_id: String,
    issue_id: String,
    choice: HumanChoice,
    note: String,
    answered_by: Option<String>,
) -> Result<RunSnapshot> {
    let _runs = state.idle(&run_id)?;
    engine::decide_issue_with_answerer(
        state.dir(&run_id)?,
        &issue_id,
        choice,
        &note,
        answered_by.as_deref(),
    )
    .map_err(|e| {
        if e.to_string() == "An answer is required" {
            e.to_string()
        } else {
            failure(e)
        }
    })
    .map(scrub)
}
#[tauri::command]
pub fn raw_output_set(state: State<'_, AppState>, enabled: bool) -> Result<()> {
    state.set_raw_output(enabled)
}
impl AppState {
    fn set_raw_output(&self, enabled: bool) -> Result<()> {
        let runs = lock(&self.runs)?;
        self.raw_output.store(enabled, Ordering::SeqCst);
        for handle in runs.values() {
            handle.control.raw_output.store(enabled, Ordering::SeqCst);
        }
        Ok(())
    }
}
#[tauri::command]
pub fn run_get(state: State<'_, AppState>, run_id: String) -> Result<RunSnapshot> {
    state.get(&run_id)
}
impl AppState {
    fn get(&self, run_id: &str) -> Result<RunSnapshot> {
        let path = self.dir(run_id)?;
        let runs = lock(&self.runs)?;
        if let Some(handle) = runs.get(run_id) {
            if let Some(error) = lock(&handle.terminal_error)?.as_ref() {
                let mut saved = snapshot(&path).map_err(|_| error.clone())?;
                saved.state = RunState::Paused;
                saved.revision += 1;
                saved.phase = None;
                saved.stop_reason = None;
                saved.last_error = Some(engine::LastError {
                    provider: Some(saved.author),
                    kind: "storage".into(),
                    message: error.clone(),
                });
                return Ok(scrub(saved));
            }
        }
        // Reads check derived artifacts under the engine publication mutex, repairing only disagreement.
        // The same mutex guards staging through cleanup, so readers cannot interrupt a writer.
        if runs.contains_key(run_id) {
            engine::read_snapshot(&path).map(scrub).map_err(failure)
        } else {
            engine::load(path).map(scrub).map_err(failure)
        }
    }
}

#[tauri::command]
pub fn runs_list(state: State<'_, AppState>) -> Result<Vec<RunSummary>> {
    state.list_runs()
}
impl AppState {
    fn list_runs(&self) -> Result<Vec<RunSummary>> {
        let mut out = vec![];
        let root = self.root()?.join("runs");
        if root.exists() {
            for entry in fs::read_dir(root).map_err(failure)? {
                let path = entry.map_err(failure)?.path();
                if let Some(id) = path.file_name().and_then(|s| s.to_str()) {
                    if let Ok(s) = self.get(id) {
                        out.push(RunSummary {
                            id: s.id,
                            name: s.name,
                            state: s.state,
                            rounds_done: s.rounds_done,
                            created_at: s.created_at,
                            run_dir: path,
                        });
                    }
                }
            }
        }
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(out)
    }
}
#[tauri::command]
pub fn run_delete(state: State<'_, AppState>, run_id: String) -> Result<()> {
    state.delete(&run_id)
}
#[derive(Deserialize)]
struct ExportState {
    snapshot: RunSnapshot,
}
fn export(dir: &Path, partial: bool) -> Result<ExportResult> {
    let saved: ExportState =
        serde_json::from_slice(&fs::read(dir.join("run_state.json")).map_err(failure)?)
            .map_err(failure)?;
    let s = saved.snapshot;
    let allowed = if partial {
        matches!(s.state, RunState::Stopped | RunState::Paused)
            || (matches!(s.state, RunState::Approved | RunState::Rejected)
                && matches!(s.stop_reason, Some(StopReason::UserStop | StopReason::Cancelled)))
    } else {
        matches!(
            s.state,
            RunState::Completed
                | RunState::Stopped
                | RunState::Approved
                | RunState::Rejected
                | RunState::Returned
        )
    };
    if !allowed {
        return Err("Pause or finish the run before exporting".into());
    }
    let p = OutputPackage::reopen(dir, s.documents.first().cloned().unwrap_or_default());
    engine::export_report(dir).map_err(failure)?;
    Ok(ExportResult {
        path: dir.join("output"),
        tree: p.tree(),
    })
}
#[tauri::command]
pub fn run_export(
    state: State<'_, AppState>,
    run_id: String,
    partial: bool,
) -> Result<ExportResult> {
    let _runs = state.idle(&run_id)?;
    export(&state.dir(&run_id)?, partial)
}
#[tauri::command]
pub fn open_folder(state: State<'_, AppState>, path: PathBuf) -> Result<()> {
    let path = contained(&state.root()?, &path)?;
    let command = if cfg!(windows) {
        "explorer.exe"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(command)
        .arg(path)
        .spawn()
        .map_err(failure)?;
    Ok(())
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: &'static str,
    os: &'static str,
    arch: &'static str,
}
#[tauri::command]
pub fn app_info() -> Result<AppInfo> {
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
    })
}

#[cfg(test)]
mod tests {
    static PROVIDER_ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());
    use super::*;
    /// The fake provider CLI from symvoulio-core, next to this test binary (target/<profile>/fake-cli[.exe]).
    /// `cargo test -p symvoulio-core -p symvoulio` (the gate) builds it; a nested cargo here would wait on the build lock.
    fn fake_cli() -> PathBuf {
        let exe = std::env::current_exe().unwrap();
        let profile_dir = exe.parent().and_then(Path::parent).unwrap().to_path_buf();
        let fake = profile_dir.join(format!("fake-cli{}", std::env::consts::EXE_SUFFIX));
        assert!(
            fake.is_file(),
            "fake-cli missing: run `cargo build -p symvoulio-core --bin fake-cli` or test both crates together"
        );
        fake
    }
    struct Workspace(PathBuf);
    impl Workspace {
        fn new() -> Self {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join(".command-tests")
                .join(identifier());
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Workspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/command_layer.rs"));
    #[test]
    fn stop_after_paused_worker_exit_persists_stopped() {
        let _environment = PROVIDER_ENV.lock().unwrap();
        std::env::set_var("SYMVOULIO_FAKE_CLI", fake_cli());
        let w = Workspace::new();
        let state = AppState {
            settings: Mutex::new(Settings {
                workspace: w.0.clone(),
                shell: Shell::default(),
                author: Provider::Claude,
                policy: Policy::default(),
            }),
            runs: Mutex::new(HashMap::new()),
            config: w.0.join("settings.json"),
            raw_output: std::sync::atomic::AtomicBool::new(false),
        };
        let file = stage(&w.0, "primary.md".into(), b"# Original".to_vec()).unwrap();
        let (tx, rx) = std::sync::mpsc::sync_channel(0);
        let rx = Mutex::new(rx);
        let channel = Channel::new(move |event| {
            if let tauri::ipc::InvokeResponseBody::Json(json) = event {
                let value: serde_json::Value = serde_json::from_str(&json).unwrap();
                if value["kind"] == "state" && value["state"] == "running" {
                    rx.lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(10))
                        .unwrap();
                }
            }
            Ok(())
        });
        let started = state
            .start(
                StartInput {
                    name: "Stop test".into(),
                    objective: "Review".into(),
                    criteria: vec!["Accuracy".into()],
                    policy: Policy::default(),
                    author: Provider::Claude,
                    primary_token: file.token,
                    supporting_tokens: vec![],
                },
                Some(channel),
                None,
            )
            .unwrap();
        lock(&state.runs)
            .unwrap()
            .get(&started.id)
            .unwrap()
            .control
            .pause
            .store(true, Ordering::SeqCst);
        tx.send(()).unwrap();
        let worker = lock(&state.runs)
            .unwrap()
            .get_mut(&started.id)
            .unwrap()
            .worker
            .take()
            .unwrap();
        worker.join().unwrap();
        let dir = state.dir(&started.id).unwrap();
        assert_eq!(snapshot(&dir).unwrap().state, RunState::Paused);
        assert!(lock(&state.runs)
            .unwrap()
            .get(&started.id)
            .unwrap()
            .terminal_error
            .lock()
            .unwrap()
            .is_none());
        // A worker blocked during termination must not block the registry or allow a new mutation.
        let (release, wait) = std::sync::mpsc::channel();
        lock(&state.runs)
            .unwrap()
            .get_mut(&started.id)
            .unwrap()
            .worker = Some(std::thread::spawn(move || {
            wait.recv().unwrap();
        }));
        std::thread::scope(|scope| {
            let stopped = scope.spawn(|| state.stop(&started.id, None));
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            let mut accessible = false;
            while std::time::Instant::now() < deadline {
                if let Ok(runs) = state.runs.try_lock() {
                    if runs.get(&started.id).unwrap().worker.is_none() {
                        accessible = true;
                        break;
                    }
                }
                std::thread::yield_now();
            }
            if accessible {
                assert!(state.idle(&started.id).is_err());
            }
            release.send(()).unwrap();
            stopped.join().unwrap().unwrap();
            assert!(accessible, "stop held global runs mutex while joining");
        });
        *lock(
            &lock(&state.runs)
                .unwrap()
                .get(&started.id)
                .unwrap()
                .terminal_error,
        )
        .unwrap() = Some("Storage operation failed".into());
        let overlay = state.get(&started.id).unwrap();
        assert_eq!(overlay.state, RunState::Paused);
        assert_eq!(
            overlay.last_error.unwrap().message,
            "Storage operation failed"
        );
        let stopped = state.stop(&started.id, None).unwrap();
        assert!(lock(&state.runs)
            .unwrap()
            .get(&started.id)
            .unwrap()
            .terminal_error
            .lock()
            .unwrap()
            .is_none());
        assert_eq!(stopped.state, RunState::Stopped);
        assert_eq!(stopped.stop_reason, Some(StopReason::UserStop));
        assert_eq!(
            snapshot(&dir).unwrap().state,
            RunState::Stopped,
            "Stop after worker exit must persist Stopped"
        );
        *lock(
            &lock(&state.runs)
                .unwrap()
                .get(&started.id)
                .unwrap()
                .terminal_error,
        )
        .unwrap() = Some("Storage operation failed".into());
        state
            .decide(
                started.id.clone(),
                DecisionKind::Approved,
                "Owner".into(),
                "".into(),
                None,
            )
            .unwrap();
        assert_eq!(state.get(&started.id).unwrap().state, RunState::Approved);
        let report_tmp = dir.join("output/final/human_decision.json.tmp");
        fs::create_dir(&report_tmp).unwrap();
        assert_eq!(
            state.get(&started.id).unwrap().state,
            RunState::Approved,
            "reading a known idle run must not republish its report"
        );
        fs::remove_dir(report_tmp).unwrap();
        // A restarted app has no worker handle at all.
        engine::decide(&dir, DecisionKind::Returned, "Owner", "").unwrap();
        lock(&state.runs).unwrap().clear();
        state.stop(&started.id, None).unwrap();
        assert_eq!(snapshot(&dir).unwrap().state, RunState::Stopped);
    }
    #[test]
    fn contract_discard_is_idempotent_and_failed_token_validation_cleans_peers() {
        let w = Workspace::new();
        let state = test_state(&w);
        let file = stage(&w.0, "private.md".into(), b"# Private".to_vec()).unwrap();
        state
            .discard(&[
                file.token.clone(),
                "unknown".into(),
                "../escape".into(),
                "".into(),
            ])
            .unwrap();
        state.discard(std::slice::from_ref(&file.token)).unwrap();
        assert!(!w.0.join("staging").join(file.token).exists());
        let file = stage(&w.0, "private.md".into(), b"# Private".to_vec()).unwrap();
        assert!(state
            .start(
                StartInput {
                    name: "Invalid".into(),
                    objective: "Review".into(),
                    criteria: vec!["Accuracy".into()],
                    policy: Policy::default(),
                    author: Provider::Claude,
                    primary_token: file.token.clone(),
                    supporting_tokens: vec!["../escape".into()],
                },
                None,
                None
            )
            .is_err());
        assert!(!w.0.join("staging").join(file.token).exists());
    }
    #[test]
    fn contract_delete_state_less_run_and_raw_session_reset() {
        let w = Workspace::new();
        let state = test_state(&w);
        let dir = w.0.join("runs/orphan");
        fs::create_dir_all(&dir).unwrap();
        let dir = state.dir("orphan").unwrap();
        fs::write(dir.join("private.md"), "private").unwrap();
        state.delete("orphan").unwrap();
        assert!(!dir.exists());
        assert!(!state.raw_output.load(Ordering::SeqCst));
        state.set_raw_output(true).unwrap();
        assert!(state.raw_output.load(Ordering::SeqCst));
        state.persist(&state.settings.lock().unwrap()).unwrap();
        assert!(!fs::read_to_string(&state.config).unwrap().contains("raw"));
        assert!(!test_state(&w).raw_output.load(Ordering::SeqCst));
    }
    #[test]
    fn contract_live_dto_has_controlled_fields_and_omits_disabled_raw() {
        let dto = RunEventDto::from(RunEvent::Line {
            provider: Provider::Claude,
            role: Role::Author,
            line: symvoulio_core::provider::StreamLine {
                raw: String::new(),
                readable: Some("assistant: assistant text, 2 words".into()),
                is_stderr: false,
            },
        });
        let value = serde_json::to_value(dto).unwrap();
        assert_eq!(value["kind"], "line");
        assert_eq!(value["summary"], "assistant: assistant text, 2 words");
        assert!(value["at"].as_str().is_some());
        assert!(value.get("raw").is_none());
        assert!(value.get("readable").is_none());
        assert!(value.get("isStderr").is_none());
    }
    fn test_state(w: &Workspace) -> AppState {
        AppState {
            settings: Mutex::new(Settings {
                workspace: w.0.clone(),
                shell: Shell::default(),
                author: Provider::Claude,
                policy: Policy::default(),
            }),
            runs: Mutex::new(HashMap::new()),
            config: w.0.join("settings.json"),
            raw_output: std::sync::atomic::AtomicBool::new(false),
        }
    }
    #[test]
    fn contract_worker_storage_notice_outlives_start_acknowledgement() {
        let _environment = PROVIDER_ENV.lock().unwrap();
        std::env::set_var("SYMVOULIO_FAKE_CLI", fake_cli());
        let w = Workspace::new();
        let state = test_state(&w);
        let file = stage(&w.0, "primary.md".into(), b"# Original".to_vec()).unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let collected = events.clone();
        let root = w.0.join("runs");
        let (tx, rx) = std::sync::mpsc::sync_channel(0);
        let rx = Mutex::new(rx);
        let channel = Channel::new(move |event| {
            if let tauri::ipc::InvokeResponseBody::Json(json) = event {
                let event: serde_json::Value = serde_json::from_str(&json).unwrap();
                if event["kind"] == "state" && event["state"] == "running" {
                    rx.lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(10))
                        .unwrap();
                }
                if event["kind"] == "phase" {
                    fs::remove_dir_all(&root).unwrap();
                    fs::write(&root, b"storage unavailable").unwrap();
                }
                collected.lock().unwrap().push(event);
            }
            Ok(())
        });
        let started = state
            .start(
                StartInput {
                    name: "Storage loss".into(),
                    objective: "Verify".into(),
                    criteria: vec!["Accuracy".into()],
                    policy: Policy::default(),
                    author: Provider::Claude,
                    primary_token: file.token,
                    supporting_tokens: vec![],
                },
                Some(channel),
                None,
            )
            .unwrap();
        tx.send(()).unwrap();
        let worker = lock(&state.runs)
            .unwrap()
            .get_mut(&started.id)
            .unwrap()
            .worker
            .take()
            .unwrap();
        worker.join().unwrap();
        let events = events.lock().unwrap();
        assert!(events.iter().any(|e| e["kind"] == "providerError"
            && e["errorKind"] == "storage"
            && e["role"] == "author"));
        assert!(events
            .iter()
            .any(|e| e["kind"] == "state" && e["state"] == "paused" && e["stopReason"].is_null()));
        assert!(lock(&state.runs)
            .unwrap()
            .get(&started.id)
            .unwrap()
            .terminal_error
            .lock()
            .unwrap()
            .is_some());
    }
    #[test]
    fn contract_restart_preserves_all_supporting_bytes_and_export_warnings() {
        let _environment = PROVIDER_ENV.lock().unwrap();
        std::env::set_var("SYMVOULIO_FAKE_CLI", fake_cli());
        let w = Workspace::new();
        let state = test_state(&w);
        let primary = stage(&w.0, "primary.md".into(), b"# Original".to_vec()).unwrap();
        let source = stage(&w.0, "source.csv".into(), b"fact,value\nA,42".to_vec()).unwrap();
        let initial = state
            .start(
                StartInput {
                    name: "Restart".into(),
                    objective: "Verify".into(),
                    criteria: vec!["Accuracy".into()],
                    policy: Policy::default(),
                    author: Provider::Claude,
                    primary_token: primary.token,
                    supporting_tokens: vec![source.token],
                },
                None,
                None,
            )
            .unwrap();
        let worker = lock(&state.runs)
            .unwrap()
            .get_mut(&initial.id)
            .unwrap()
            .worker
            .take()
            .unwrap();
        worker.join().unwrap();
        let dir = state.dir(&initial.id).unwrap();
        let saved = snapshot(&dir).unwrap();
        let restarted = state.restart(&initial.id, 1).unwrap();
        assert_eq!(restarted.primary_name, "revision_01.md");
        assert_eq!(restarted.supporting_names, vec!["source.csv"]);
        assert_eq!(
            fs::read(state.staged(&restarted.primary_token).unwrap()).unwrap(),
            fs::read(dir.join("output/revisions/revision_01.md")).unwrap()
        );
        assert_eq!(
            fs::read(state.staged(&restarted.supporting_tokens[0]).unwrap()).unwrap(),
            b"fact,value\nA,42"
        );
        let original = state.restart(&initial.id, 0).unwrap();
        assert_eq!(
            fs::read(state.staged(&original.primary_token).unwrap()).unwrap(),
            b"# Original"
        );
        assert!(state.restart(&initial.id, saved.rounds_done + 1).is_err());
        let second = state
            .start(
                StartInput {
                    name: "Restarted".into(),
                    objective: "Verify".into(),
                    criteria: vec!["Accuracy".into()],
                    policy: Policy::default(),
                    author: Provider::Claude,
                    primary_token: restarted.primary_token.clone(),
                    supporting_tokens: restarted.supporting_tokens.clone(),
                },
                None,
                None,
            )
            .unwrap();
        let worker = lock(&state.runs)
            .unwrap()
            .get_mut(&second.id)
            .unwrap()
            .worker
            .take()
            .unwrap();
        worker.join().unwrap();
        let second_dir = state.dir(&second.id).unwrap();
        assert_eq!(
            snapshot(&second_dir).unwrap().supporting_names,
            vec!["source.csv"]
        );
        let durable: serde_json::Value =
            serde_json::from_slice(&fs::read(second_dir.join("run_state.json")).unwrap()).unwrap();
        assert!(durable["supporting"].to_string().contains("42"));
        let before = fs::read_to_string(dir.join("output/final/final_review.html")).unwrap();
        export(&dir, false).unwrap();
        assert_eq!(
            before,
            fs::read_to_string(dir.join("output/final/final_review.html")).unwrap()
        );
        assert!(
            fs::read_to_string(dir.join("output/final/final_review.html"))
                .unwrap()
                .contains("Contribution limit exceeded")
        );
        state.discard(&restarted.supporting_tokens).unwrap();
        state
            .discard(&[restarted.primary_token, original.primary_token])
            .unwrap();
        state.discard(&original.supporting_tokens).unwrap();
    }

    #[test]
    fn staging_removed_after_failed_start() {
        let _environment = PROVIDER_ENV.lock().unwrap();
        std::env::set_var("SYMVOULIO_FAKE_CLI", fake_cli());
        let w = Workspace::new();
        let state = AppState {
            settings: Mutex::new(Settings {
                workspace: w.0.clone(),
                shell: Shell::default(),
                author: Provider::Claude,
                policy: Policy::default(),
            }),
            runs: Mutex::new(HashMap::new()),
            config: w.0.join("settings.json"),
            raw_output: std::sync::atomic::AtomicBool::new(false),
        };
        let file = stage(&w.0, "bad.docx".into(), b"damaged".to_vec()).unwrap();
        let result = state.start(
            StartInput {
                name: "Failed".into(),
                objective: "Review".into(),
                criteria: vec!["Accuracy".into()],
                policy: Policy::default(),
                author: Provider::Claude,
                primary_token: file.token.clone(),
                supporting_tokens: vec![],
            },
            None,
            None,
        );
        assert!(result.is_err());
        assert!(
            !w.0.join("staging").join(file.token).exists(),
            "failed start retained staging copy"
        );
        state.stop_all();
    }
    #[test]
    fn fake_failure_and_delete_remove_owned_staging() {
        let _environment = PROVIDER_ENV.lock().unwrap();
        std::env::set_var("SYMVOULIO_FAKE_CLI", fake_cli());
        std::env::set_var("FAKE_CLI_SCENARIO", "auth_fail");
        let w = Workspace::new();
        let state = AppState {
            settings: Mutex::new(Settings {
                workspace: w.0.clone(),
                shell: Shell::default(),
                author: Provider::Claude,
                policy: Policy::default(),
            }),
            runs: Mutex::new(HashMap::new()),
            config: w.0.join("settings.json"),
            raw_output: std::sync::atomic::AtomicBool::new(false),
        };
        let file = stage(&w.0, "primary.md".into(), b"private".to_vec()).unwrap();
        let started = state
            .start(
                StartInput {
                    name: "Failed".into(),
                    objective: "Review".into(),
                    criteria: vec!["Accuracy".into()],
                    policy: Policy::default(),
                    author: Provider::Claude,
                    primary_token: file.token.clone(),
                    supporting_tokens: vec![],
                },
                None,
                None,
            )
            .unwrap();
        lock(&state.runs)
            .unwrap()
            .get_mut(&started.id)
            .unwrap()
            .worker
            .take()
            .unwrap()
            .join()
            .unwrap();
        std::env::remove_var("FAKE_CLI_SCENARIO");
        assert_eq!(
            snapshot(&state.dir(&started.id).unwrap())
                .unwrap()
                .last_error
                .unwrap()
                .kind,
            "auth"
        );
        assert!(!w.0.join("staging").join(file.token).exists());
        let abandoned = stage(&w.0, "abandoned.md".into(), b"private".to_vec()).unwrap();
        let abandoned_dir = w.0.join("staging").join(abandoned.token);
        fs::write(abandoned_dir.join(".owner"), &started.id).unwrap();
        state.delete(&started.id).unwrap();
        assert!(!abandoned_dir.exists());
        assert!(!w.0.join("runs").join(started.id).exists());
    }
    #[test]
    fn staging_cleanup_respects_ownership_and_age() {
        let w = Workspace::new();
        let owned = stage(&w.0, "owned.md".into(), b"private".to_vec()).unwrap();
        let other = stage(&w.0, "other.md".into(), b"private".to_vec()).unwrap();
        let owned_dir = w.0.join("staging").join(&owned.token);
        let other_dir = w.0.join("staging").join(&other.token);
        fs::write(owned_dir.join(".owner"), "run-id").unwrap();
        clean_staging(&w.0, Some("run-id"), std::time::SystemTime::now()).unwrap();
        assert!(!owned_dir.exists());
        assert!(other_dir.exists());
        clean_staging(&w.0, None, std::time::SystemTime::now()).unwrap();
        assert!(other_dir.exists());
        clean_staging(
            &w.0,
            None,
            std::time::SystemTime::now() + Duration::from_secs(25 * 60 * 60),
        )
        .unwrap();
        assert!(!other_dir.exists());
    }
    #[test]
    fn run_ids_have_utc_format() {
        let id = identifier();
        assert_eq!(id.len(), 22);
        assert_eq!(&id[8..9], "-");
        assert_eq!(&id[15..16], "-");
        assert!(id[..8]
            .bytes()
            .chain(id[9..15].bytes())
            .all(|c| c.is_ascii_digit()));
        assert!(id[16..].bytes().all(|c| c.is_ascii_hexdigit()));
    }
    #[test]
    fn names_are_safe() {
        assert_eq!(sanitize("..\\..\\x.md"), "x.md");
        assert_eq!(sanitize("CON.txt"), "_CON.txt");
        assert_eq!(sanitize("x\n\t\0.md"), "x.md");
        assert_eq!(sanitize(".."), "document");
        assert_eq!(sanitize("LPT1.txt"), "_LPT1.txt");
    }
    #[test]
    fn folder_containment() {
        let w = Workspace::new();
        let inside = w.0.join("inside");
        fs::create_dir(&inside).unwrap();
        assert!(contained(&w.0, &inside).is_ok());
        assert!(contained(&inside, &w.0).is_err());
        assert!(contained(&inside, &inside.join("..")).is_err());
        assert!(component("../x").is_err());
    }
    #[test]
    fn staging_validates_documents() {
        let w = Workspace::new();
        let md = stage(&w.0, "../file.md".into(), "Hello é".as_bytes().to_vec()).unwrap();
        assert_eq!(md.chars, 7);
        assert_eq!(md.kind, "md");
        assert!(md.error.is_none());
        assert!(w.0.join("staging").join(md.token).join(md.name).exists());
        let ppt = stage(&w.0, "slides.pptx".into(), vec![1, 2]).unwrap();
        assert!(ppt.error.unwrap().contains("unsupported"));
    }
    #[test]
    fn controlled_error_events_keep_their_description() {
        for kind in ["malformed", "storage", "input_too_large", "auth"] {
            let event = RunEventDto::from(RunEvent::ProviderError {
                provider: Provider::Claude,
                role: Role::Author,
                kind: kind.into(),
                message: "Invalid structured output: conflicting question id".into(),
            });
            let RunEventDto::ProviderError { message, .. } = event else {
                panic!()
            };
            if kind == "auth" {
                assert!(!message.contains("question"));
            } else {
                assert!(message.contains("conflicting question id"));
            }
        }
    }
    #[test]
    fn golden_wire_fixture() {
        use symvoulio_core::engine::{
            AuthorReply, IssueSnapshot, IssueStep, LastError, RoundSnapshot,
        };
        use symvoulio_core::ledger::HumanIssueDecision;
        let timestamp = "2026-10-30T00:00:00Z";
        let policy = Policy::default();
        let counts = Counts {
            critical: 1,
            high: 2,
            medium: 3,
            low: 4,
        };
        let gate = Gate {
            kind: GateKind::Stopped,
            reason: GateReason::StoppedHuman,
            counts,
        };
        let decision = HumanDecision {
            kind: DecisionKind::Returned,
            name: "Reviewer".into(),
            note: "keep_my_text".into(),
            at: timestamp.into(),
            after_round: 1,
            revision: 1,
            document_sha256: "a".repeat(64),
        };
        let snapshot = RunSnapshot {
            generation: 2,
            revision: 17,
            id: "wire-run".into(),
            name: "Wire check".into(),
            created_at: timestamp.into(),
            author: Provider::Claude,
            reviewer: Provider::Codex,
            policy: policy.clone(),
            objective: "Check wire".into(),
            criteria: vec!["Accuracy".into()],
            state: RunState::Returned,
            stop_reason: Some(StopReason::GateHumanReviewRequired),
            last_error: Some(LastError {
                provider: Some(Provider::Codex),
                kind: "auth".into(),
                message: "Retry".into(),
            }),
            phase: None,
            pending_revision: None,
            rounds_done: 1,
            documents: vec!["# Original\n\nkeep_my_text".into(), "# Revised".into()],
            rounds: vec![RoundSnapshot {
                round: 1,
                gate: gate.clone(),
                new_text_pct: 12.5,
                contribution_exceeded: true,
                author_usage: Some(Usage {
                    input_tokens: 100,
                    output_tokens: 20,
                    cache_read_tokens: 10,
                    cache_creation_tokens: Some(5),
                }),
                reviewer_usage: Some(Usage {
                    input_tokens: 200,
                    output_tokens: 30,
                    cache_read_tokens: 0,
                    cache_creation_tokens: None,
                }),
                author_model: Some("fake-author".into()),
                reviewer_model: Some("fake-reviewer".into()),
            }],
            issues: vec![IssueSnapshot {
                issue: Issue {
                    issue_id: "I-1".into(),
                    severity: Severity::Critical,
                    criterion: "Accuracy".into(),
                    finding: "Check total".into(),
                    evidence: "Source".into(),
                    proposed_action: "Correct total".into(),
                    human_decision_required: true,
                    status: Status::Open,
                },
                raised_in: 1,
                resolved_on_arrival: None,
                history: vec![IssueStep {
                    round: 1,
                    status: Status::Open,
                    severity: Severity::Critical,
                    finding: "Check total".into(),
                }],
                author_responses: vec![AuthorReply {
                    round: 1,
                    response: IssueResponse::RejectedWithEvidence,
                    explanation: "keep_my_text".into(),
                    evidence: "Source".into(),
                }],
                human_decision: Some(HumanIssueDecision {
                    issue_id: "I-1".into(),
                    choice: HumanChoice::Keep,
                    note: "Known risk".into(),
                    round: 1,
                }),
            }],
            human_questions: vec![symvoulio_core::ledger::HumanQuestionRecord {
                issue_id: "R01-099".into(),
                question: "Who owns this?".into(),
                round: 1,
                resolved: false,
                answer: None,
                answered_at: None,
                answered_by: None,
            }],
            counts,
            decision: Some(decision.clone()),
            decision_log: vec![decision],
            output_tree: vec!["final/final_document.md".into()],
            primary_name: "input.md".into(),
            supporting_names: vec!["source.md".into()],
        };
        let settings = Settings {
            workspace: "wire-workspace".into(),
            shell: Shell {
                claude: Some("pwsh".into()),
                codex: None,
            },
            author: Provider::Claude,
            policy: policy.clone(),
        };
        let input = StartInput {
            name: "Wire check".into(),
            objective: "Check wire".into(),
            criteria: vec!["Accuracy".into()],
            policy,
            author: Provider::Claude,
            primary_token: "primary".into(),
            supporting_tokens: vec!["support".into()],
        };
        let events = vec![
            RunEventDto::Line {
                provider: Provider::Claude,
                role: Role::Author,
                raw: Some("keep_my_text".into()),
                at: timestamp.into(),
                summary: "assistant: assistant text, 2 words".into(),
            },
            RunEventDto::Phase {
                round: 1,
                role: Role::Reviewer,
                provider: Provider::Codex,
                phase: "review".into(),
            },
            RunEventDto::RoundCompleted {
                round: 1,
                gate,
                new_text_pct: 12.5,
                contribution_exceeded: true,
            },
            RunEventDto::State {
                state: RunState::Stopped,
                stop_reason: Some(StopReason::UserStop),
            },
            RunEventDto::ProviderError {
                provider: Provider::Codex,
                role: Role::Reviewer,
                error_kind: "auth".into(),
                message: "Retry".into(),
            },
        ];
        let wire = serde_json::json!({
            "decisionResult": DecisionResult { snapshot: snapshot.clone(), publication: "incomplete" },
            "runSnapshot": snapshot,
            "runSummary": RunSummary { id: "wire-run".into(), name: "Wire check".into(), state: RunState::Returned, rounds_done: 1, created_at: timestamp.into(), run_dir: "wire-workspace/runs/wire-run".into() },
            "detection": Detection { provider: Provider::Claude, installed: true, version: Some("fake-1.0".into()), connected: Some(true), method: Some(detect::AuthMethod::ApiKey), status_exit_code: Some(0), error: None },
            "settings": settings, "shell": settings.shell, "startInput": input,
            "restartInput": RestartInput { primary_token: "revision".into(), primary_name: "revision_01.md".into(), supporting_tokens: vec!["source".into()], supporting_names: vec!["source.csv".into()] },
            "stagedFile": StagedFile { token: "primary".into(), name: "input.md".into(), size: 42, kind: "md".into(), chars: 40, error: None },
            "exportResult": ExportResult { path: "wire-workspace/export".into(), tree: vec!["final/final_document.md".into()] },
            "appInfo": AppInfo { version: env!("CARGO_PKG_VERSION"), os: "fixture-os", arch: "fixture-arch" },
            "events": events,
        });
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/services/wire.fixture.json");
        let actual = format!("{}\n", serde_json::to_string_pretty(&wire).unwrap());
        if std::env::var("UPDATE_WIRE_FIXTURE").as_deref() == Ok("1") {
            fs::write(&path, &actual).unwrap();
        }
        assert_eq!(actual, fs::read_to_string(path).unwrap());
    }
    #[test]
    fn settings_policy_round_trips_and_rejects_invalid_bounds() {
        let w = Workspace::new();
        let settings = Settings {
            workspace: w.0.clone(),
            shell: Shell::default(),
            author: Provider::Codex,
            policy: Policy::default(),
        };
        let wire = serde_json::to_value(&settings).unwrap();
        assert_eq!(wire["policy"]["max_rounds"], 5);
        let restored: Settings = serde_json::from_value(wire).unwrap();
        assert_eq!(restored.policy, settings.policy);
        assert_ne!(restored.author, restored.author.other());
        let mut policy = restored.policy;
        policy.min_rounds = 6;
        assert!(policy.validate().is_err());
    }
    #[cfg(windows)]
    #[test]
    fn committed_publication_failure_reaches_command_wire_and_return_launches() {
        use std::os::windows::fs::OpenOptionsExt;
        use std::sync::atomic::AtomicBool;
        use std::time::Instant;
        let _environment = PROVIDER_ENV.lock().unwrap();
        std::env::set_var("SYMVOULIO_FAKE_CLI", fake_cli());
        for kind in [DecisionKind::Approved, DecisionKind::Returned] {
            let mut proved = false;
            for attempt in 0..30 {
                let w = Workspace::new();
                let state = test_state(&w);
                let file = stage(&w.0, "primary.md".into(), b"# Original".to_vec()).unwrap();
                let started = state.start(StartInput {
                    name: "Publication boundary".into(), objective: "Review".into(),
                    criteria: vec!["Accuracy".into()], policy: Policy { max_rounds: 5, new_content_limit_pct: 100.0, ..Policy::default() },
                    author: Provider::Claude, primary_token: file.token, supporting_tokens: vec![],
                }, None, None).unwrap();
                let worker = lock(&state.runs).unwrap().get_mut(&started.id).unwrap().worker.take().unwrap();
                worker.join().unwrap();
                let dir = state.dir(&started.id).unwrap();
                *lock(&lock(&state.runs).unwrap().get(&started.id).unwrap().terminal_error).unwrap() = Some("old storage error".into());
                let rounds_before = snapshot(&dir).unwrap().rounds_done;
                let name = format!("Publication-Fault-{attempt}");
                let target_name = name.clone();
                let path = dir.clone();
                let done = Arc::new(AtomicBool::new(false));
                let stopped = done.clone();
                let watcher = std::thread::spawn(move || {
                    let staged = path.join(".publication/output/final/human_decision.json");
                    let until = Instant::now() + Duration::from_secs(30);
                    while !stopped.load(Ordering::SeqCst) && Instant::now() < until {
                        if fs::read_to_string(&staged).is_ok_and(|s| s.contains(&target_name)) {
                            // Native sharing violation: reads allowed, rename/delete denied.
                            if let Ok(held) = fs::OpenOptions::new().read(true).share_mode(3).open(path.join("output/final/human_decision.json")) {
                                while path.join(".publication").exists() && Instant::now() < until { std::thread::yield_now(); }
                                drop(held);
                                return true;
                            }
                        }
                        std::thread::yield_now();
                    }
                    false
                });
                let result = state.decide(started.id.clone(), kind, name.clone(), "saved".into(), None);
                done.store(true, Ordering::SeqCst);
                let injected = watcher.join().unwrap();
                let result = result.unwrap();
                if kind == DecisionKind::Returned {
                    let worker = lock(&state.runs).unwrap().get_mut(&started.id).unwrap().worker.take().expect("Return must launch a worker");
                    worker.join().unwrap();
                }
                if injected && result.publication == "incomplete" {
                    let wire = serde_json::to_value(&result).unwrap();
                    assert_eq!(wire["publication"], "incomplete");
                    assert_eq!(wire["decision"]["name"], name);
                    assert!(lock(&lock(&state.runs).unwrap().get(&started.id).unwrap().terminal_error).unwrap().is_none());
                    let saved = state.get(&started.id).unwrap();
                    if kind == DecisionKind::Returned {
                        assert_eq!(result.state, RunState::Running);
                        assert_eq!(saved.rounds_done, rounds_before + 1);
                    } else { assert_eq!(saved.state, RunState::Approved); }
                    proved = true;
                    break;
                }
            }
            assert!(proved, "native post-commit publication fault never reached command wire: {kind:?}");
        }
    }

    #[test]
    fn fake_cli_run_decision_and_export() {
        let _environment = PROVIDER_ENV.lock().unwrap();
        let fake = fake_cli();
        assert!(fake.is_file());
        // Every provider call in this test goes to the fake; real CLIs are forbidden in tests.
        std::env::set_var("SYMVOULIO_FAKE_CLI", &fake);
        let w = Workspace::new();
        let state = AppState {
            settings: Mutex::new(Settings {
                workspace: w.0.clone(),
                shell: Shell::default(),
                author: Provider::Claude,
                policy: Policy::default(),
            }),
            runs: Mutex::new(HashMap::new()),
            config: w.0.join("settings.json"),
            raw_output: std::sync::atomic::AtomicBool::new(false),
        };
        let file = stage(
            &w.0,
            "business.md".into(),
            b"# Northwind\nCapital cost $4.2M. Growth 18 %. owner to be confirmed by Operations"
                .to_vec(),
        )
        .unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let channel = Channel::new(move |event| {
            let _ = tx.send(event);
            Ok(())
        });
        let started = state
            .start(
                StartInput {
                    name: "Northwind".into(),
                    objective: "Improve for executive review".into(),
                    criteria: vec!["All totals must reconcile.".into()],
                    // The fake revisions exceed 10 % new text on this tiny document (R11 would keep the run going).
                    policy: Policy {
                        new_content_limit_pct: 100.0,
                        ..Policy::default()
                    },
                    author: Provider::Claude,
                    primary_token: file.token,
                    supporting_tokens: vec![],
                },
                Some(channel),
                None,
            )
            .unwrap();
        assert_eq!(
            fs::read_dir(w.0.join("staging")).unwrap().count(),
            0,
            "successful start retained staging copy"
        );
        let mut completed = false;
        let mut rounds = 0;
        while let Ok(body) = rx.recv_timeout(Duration::from_secs(30)) {
            let tauri::ipc::InvokeResponseBody::Json(json) = body else {
                panic!("JSON event expected")
            };
            let value: serde_json::Value = serde_json::from_str(&json).unwrap();
            if value["kind"] == "roundCompleted" {
                rounds += 1;
            }
            if value["kind"] == "state" && value["state"] == "completed" {
                completed = true;
                break;
            }
        }
        assert!(completed);
        assert_eq!(rounds, 3);
        let handle = lock(&state.runs).unwrap().remove(&started.id).unwrap();
        handle.worker.unwrap().join().unwrap();
        let dir = state.dir(&started.id).unwrap();
        assert_eq!(snapshot(&dir).unwrap().rounds_done, 3);
        assert!(state
            .decide(
                started.id.clone(),
                DecisionKind::Approved,
                " ".into(),
                "".into(),
                None
            )
            .is_err());
        let approved = state
            .decide(
                started.id.clone(),
                DecisionKind::Approved,
                "Reviewer".into(),
                "Approved".into(),
                None,
            )
            .unwrap();
        assert_eq!(approved.state, RunState::Approved);
        let exported = export(&dir, false).unwrap();
        let mut expected = vec![
            "original/business.md".to_string(),
            "audit/run_manifest.json".into(),
            "audit/timeline.json".into(),
        ];
        for name in [
            "final_document.docx",
            "final_document.md",
            "final_review.html",
            "human_decision.json",
            "unresolved_issues.json",
        ] {
            expected.push(format!("final/{name}"));
        }
        for n in 1..=3 {
            for ext in ["md", "docx"] {
                expected.push(format!("revisions/revision_{n:02}.{ext}"));
            }
            expected.push(format!("reviews/review_{n:02}.json"));
            for role in ["author", "reviewer"] {
                {
                    let ext = "jsonl";
                    expected.push(format!(
                        "audit/provider_output/round_{n:02}_{role}_{}_a01.{ext}",
                        if role == "author" { "claude" } else { "codex" }
                    ));
                }
            }
            expected.push(format!("reviews/author_response_{n:02}.json"));
        }
        expected.sort();
        assert_eq!(exported.tree, expected);
        assert!(export(&dir, true).is_err());
        // Returning a finished run starts exactly one additional round immediately.
        let returned = state
            .decide(
                started.id.clone(),
                DecisionKind::Returned,
                "Reviewer".into(),
                "Check once more".into(),
                None,
            )
            .unwrap();
        assert_eq!(returned.state, RunState::Running);
        let handle = lock(&state.runs).unwrap().remove(&started.id).unwrap();
        handle.worker.unwrap().join().unwrap();
        let saved = snapshot(&dir).unwrap();
        assert_eq!(saved.rounds_done, 4);
        assert_eq!(saved.state, RunState::Completed);
    }
}
