// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! Durable, independently reviewed round loop.
use crate::{ledger::HumanIssueDecision, prompts::*, provider::*, *};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
type Clients = (Arc<dyn ProviderClient>, Arc<dyn ProviderClient>);
/// The round a run has reached: completed rounds, plus the round in progress when its revision is already written.
fn reached_round(s: &Saved) -> u32 {
    s.snapshot.rounds_done + u32::from(s.pending.is_some())
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunConfig {
    pub run_id: String,
    pub name: String,
    pub run_dir: PathBuf,
    pub author: Provider,
    pub objective: String,
    pub criteria: Vec<String>,
    pub policy: Policy,
    pub primary: PathBuf,
    pub supporting: Vec<PathBuf>,
    pub call_timeout: Duration,
    pub author_model: Option<String>,
    pub reviewer_model: Option<String>,
}
pub type RawObserver = Arc<dyn Fn(&str) + Send + Sync>;
#[derive(Default)]
pub struct RunControl {
    pub pause: AtomicBool,
    pub stop: AtomicBool,
    pub cancel: CancelToken,
    pub raw_output: AtomicBool,
    pub tool_observer: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Evidence-only callback; product IPC always receives redacted lines.
    pub raw_observer: Option<RawObserver>,
    termination: Mutex<()>,
}
impl RunControl {
    pub fn stop(&self) {
        let _termination = self.termination.lock().unwrap_or_else(|e| e.into_inner());
        self.stop.store(true, Ordering::SeqCst);
        self.cancel.store(true, Ordering::SeqCst);
    }
}
#[derive(Debug)]
pub enum RunEvent {
    Line {
        provider: Provider,
        role: Role,
        line: StreamLine,
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
        kind: String,
        message: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastError {
    pub provider: Option<Provider>,
    pub kind: String,
    pub message: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundSnapshot {
    pub round: u32,
    pub gate: Gate,
    pub new_text_pct: f64,
    pub contribution_exceeded: bool,
    pub author_usage: Option<Usage>,
    pub reviewer_usage: Option<Usage>,
    pub author_model: Option<String>,
    pub reviewer_model: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueStep {
    pub round: u32,
    pub status: Status,
    pub severity: Severity,
    pub finding: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorReply {
    pub round: u32,
    pub response: IssueResponse,
    pub explanation: String,
    pub evidence: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueSnapshot {
    pub issue: Issue,
    pub raised_in: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_on_arrival: Option<u32>,
    pub history: Vec<IssueStep>,
    pub author_responses: Vec<AuthorReply>,
    pub human_decision: Option<HumanIssueDecision>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSnapshot {
    #[serde(default)]
    pub generation: u64,
    #[serde(default)]
    pub revision: u64,
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub author: Provider,
    pub reviewer: Provider,
    pub policy: Policy,
    pub objective: String,
    pub criteria: Vec<String>,
    pub state: RunState,
    pub stop_reason: Option<StopReason>,
    pub last_error: Option<LastError>,
    #[serde(default)]
    pub phase: Option<Role>,
    #[serde(default)]
    pub pending_revision: Option<u32>,
    pub rounds_done: u32,
    pub documents: Vec<String>,
    pub rounds: Vec<RoundSnapshot>,
    pub issues: Vec<IssueSnapshot>,
    #[serde(default)]
    pub human_questions: Vec<ledger::HumanQuestionRecord>,
    pub counts: Counts,
    pub decision: Option<HumanDecision>,
    pub decision_log: Vec<HumanDecision>,
    pub output_tree: Vec<String>,
    pub primary_name: String,
    pub supporting_names: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub id: String,
    pub name: String,
    pub state: RunState,
    pub rounds_done: u32,
    pub created_at: String,
    pub run_dir: PathBuf,
}
#[derive(Clone, Serialize, Deserialize)]
struct Pending {
    usage: Option<Usage>,
    model: Option<String>,
    new_text_pct: f64,
}
#[derive(Clone, Serialize, Deserialize)]
struct Saved {
    config: RunConfig,
    snapshot: RunSnapshot,
    ledger: IssueLedger,
    timeline: Vec<TimelineEvent>,
    manifest: RunManifest,
    supporting: Vec<(String, String)>,
    last_review: Option<Review>,
    pending: Option<Pending>,
    returned_until: Option<u32>,
}
fn invalid(s: impl Into<String>) -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(io::Error::other(s.into()))
}
fn name(p: &Path) -> Result<String> {
    p.file_name()
        .and_then(|s| s.to_str())
        .map(str::to_owned)
        .ok_or_else(|| invalid("input name must be UTF-8"))
}
fn cli_version(provider: Provider, cancel: &CancelToken) -> Option<String> {
    let resolved = cli::resolve(provider)?;
    let mut cmd = process::command(&resolved);
    cmd.arg("--version");
    let output = process::run(
        cmd,
        String::new(),
        Duration::from_secs(20),
        cancel,
        &mut |_| {},
    )
    .ok()?;
    (output.exit_code == Some(0))
        .then(|| crate::redact::version(&output.stdout.join("\n")))
        .flatten()
}
fn package(s: &Saved) -> OutputPackage {
    OutputPackage::reopen(&s.config.run_dir, s.snapshot.documents[0].clone())
}
fn event(
    s: &mut Saved,
    kind: TimelineKind,
    round: Option<u32>,
    role: Option<Role>,
    provider: Option<Provider>,
    detail: impl Into<String>,
) {
    let started = matches!(
        kind,
        TimelineKind::AuthorStarted | TimelineKind::ReviewStarted
    );
    let previous = role.zip(provider).and_then(|(role, provider)| {
        s.timeline.iter().rev().find(|e| {
            e.role == Some(role)
                && e.provider == Some(provider)
                && matches!(
                    e.kind,
                    TimelineKind::AuthorStarted | TimelineKind::ReviewStarted
                )
        })
    });
    let attempt = if started {
        Some(
            s.timeline
                .iter()
                .filter(|e| {
                    e.round == round
                        && e.role == role
                        && e.provider == provider
                        && matches!(
                            e.kind,
                            TimelineKind::AuthorStarted | TimelineKind::ReviewStarted
                        )
                })
                .map(|e| e.attempt.unwrap_or(1))
                .max()
                .unwrap_or(0)
                + 1,
        )
    } else {
        previous.and_then(|e| e.attempt)
    };
    let round = round.or_else(|| previous.and_then(|e| e.round));
    s.timeline.push(TimelineEvent {
        attempt,
        ts: now_utc_rfc3339(),
        kind,
        round,
        role,
        provider,
        detail: redact(&detail.into()),
        duration_s: None,
        usage: None,
    });
}
// Serialize publication and read/repair so a reader cannot remove a writer's stage.
static PUBLICATION: Mutex<()> = Mutex::new(());

fn save(s: &mut Saved) -> Result<()> {
    match save_before_commit(s, |_| Ok(())) {
        // A returned run must keep executing after its state is durable, even while
        // the derived package is obstructed. Later saves and Export repair it.
        Err(error) if s.snapshot.state == RunState::Running
            && s.snapshot.decision.as_ref().is_some_and(|d| d.kind == DecisionKind::Returned)
            && error.downcast_ref::<PublicationIncomplete>().is_some() => Ok(()),
        result => result,
    }
}
fn save_before_commit(
    s: &mut Saved,
    before_commit: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    save_publication(s, before_commit, |_, _| Ok(()))
}
#[derive(Clone, Copy, Debug)]
enum PublicationStep { Final, Audit, Cleanup }
fn save_publication(
    s: &mut Saved,
    before_commit: impl FnOnce(&Path) -> Result<()>,
    mut publish: impl FnMut(PublicationStep, &Path) -> Result<()>,
) -> Result<()> {
    let _publication = PUBLICATION.lock().unwrap_or_else(|e| e.into_inner());
    s.snapshot.issues = s
        .ledger
        .histories()
        .filter_map(|h| {
            h.reviews.last().map(|(_, issue)| IssueSnapshot {
                issue: issue.clone(),
                raised_in: h.first_round,
                resolved_on_arrival: h.resolved_on_arrival(),
                history: h
                    .reviews
                    .iter()
                    .map(|(round, i)| IssueStep {
                        round: *round,
                        status: i.status,
                        severity: i.severity,
                        finding: i.finding.clone(),
                    })
                    .collect(),
                author_responses: h
                    .responses
                    .iter()
                    .map(|(round, r)| AuthorReply {
                        round: *round,
                        response: r.response,
                        explanation: r.explanation.clone(),
                        evidence: r.evidence.clone(),
                    })
                    .collect(),
                human_decision: h.decision.clone(),
            })
        })
        .collect();
    s.snapshot.pending_revision = s.pending.as_ref().map(|_| s.snapshot.rounds_done + 1);
    s.snapshot.human_questions = s.ledger.human_questions().to_vec();
    s.snapshot.counts = s.ledger.open_counts();
    s.manifest.updated_at = now_utc_rfc3339();
    s.manifest.state = s.snapshot.state;
    s.manifest.stop_reason = s.snapshot.stop_reason;
    s.manifest.rounds_completed = s.snapshot.rounds_done;
    s.manifest.human_decision = s.snapshot.decision.clone();
    // Build derived artifacts outside the published output tree. A failed state
    // commit cannot expose an uncommitted decision.
    let stage = s.config.run_dir.join(".publication");
    if stage.exists() {
        fs::remove_dir_all(&stage)?;
    }
    let mut committed = false;
    let result: Result<()> = (|| {
        let mut staged = s.clone();
        staged.config.run_dir = stage.clone();
        for dir in ["output/revisions", "output/final", "output/audit"] {
            fs::create_dir_all(stage.join(dir))?;
        }
        // Retain the staged revision companion for publication cleanup, but
        // derive it from saved state rather than the editable output sidecar.
        if s.snapshot.rounds_done > 0 {
            let revision = s.snapshot.rounds_done;
            let markdown = s.snapshot.documents.get(revision as usize).ok_or(
                RevisionIntegrity { revision, reason: "missing from saved state" }
            )?;
            fs::write(stage.join(format!("output/revisions/revision_{revision:02}.md")), markdown)?;
        }
        package(&staged).write_manifest(&s.manifest)?;
        package(&staged).write_timeline(&s.timeline)?;
        write_report(&staged)?;
        let mut tree = package(s).tree();
        tree.extend(package(&staged).tree());
        tree.sort();
        tree.dedup();
        s.snapshot.output_tree = tree;
        s.snapshot.revision += 1;
        before_commit(&stage)?;
        let path = s.config.run_dir.join("run_state.json");
        let tmp = path.with_extension("json.tmp");
        let mut file = fs::File::create(&tmp)?;
        let mut persisted = serde_json::to_value(&*s)?;
        persisted["config"]["run_dir"] = serde_json::json!(".");
        persisted["config"]["primary"] =
            serde_json::json!(format!("inputs/{}", s.snapshot.primary_name));
        persisted["config"]["supporting"] = serde_json::json!(s
            .snapshot
            .supporting_names
            .iter()
            .map(|name| format!("inputs/supporting/{name}"))
            .collect::<Vec<_>>());
        file.write_all(&serde_json::to_vec_pretty(&persisted)?)?;
        file.sync_all()?;
        drop(file);
        fs::rename(tmp, path)?;
        committed = true;
        publish(PublicationStep::Final, &stage)?;
        publish_directory(
            &stage.join("output/final"),
            &s.config.run_dir.join("output/final"),
        )?;
        publish(PublicationStep::Audit, &stage)?;
        publish_directory(
            &stage.join("output/audit"),
            &s.config.run_dir.join("output/audit"),
        )?;
        Ok(())
    })();
    // Staging may contain names and notes that never reached committed state.
    let cleanup = publish(PublicationStep::Cleanup, &stage).and_then(|_| fs::remove_dir_all(&stage).map_err(Into::into));
    if let Err(error) = result.and(cleanup) {
        if committed {
            // Discard staged private payloads even if a concurrent obstruction prevents
            // removing the staging directory itself.
            for dir in ["output/final", "output/audit"] {
                let _ = fs::remove_dir_all(stage.join(dir));
            }
            return Err(Box::new(PublicationIncomplete { snapshot: s.snapshot.clone() }));
        }
        return Err(error);
    }
    Ok(())
}
/// A durable state whose derived package still needs repair.
#[derive(Debug)]
pub struct PublicationIncomplete {
    /// Authoritative committed snapshot, including the human decision.
    pub snapshot: RunSnapshot,
}
impl std::fmt::Display for PublicationIncomplete {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.snapshot.decision.is_some() {
            write!(f, "decision committed, publication incomplete; decision saved; package will be repaired")
        } else {
            write!(f, "state committed, publication incomplete; package will be repaired")
        }
    }
}
impl std::error::Error for PublicationIncomplete {}
// Rename each fully written derived file only after the authoritative state commits.
fn publish_directory(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        fs::rename(entry.path(), to.join(entry.file_name()))?;
    }
    Ok(())
}
fn repair_artifacts(s: &Saved) -> Result<()> {
    let stage = s.config.run_dir.join(".publication");
    if stage.exists() {
        fs::remove_dir_all(stage)?;
    }
    package(s).write_manifest(&s.manifest)?;
    package(s).write_timeline(&s.timeline)?;
    write_report(s)
}
fn write_report(s: &Saved) -> Result<()> {
    write_report_package(s, &package(s))
}
/// A saved approval cannot authorize publication of different revision text.
#[derive(Debug, thiserror::Error)]
#[error("publication integrity: revision {revision:02} {reason}")]
struct RevisionIntegrity {
    revision: u32,
    reason: &'static str,
}
fn write_report_package(s: &Saved, p: &OutputPackage) -> Result<()> {
    let revision = s.manifest.rounds_completed;
    let markdown = s.snapshot.documents.get(revision as usize).ok_or(
        RevisionIntegrity { revision, reason: "missing from saved state" }
    )?;
    for decision in s.snapshot.decision.iter().chain(s.manifest.human_decision.iter()) {
        if decision.kind == DecisionKind::Approved {
            use sha2::{Digest, Sha256};
            if decision.revision != revision
                || decision.document_sha256 != format!("{:x}", Sha256::digest(markdown.as_bytes()))
            {
                return Err(Box::new(RevisionIntegrity {
                    revision, reason: "does not match saved approval",
                }));
            }
        }
    }
    p.write_final_document(
        &s.manifest,
        &s.ledger,
        s.last_review.as_ref(),
        &s.snapshot
            .rounds
            .iter()
            .map(|r| (r.round, r.gate.clone()))
            .collect::<Vec<_>>(),
        markdown,
    )?;
    p.write_contribution_warnings(
        &s.snapshot
            .rounds
            .iter()
            .filter(|r| r.contribution_exceeded)
            .map(|r| (r.round, r.new_text_pct))
            .collect::<Vec<_>>(),
        s.config.policy.new_content_limit_pct,
    )?;
    Ok(())
}
/// Regenerate the same complete final report used by durable engine saves.
pub fn export_report(run_dir: impl AsRef<Path>) -> Result<()> {
    reconcile(run_dir.as_ref(), true).map(|_| ())
}
// Render without touching disk, then compare every derived artifact. ZIP metadata
// can vary between DOCX generations; compare every uncompressed package member.
fn artifacts_agree(s: &Saved) -> Result<bool> {
    if s.config.run_dir.join(".publication").exists() { return Ok(false); }
    let expected = OutputPackage::inspection(&s.config.run_dir, s.snapshot.documents[0].clone());
    expected.write_manifest(&s.manifest)?;
    expected.write_timeline(&s.timeline)?;
    write_report_package(s, &expected)?;
    for (relative, bytes) in expected.captured() {
        let actual = match fs::read(s.config.run_dir.join("output").join(&relative)) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        if relative.extension().is_some_and(|e| e == "docx") {
            if docx_parts(&actual).ok() != Some(docx_parts(&bytes)?) { return Ok(false); }
        } else if actual != bytes { return Ok(false); }
    }
    Ok(true)
}
fn docx_parts(bytes: &[u8]) -> Result<std::collections::BTreeMap<String, Vec<u8>>> {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    let mut parts = std::collections::BTreeMap::new();
    for i in 0..zip.len() {
        let mut part = zip.by_index(i)?;
        let mut bytes = Vec::new();
        part.read_to_end(&mut bytes)?;
        parts.insert(part.name().to_owned(), bytes);
    }
    Ok(parts)
}
fn reconcile(path: &Path, force: bool) -> Result<Saved> {
    let _publication = PUBLICATION.lock().unwrap_or_else(|e| e.into_inner());
    // Derived artifacts reflect the persisted ledger, before display-only legacy
    // question ID migration. Repair from that same state so subsequent reads
    // still agree until a durable save publishes the migrated IDs.
    let mut s = read_persisted(path)?;
    // Once state is readable, inspection and publication failures must carry it
    // back to callers, including when no human decision has yet been made.
    let result: Result<()> = (|| {
        if !force && artifacts_agree(&s)? { return Ok(()); }
        repair_artifacts(&s)
    })();
    migrate_question_ids(&mut s);
    if let Err(error) = result {
        if error.downcast_ref::<RevisionIntegrity>().is_some() {
            return Err(error);
        }
        return Err(Box::new(PublicationIncomplete { snapshot: s.snapshot }));
    }
    Ok(s)
}
fn migrate_question_ids(saved: &mut Saved) {
    saved.ledger.migrate_question_ids();
    saved.snapshot.human_questions = saved.ledger.human_questions().to_vec();
}
fn read(path: &Path) -> Result<Saved> {
    let mut saved = read_persisted(path)?;
    migrate_question_ids(&mut saved);
    Ok(saved)
}
fn read_persisted(path: &Path) -> Result<Saved> {
    let mut saved: Saved = serde_json::from_slice(&fs::read(path.join("run_state.json"))?)?;
    saved.config.run_dir = fs::canonicalize(path)?;
    // Rebuild from the durable input copies, also migrating older absolute-path saves.
    saved.config.primary = saved
        .config
        .run_dir
        .join("inputs")
        .join(&saved.snapshot.primary_name);
    saved.config.supporting = saved
        .snapshot
        .supporting_names
        .iter()
        .map(|name| saved.config.run_dir.join("inputs/supporting").join(name))
        .collect();
    saved.snapshot.pending_revision = saved
        .pending
        .as_ref()
        .map(|_| saved.snapshot.rounds_done + 1);
    if saved.snapshot.state != RunState::Running {
        saved.snapshot.phase = None;
    }
    Ok(saved)
}
fn state(
    s: &mut Saved,
    value: RunState,
    reason: Option<StopReason>,
    on: &impl Fn(RunEvent),
) -> Result<()> {
    s.snapshot.phase = None;
    s.snapshot.state = value;
    s.snapshot.stop_reason = reason;
    save(s)?;
    on(RunEvent::State {
        state: value,
        stop_reason: reason,
    });
    Ok(())
}
struct CreationGuard {
    path: PathBuf,
    owned: bool,
}
impl Drop for CreationGuard {
    fn drop(&mut self) {
        if self.owned && !self.path.join("run_state.json").exists() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
pub fn start(
    mut cfg: RunConfig,
    clients: Clients,
    control: Arc<RunControl>,
    on_event: impl Fn(RunEvent),
) -> Result<RunSnapshot> {
    if clients.0.provider() == clients.1.provider() {
        return Err(invalid("No model grades its own work."));
    }
    if clients.0.provider() != cfg.author || clients.1.provider() != cfg.author.other() {
        return Err(invalid("provider assignments do not match configuration"));
    }
    cfg.policy.validate().map_err(invalid)?;
    if cfg.run_dir.join("run_state.json").exists() {
        return Err(invalid("run already exists"));
    }
    let original = extract(&cfg.primary)?;
    if original.kind == DocumentKind::Csv {
        return Err(invalid("CSV is supporting only"));
    }
    if original.text.trim().is_empty() {
        return Err(documents::DocumentError::EmptyPrimary.into());
    }
    let mut supporting = vec![];
    let mut inputs = vec![InputFile {
        file_name: name(&cfg.primary)?,
        role: InputRole::Primary,
        bytes: original.bytes,
        sha256: original.sha256.clone(),
        extracted_chars: original.text.chars().count() as u64,
    }];
    for p in &cfg.supporting {
        let e = extract(p)?;
        supporting.push((name(p)?, e.text.clone()));
        inputs.push(InputFile {
            file_name: name(p)?,
            role: InputRole::Supporting,
            bytes: e.bytes,
            sha256: e.sha256,
            extracted_chars: e.text.chars().count() as u64,
        });
    }
    let total = original.text.len() + supporting.iter().map(|(_, text)| text.len()).sum::<usize>();
    if total > documents::MAX_TEXT_BYTES {
        let mut files = vec![format!(
            "{} ({} bytes)",
            name(&cfg.primary)?,
            original.text.len()
        )];
        files.extend(
            supporting
                .iter()
                .map(|(name, text)| format!("{name} ({} bytes)", text.len())),
        );
        return Err(invalid(format!(
            "Extracted inputs exceed 8 MB: {}",
            files.join(", ")
        )));
    }
    let _creation = CreationGuard {
        path: cfg.run_dir.clone(),
        owned: !cfg.run_dir.exists(),
    };
    OutputPackage::create(&cfg.run_dir, &cfg.primary, &cfg.supporting)?;
    cfg.run_dir = fs::canonicalize(&cfg.run_dir)?;
    fs::create_dir(cfg.run_dir.join("inputs"))?;
    fs::copy(
        &cfg.primary,
        cfg.run_dir.join("inputs").join(name(&cfg.primary)?),
    )?;
    fs::create_dir(cfg.run_dir.join("inputs/supporting"))?;
    for p in &cfg.supporting {
        fs::copy(p, cfg.run_dir.join("inputs/supporting").join(name(p)?))?;
    }
    for role in ["author", "reviewer"] {
        fs::create_dir_all(cfg.run_dir.join("workspaces").join(role).join(".symvoulio"))?;
    }
    let at = now_utc_rfc3339();
    let manifest = RunManifest {
        schema_version: 1,
        run_id: cfg.run_id.clone(),
        name: cfg.name.clone(),
        created_at: at.clone(),
        updated_at: at.clone(),
        os: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        app_version: env!("CARGO_PKG_VERSION").into(),
        author: Participant {
            provider: cfg.author,
            cli_version: cli_version(cfg.author, &control.cancel),
            model: None,
        },
        reviewer: Participant {
            provider: cfg.author.other(),
            cli_version: cli_version(cfg.author.other(), &control.cancel),
            model: None,
        },
        inputs,
        objective: cfg.objective.clone(),
        criteria: cfg.criteria.clone(),
        policy: cfg.policy.clone(),
        rounds_completed: 0,
        state: RunState::Running,
        stop_reason: None,
        human_decision: None,
    };
    let snapshot = RunSnapshot {
        generation: 1,
        revision: 0,
        id: cfg.run_id.clone(),
        name: cfg.name.clone(),
        created_at: at,
        author: cfg.author,
        reviewer: cfg.author.other(),
        policy: cfg.policy.clone(),
        objective: cfg.objective.clone(),
        criteria: cfg.criteria.clone(),
        state: RunState::Running,
        stop_reason: None,
        last_error: None,
        phase: None,
        pending_revision: None,
        rounds_done: 0,
        documents: vec![original.text],
        rounds: vec![],
        issues: vec![],
        human_questions: vec![],
        counts: Counts::default(),
        decision: None,
        decision_log: vec![],
        output_tree: vec![],
        primary_name: name(&cfg.primary)?,
        supporting_names: supporting.iter().map(|(n, _)| n.clone()).collect(),
    };
    let mut s = Saved {
        config: cfg,
        snapshot,
        manifest,
        ledger: IssueLedger::default(),
        timeline: vec![],
        supporting,
        last_review: None,
        pending: None,
        returned_until: None,
    };
    event(
        &mut s,
        TimelineKind::RunStarted,
        None,
        None,
        None,
        "Run started",
    );
    save(&mut s)?;
    on_event(RunEvent::State {
        state: RunState::Running,
        stop_reason: None,
    });
    drive(s, clients, control, on_event)
}
fn check(s: &mut Saved, c: &RunControl, on: &impl Fn(RunEvent)) -> Result<bool> {
    if c.stop.load(Ordering::SeqCst) || c.cancel.load(Ordering::SeqCst) {
        event(s, TimelineKind::Stopped, None, None, None, "User stop");
        state(s, RunState::Stopped, Some(StopReason::UserStop), on)?;
        return Ok(true);
    }
    if c.pause.load(Ordering::SeqCst) {
        event(
            s,
            TimelineKind::Paused,
            None,
            None,
            None,
            "Paused before next provider call",
        );
        state(s, RunState::Paused, None, on)?;
        return Ok(true);
    }
    Ok(false)
}
fn provider_error(
    s: &mut Saved,
    e: CallError,
    p: Provider,
    role: Role,
    on: &impl Fn(RunEvent),
) -> Result<()> {
    if matches!(e, CallError::Cancelled) {
        event(
            s,
            TimelineKind::Stopped,
            None,
            Some(role),
            Some(p),
            "User stop: active call cancelled",
        );
        return state(s, RunState::Stopped, Some(StopReason::UserStop), on);
    }
    let kind = match &e {
        CallError::InputTooLarge(_) => "input_too_large",
        CallError::AuthFailed(_) => "auth",
        CallError::Timeout => "timeout",
        CallError::Malformed(_) => "malformed",
        CallError::ProcessFailed { .. } => "process",
        CallError::Io(_) => "io",
        CallError::NotInstalled => "not_installed",
        CallError::Cancelled => unreachable!(),
    }
    .to_owned();
    // Error variants carry arbitrary provider bodies; only controlled descriptions leave this boundary.
    let message = match &e {
        CallError::InputTooLarge(_) => format!(
            "Input exceeds the prompt limit. Primary: {}; supporting: {}",
            s.snapshot.primary_name,
            s.snapshot.supporting_names.join(", ")
        ),
        CallError::Malformed(detail) => {
            let controlled = validation_reason(detail);
            let reason = controlled.as_deref().unwrap_or_else(|| [
                "duplicate issue id",
                "issue_id",
                "missing response",
                "non-empty evidence",
                "duplicate or unknown response id",
                "conflicting question id",
                "finding conflicts with question id",
            ]
            .into_iter()
            .find(|reason| detail.contains(reason))
            .unwrap_or("invalid structured output"));
            // Only the schema-shaped id may accompany the controlled rejection reason.
            let question_id = detail
                .strip_prefix("conflicting question id: ")
                .filter(|id| {
                    regex::Regex::new(r"^R[0-9]{2}-[0-9]{3}$")
                        .unwrap()
                        .is_match(id)
                });
            if let Some(id) = question_id {
                format!("Invalid structured output: conflicting question id {id}. Diagnostic body omitted.")
            } else {
                format!("Invalid structured output: {reason}. Diagnostic body omitted.")
            }
        }
        _ => format!("Provider {kind} failure. Diagnostic body omitted."),
    };
    s.snapshot.last_error = Some(LastError {
        provider: Some(p),
        kind: kind.clone(),
        message: message.clone(),
    });
    event(
        s,
        TimelineKind::ProviderError,
        None,
        Some(role),
        Some(p),
        format!("{kind}: {message}"),
    );
    on(RunEvent::ProviderError {
        provider: p,
        role,
        kind,
        message,
    });
    state(s, RunState::Paused, Some(StopReason::ProviderError), on)
}
fn drive(
    s: Saved,
    clients: Clients,
    control: Arc<RunControl>,
    on: impl Fn(RunEvent),
) -> Result<RunSnapshot> {
    let path = s.config.run_dir.clone();
    let author = s.config.author;
    let result = drive_inner(s, clients, control.clone(), &on);
    let result = match result {
        Ok(snapshot) => Ok(snapshot),
        Err(error) => {
            let message = "Storage operation failed. Check the workspace and retry.".to_owned();
            on(RunEvent::ProviderError {
                provider: author,
                role: Role::Author,
                kind: "storage".into(),
                message: message.clone(),
            });
            let recovered = (|| {
                let mut saved = read(&path)?;
                saved.snapshot.last_error = Some(LastError {
                    provider: Some(author),
                    kind: "storage".into(),
                    message,
                });
                event(
                    &mut saved,
                    TimelineKind::ProviderError,
                    None,
                    Some(Role::Author),
                    Some(author),
                    "Storage operation failed",
                );
                saved.snapshot.phase = None;
                saved.snapshot.state = RunState::Paused;
                saved.snapshot.stop_reason = None;
                save(&mut saved)?;
                Ok(saved.snapshot)
            })();
            on(RunEvent::State {
                state: RunState::Paused,
                stop_reason: None,
            });
            recovered.map_err(|_: Box<dyn std::error::Error + Send + Sync>| error)
        }
    };
    // A Stop and terminal reconciliation share a lock; neither can leave a late Paused write behind.
    let termination = control
        .termination
        .lock()
        .map_err(|_| invalid("termination lock poisoned"))?;
    let result = if control.stop.load(Ordering::SeqCst)
        && result.as_ref().is_ok_and(|s| s.state == RunState::Paused)
    {
        let snapshot = stop_idle(&path)?;
        Ok(snapshot)
    } else {
        result
    };
    drop(termination);
    if result.as_ref().is_ok_and(|s| s.state == RunState::Stopped)
        && control.stop.load(Ordering::SeqCst)
    {
        on(RunEvent::State {
            state: RunState::Stopped,
            stop_reason: Some(StopReason::UserStop),
        });
    }
    result
}

fn drive_inner(
    mut s: Saved,
    clients: Clients,
    control: Arc<RunControl>,
    on: &impl Fn(RunEvent),
) -> Result<RunSnapshot> {
    loop {
        if check(&mut s, &control, &on)? {
            break;
        }
        // A crash can occur after saving the reviewer step but before saving
        // its terminal state. Honor that already-computed gate on recovery.
        if s.pending.is_none() {
            if let Some(last) = s.snapshot.rounds.last() {
                let returned_done = s
                    .returned_until
                    .is_some_and(|n| s.snapshot.rounds_done >= n);
                let ordinary_done =
                    s.returned_until.is_none() && last.gate.kind != GateKind::Continue;
                if returned_done || ordinary_done {
                    let value = if last.gate.kind == GateKind::Stopped {
                        RunState::Stopped
                    } else {
                        RunState::Completed
                    };
                    let reason = last.gate.reason.stop_reason();
                    s.returned_until = None;
                    state(&mut s, value, reason, &on)?;
                    break;
                }
            }
        }
        let n = s.snapshot.rounds_done + 1;
        let open: Vec<Issue> = s.ledger.open().into_iter().cloned().collect();
        for (role, client) in [(Role::Author, &clients.0), (Role::Reviewer, &clients.1)] {
            if role == Role::Author && s.pending.is_some() {
                continue;
            }
            if check(&mut s, &control, &on)? {
                return Ok(s.snapshot);
            }
            let p = client.provider();
            let author = role == Role::Author;
            let mut prompt = if author {
                author_prompt(
                    n,
                    &s.config.objective,
                    &s.config.criteria,
                    &s.config.policy,
                    &s.snapshot.documents[0],
                    s.snapshot.documents.last().expect("original"),
                    &s.supporting,
                    &open,
                )
            } else {
                reviewer_prompt(
                    n,
                    &s.config.objective,
                    &s.config.criteria,
                    &s.config.policy,
                    &s.snapshot.documents[0],
                    s.snapshot.documents.last().expect("revision"),
                    &s.supporting,
                    &open,
                )
            };
            prompt.push_str(&human_instructions(&s.ledger));
            let label = if author {
                "Creating revision"
            } else {
                "Reviewing revision"
            };
            event(
                &mut s,
                if author {
                    TimelineKind::AuthorStarted
                } else {
                    TimelineKind::ReviewStarted
                },
                Some(n),
                Some(role),
                Some(p),
                format!("{label} {n}"),
            );
            let attempt = s
                .timeline
                .last()
                .and_then(|e| e.attempt)
                .expect("call attempt");
            s.snapshot.phase = Some(role);
            save(&mut s)?;
            on(RunEvent::Phase {
                round: n,
                role,
                provider: p,
                phase: format!("{label} {n}"),
            });
            let req = CallRequest {
                role,
                round: n,
                prompt,
                schema: if author {
                    SchemaKind::AuthorResponse
                } else {
                    SchemaKind::Review
                },
                workdir: s.config.run_dir.join("workspaces").join(if author {
                    "author"
                } else {
                    "reviewer"
                }),
                timeout: s.config.call_timeout,
                model: if author {
                    s.config.author_model.clone()
                } else {
                    s.config.reviewer_model.clone()
                },
            };
            let mut raw_stdout = Vec::new();
            let mut raw_stderr = String::new();
            let result = if req.prompt.len() > MAX_PROMPT_BYTES {
                let mut inputs = vec![(
                    s.snapshot.primary_name.clone(),
                    s.snapshot.documents[0].len(),
                )];
                inputs.extend(
                    s.supporting
                        .iter()
                        .map(|(name, text)| (name.clone(), text.len())),
                );
                inputs.sort_by_key(|(_, size)| std::cmp::Reverse(*size));
                Err(CallError::InputTooLarge(format!(
                    "largest inputs: {}",
                    inputs
                        .iter()
                        .take(5)
                        .map(|(name, size)| format!("{name} ({size} bytes)"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )))
            } else {
                client.call(
                    &req,
                    &mut |mut line| {
                        if is_tool_execution(&line.raw) {
                            if let Some(observer) = &control.tool_observer {
                                observer();
                            }
                        }
                        if let Some(observer) = &control.raw_observer { observer(&line.raw); }
                        let summary = controlled_summary(&line.raw);
                        // Live text is transient; the audit writer persists only allowlisted metadata.
                        line.raw = redact(&line.raw);
                        line.readable = line.readable.map(|text| redact(&text));
                        if line.is_stderr {
                            raw_stderr.push_str(&line.raw);
                            raw_stderr.push('\n');
                        } else {
                            raw_stdout.push(line.raw.clone());
                        }
                        if !control.raw_output.load(Ordering::SeqCst) {
                            line.raw.clear();
                        }
                        line.readable = Some(summary);
                        on(RunEvent::Line {
                            provider: p,
                            role,
                            line,
                        })
                    },
                    &control.cancel,
                )
            };
            s.snapshot.phase = None;
            let mut o = match result {
                Ok(o) => o,
                Err(e) => {
                    package(&s).write_provider_output(
                        n,
                        role,
                        p,
                        attempt,
                        &raw_stdout,
                        &raw_stderr,
                    )?;
                    provider_error(&mut s, e, p, role, &on)?;
                    return Ok(s.snapshot);
                }
            };
            if let Err(e) = normalize_issue_ids(req.schema, &mut o.structured, n)
                .and_then(|_| validate_answer(req.schema, &o.structured, &s.ledger, n)) {
                package(&s).write_provider_output(
                    n,
                    role,
                    p,
                    attempt,
                    &o.stdout_lines,
                    &o.stderr,
                )?;
                provider_error(&mut s, e, p, role, &on)?;
                return Ok(s.snapshot);
            }
            let package = package(&s);
            package.write_provider_output(n, role, p, attempt, &o.stdout_lines, &o.stderr)?;
            if author {
                let response: AuthorResponse = serde_json::from_value(o.structured)?;
                let pct = new_text_pct(
                    s.snapshot.documents.last().expect("document"),
                    &response.revised_document,
                );
                package.write_revision(n, &response.revised_document)?;
                package.write_author_response(n, &response)?;
                s.ledger.apply_author(n, &response);
                s.snapshot.documents.push(response.revised_document);
                s.pending = Some(Pending {
                    usage: o.usage.clone(),
                    model: o.model.clone(),
                    new_text_pct: pct,
                });
                s.manifest.author.model = o.model;
            } else {
                let review: Review = serde_json::from_value(o.structured)?;
                package.write_review(n, &review)?;
                s.ledger.apply_review(n, &review);
                s.last_review = Some(review);
                s.manifest.reviewer.model = o.model.clone();
                let a = s.pending.take().expect("persisted author step");
                let mut gate = gate_after(n, &s.config.policy, &s.ledger);
                let exceeded = a.new_text_pct > s.config.policy.new_content_limit_pct;
                if exceeded {
                    gate.kind = if n >= s.config.policy.max_rounds {
                        GateKind::Stopped
                    } else {
                        GateKind::Continue
                    };
                    gate.reason = if n >= s.config.policy.max_rounds {
                        GateReason::StoppedHuman
                    } else {
                        GateReason::ContinueContribution
                    };
                }
                s.snapshot.rounds.push(RoundSnapshot {
                    round: n,
                    gate: gate.clone(),
                    new_text_pct: a.new_text_pct,
                    contribution_exceeded: exceeded,
                    author_usage: a.usage,
                    reviewer_usage: o.usage.clone(),
                    author_model: a.model,
                    reviewer_model: o.model,
                });
                s.snapshot.rounds_done = n;
            }
            event(
                &mut s,
                if author {
                    TimelineKind::AuthorCompleted
                } else {
                    TimelineKind::ReviewCompleted
                },
                Some(n),
                Some(role),
                Some(p),
                "Completed",
            );
            if let Some(e) = s.timeline.last_mut() {
                e.duration_s = Some(o.duration_s);
                e.usage = o.usage;
            }
            save(&mut s)?;
        }
        let r = s.snapshot.rounds.last().expect("completed round").clone();
        event(
            &mut s,
            TimelineKind::Gate,
            Some(n),
            None,
            None,
            format!("{:?}", r.gate.reason),
        );
        save(&mut s)?;
        on(RunEvent::RoundCompleted {
            round: n,
            gate: r.gate.clone(),
            new_text_pct: r.new_text_pct,
            contribution_exceeded: r.contribution_exceeded,
        });
        if check(&mut s, &control, &on)? {
            break;
        }
        if s.returned_until.is_some_and(|limit| n >= limit) || r.gate.kind != GateKind::Continue {
            s.returned_until = None;
            let value = if r.gate.kind == GateKind::Stopped {
                RunState::Stopped
            } else {
                RunState::Completed
            };
            state(&mut s, value, r.gate.reason.stop_reason(), &on)?;
            break;
        }
    }
    Ok(s.snapshot)
}
/// Read authoritative state and reconcile its derived package without interrupting a run.
pub fn read_snapshot(run_dir: impl AsRef<Path>) -> Result<RunSnapshot> {
    match reconcile(run_dir.as_ref(), false) {
        Ok(s) => Ok(s.snapshot),
        Err(error) => {
            if let Some(incomplete) = error.downcast_ref::<PublicationIncomplete>() {
                let mut snapshot = incomplete.snapshot.clone();
                snapshot.last_error = Some(LastError { provider: None, kind: "publication".into(), message: incomplete.to_string() });
                Ok(snapshot)
            } else { Err(error) }
        }
    }
}

pub fn load(run_dir: impl AsRef<Path>) -> Result<RunSnapshot> {
    let mut s = read(run_dir.as_ref())?;
    if s.snapshot.state == RunState::Running {
        s.snapshot.phase = None;
        s.snapshot.state = RunState::Paused;
        s.snapshot.last_error = Some(LastError {
            provider: None,
            kind: "interrupted".into(),
            message: "interrupted".into(),
        });
        event(
            &mut s,
            TimelineKind::Paused,
            None,
            None,
            None,
            "interrupted",
        );
        save(&mut s)?;
    }
    read_snapshot(run_dir)
}
pub fn resume(
    run_dir: impl AsRef<Path>,
    clients: Clients,
    control: Arc<RunControl>,
    on_event: impl Fn(RunEvent),
) -> Result<RunSnapshot> {
    let mut s = read(run_dir.as_ref())?;
    if !matches!(
        s.snapshot.state,
        RunState::Paused | RunState::Running | RunState::Returned
    ) {
        return Err(invalid(
            "only paused, interrupted, or returned runs can resume",
        ));
    }
    if clients.0.provider() != s.config.author || clients.1.provider() != s.config.author.other() {
        return Err(invalid("provider assignments do not match configuration"));
    }
    if s.snapshot.state == RunState::Returned {
        s.returned_until = Some(s.snapshot.rounds_done + 1);
    }
    if s.snapshot.state != RunState::Returned {
        s.snapshot.generation += 1;
    }
    s.snapshot.last_error = None;
    event(&mut s, TimelineKind::Resumed, None, None, None, "Resumed");
    state(&mut s, RunState::Running, None, &on_event)?;
    drive(s, clients, control, on_event)
}
pub fn decide(
    run_dir: impl AsRef<Path>,
    kind: DecisionKind,
    name: impl Into<String>,
    note: impl Into<String>,
) -> Result<RunSnapshot> {
    let mut s = {
        let _publication = PUBLICATION.lock().unwrap_or_else(|e| e.into_inner());
        let saved = read(run_dir.as_ref())?;
        repair_artifacts(&saved)?;
        saved
    };
    if !matches!(
        s.snapshot.state,
        RunState::Stopped
            | RunState::Completed
            | RunState::Approved
            | RunState::Rejected
            | RunState::Returned
    ) {
        return Err(invalid("A decision needs a stopped or completed run"));
    }
    // no Return once the run has reached ITS OWN maximum of rounds
    // (policy.max_rounds), whether it completed its last round or was stopped in the middle of it (its revision is
    // written: `pending`). Below that maximum, a run stopped mid-round keeps that revision and Return finishes the
    // round's review (`drive` skips the author while `pending` is set).
    if kind == DecisionKind::Returned && reached_round(&s) >= s.config.policy.max_rounds {
        return Err(invalid(
            "Maximum rounds reached — use Restart from revision on Reviews",
        ));
    }
    // Bind the decision to the repaired bytes of the committed revision.
    use sha2::{Digest, Sha256};
    let document_sha256 = format!(
        "{:x}",
        Sha256::digest(fs::read(
            s.config.run_dir.join("output/final/final_document.md")
        )?)
    );
    let decision = HumanDecision {
        kind,
        name: name.into(),
        note: note.into(),
        at: now_utc_rfc3339(),
        after_round: s.snapshot.rounds_done,
        revision: s.snapshot.rounds_done,
        document_sha256,
    };
    if kind == DecisionKind::Returned {
        s.snapshot.generation += 1;
    }
    s.snapshot.decision = Some(decision.clone());
    s.snapshot.decision_log.push(decision);
    s.snapshot.state = match kind {
        DecisionKind::Approved => RunState::Approved,
        DecisionKind::Rejected => RunState::Rejected,
        DecisionKind::Returned => RunState::Returned,
    };
    event(
        &mut s,
        TimelineKind::HumanDecision,
        None,
        None,
        None,
        format!("{kind:?}"),
    );
    save(&mut s)?;
    // Decide must report publication failure even though reads retain the snapshot.
    reconcile(run_dir.as_ref(), false).map(|saved| saved.snapshot)
}
pub fn decide_issue(
    run_dir: impl AsRef<Path>,
    issue_id: &str,
    choice: HumanChoice,
    note: &str,
) -> Result<RunSnapshot> {
    decide_issue_with_answerer(run_dir, issue_id, choice, note, None)
}
/// Answer with explicit attribution; absence never inherits another decision's identity.
pub fn decide_issue_with_answerer(
    run_dir: impl AsRef<Path>,
    issue_id: &str,
    choice: HumanChoice,
    note: &str,
    answered_by: Option<&str>,
) -> Result<RunSnapshot> {
    let mut s = read(run_dir.as_ref())?;
    if s.snapshot.state == RunState::Running {
        return Err(invalid("pause or finish the run before deciding"));
    }
    if s.ledger.history(issue_id).is_none()
        && s.ledger
            .human_questions()
            .iter()
            .any(|q| q.issue_id == issue_id)
        && (choice != HumanChoice::Accept || note.trim().is_empty())
    {
        return Err(invalid("An answer is required"));
    }
    if !s
        .ledger
        .apply_human(issue_id, choice, note, s.snapshot.rounds_done)
    {
        return Err(invalid("unknown issue"));
    }
    s.ledger.set_question_answerer(issue_id, answered_by);
    event(
        &mut s,
        TimelineKind::HumanDecision,
        None,
        None,
        None,
        format!("{issue_id}: {choice:?}: {note}"),
    );
    save(&mut s)?;
    Ok(s.snapshot)
}
pub fn list(runs_root: impl AsRef<Path>) -> Vec<RunSummary> {
    let mut out = vec![];
    if let Ok(entries) = fs::read_dir(runs_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Ok(s) = load(&path) {
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
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    out
}

/// Stop a run whose worker has exited. Callers serialize this with worker launch and decisions.
pub fn stop_idle(run_dir: impl AsRef<Path>) -> Result<RunSnapshot> {
    let mut s = read(run_dir.as_ref())?;
    if !matches!(s.snapshot.state, RunState::Paused | RunState::Returned) {
        return Err(invalid(
            "only paused or returned runs can be stopped while idle",
        ));
    }
    event(&mut s, TimelineKind::Stopped, None, None, None, "user_stop");
    state(
        &mut s,
        RunState::Stopped,
        Some(StopReason::UserStop),
        &|_| {},
    )?;
    Ok(s.snapshot)
}

#[cfg(test)]
mod publication_tests {
    use super::*;
    fn original_hash() -> String {
        use sha2::{Digest, Sha256};
        format!("{:x}", Sha256::digest(b"Original"))
    }
    #[test]
    fn recovery_and_export_reject_a_mismatched_saved_approval() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = saved(dir.path());
        save(&mut s).unwrap();
        decide(dir.path(), DecisionKind::Approved, "Ada", "OK").unwrap();
        let state_path = dir.path().join("run_state.json");
        let original_state = fs::read(&state_path).unwrap();
        let final_path = dir.path().join("output/final/final_document.md");
        let approved_text = fs::read(&final_path).unwrap();
        for change_text in [true, false] {
            let mut state: serde_json::Value = serde_json::from_slice(&original_state).unwrap();
            if change_text {
                state["snapshot"]["documents"][0] = serde_json::json!("Unapproved text");
            } else {
                state["manifest"]["human_decision"]["document_sha256"] = serde_json::json!("wrong hash");
            }
            let state_bytes = serde_json::to_vec(&state).unwrap();
            fs::write(&state_path, &state_bytes).unwrap();
            for error in [read_snapshot(dir.path()).unwrap_err(), export_report(dir.path()).unwrap_err()] {
                assert!(error.downcast_ref::<RevisionIntegrity>().is_some());
                assert!(error.to_string().contains("revision 00"));
            }
            assert_eq!(fs::read(&state_path).unwrap(), state_bytes);
            assert_eq!(fs::read(&final_path).unwrap(), approved_text);
        }
    }
    fn saved(path: &Path) -> Saved {
        let policy = Policy::default();
        serde_json::from_value(serde_json::json!({
            "config": {"run_id":"test", "name":"Test", "run_dir":path, "author":"claude",
                "objective":"Review", "criteria":[], "policy":policy, "primary":"doc.md", "supporting":[],
                "call_timeout":{"secs":1,"nanos":0}, "author_model":null,"reviewer_model":null},
            "snapshot": {"id":"test","name":"Test","createdAt":"2026-01-01T00:00:00Z",
                "author":"claude","reviewer":"codex","policy":policy,"objective":"Review","criteria":[],
                "state":"completed","stopReason":null,"lastError":null,"roundsDone":0,"documents":["Original"],
                "rounds":[],"issues":[],"counts":{"critical":0,"high":0,"medium":0,"low":0},
                "decision":null,"decisionLog":[],"outputTree":[],"primaryName":"doc.md","supportingNames":[]},
            "ledger":{"issues":{}},"timeline":[], "supporting":[],"last_review":null,"pending":null,"returned_until":null,
            "manifest":{"schema_version":1,"run_id":"test","name":"Test","created_at":"now","updated_at":"now",
                "os":"test","app_version":"test","author":{"provider":"claude","cli_version":null,"model":null},
                "reviewer":{"provider":"codex","cli_version":null,"model":null},"inputs":[],"objective":"Review",
                "criteria":[],"policy":policy,"rounds_completed":0,"state":"completed","stop_reason":null,"human_decision":null}
        })).unwrap()
    }
    #[test]
    fn consistent_reads_leave_output_untouched() {
        fn capture(path: &Path) -> std::collections::BTreeMap<PathBuf, (std::time::SystemTime, Vec<u8>)> {
            let mut files = std::collections::BTreeMap::new();
            for entry in fs::read_dir(path).unwrap() {
                let path = entry.unwrap().path();
                assert_ne!(path.extension().and_then(|s| s.to_str()), Some("tmp"), "temporary output appeared");
                let metadata = fs::metadata(&path).unwrap();
                let bytes = if metadata.is_dir() { Vec::new() } else { fs::read(&path).unwrap() };
                files.insert(path.clone(), (metadata.modified().unwrap(), bytes));
                if metadata.is_dir() { files.extend(capture(&path)); }
            }
            files
        }
        for approved in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let mut s = saved(dir.path());
            if approved {
                s.snapshot.state = RunState::Approved;
                s.snapshot.decision = Some(HumanDecision {
                    kind: DecisionKind::Approved, name: "Ada".into(), note: "saved".into(),
                    at: now_utc_rfc3339(), after_round: 0, revision: 0,
                    document_sha256: original_hash(),
                });
            }
            save(&mut s).unwrap();
            let output = dir.path().join("output");
            let before = capture(&output);
            let watching = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
            let active = watching.clone();
            let watched = output.clone();
            let watcher = std::thread::spawn(move || {
                while active.load(std::sync::atomic::Ordering::SeqCst) {
                    capture(&watched);
                    std::thread::yield_now();
                }
            });
            for _ in 0..50 { read_snapshot(dir.path()).unwrap(); }
            watching.store(false, std::sync::atomic::Ordering::SeqCst);
            let observed = watcher.join();
            assert!(before == capture(&output), "reads changed output bytes or mtimes");
            observed.unwrap();
        }
    }

    #[test]
    fn reads_and_loads_repair_disagreement_and_missing_artifacts() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = saved(dir.path());
        s.snapshot.state = RunState::Approved;
        s.snapshot.decision = Some(HumanDecision {
            kind: DecisionKind::Approved, name: "Ada".into(), note: "saved".into(),
            at: now_utc_rfc3339(), after_round: 0, revision: 0,
            document_sha256: original_hash(),
        });
        save(&mut s).unwrap();
        for use_load in [false, true] {
            for relative in [
                "audit/run_manifest.json", "audit/timeline.json",
                "final/human_decision.json", "final/final_document.md",
                "final/final_document.docx", "final/unresolved_issues.json", "final/final_review.html",
            ] {
                fs::remove_file(dir.path().join("output").join(relative)).unwrap();
                if use_load { load(dir.path()).unwrap(); } else { read_snapshot(dir.path()).unwrap(); }
                assert!(artifacts_agree(&read(dir.path()).unwrap()).unwrap(), "{relative}");
            }
            for (relative, field) in [
                ("audit/run_manifest.json", "state"),
                ("audit/run_manifest.json", "human_decision"),
                ("final/human_decision.json", "document_sha256"),
                ("final/human_decision.json", "name"),
            ] {
                let path = dir.path().join("output").join(relative);
                let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                value[field] = serde_json::json!("stale");
                fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
                if use_load { load(dir.path()).unwrap(); } else { read_snapshot(dir.path()).unwrap(); }
                assert!(artifacts_agree(&read(dir.path()).unwrap()).unwrap(), "{relative}: {field}");
            }
        }
    }

    #[test]
    fn question_rejection_names_only_a_validated_id() {
        for (detail, expected) in [
            ("conflicting question id: R01-001", true),
            ("conflicting question id: R01-001 password=private", false),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let mut s = saved(dir.path());
            provider_error(
                &mut s,
                CallError::Malformed(detail.into()),
                Provider::Claude,
                Role::Author,
                &|_| {},
            )
            .unwrap();
            let message = &s.snapshot.last_error.unwrap().message;
            assert_eq!(message.contains("R01-001"), expected);
            assert!(!message.contains("private"));
        }
    }
    #[test]
    fn failed_commit_and_load_remove_private_staging() {
        for callback_failure in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let mut s = saved(dir.path());
            save(&mut s).unwrap();
            s.snapshot.decision = Some(HumanDecision {
                kind: DecisionKind::Approved,
                name: "Private Name".into(),
                note: "Private Note".into(),
                at: now_utc_rfc3339(),
                after_round: 0,
                revision: 0,
                document_sha256: original_hash(),
            });
            let result = save_before_commit(&mut s, |stage| {
                assert!(
                    fs::read_to_string(stage.join("output/final/human_decision.json"))?
                        .contains("Private Note")
                );
                if callback_failure {
                    return Err("injected before commit".into());
                }
                fs::create_dir(dir.path().join("run_state.json.tmp"))?;
                Ok(())
            });
            assert!(result.is_err());
            assert!(
                !dir.path().join(".publication").exists(),
                "failed commit retained private staging"
            );
        }
        let dir = tempfile::tempdir().unwrap();
        let mut s = saved(dir.path());
        save(&mut s).unwrap();
        fs::create_dir_all(dir.path().join(".publication/output/final")).unwrap();
        fs::write(
            dir.path()
                .join(".publication/output/final/human_decision.json"),
            "Private Note",
        )
        .unwrap();
        load(dir.path()).unwrap();
        assert!(
            !dir.path().join(".publication").exists(),
            "load retained stale private staging"
        );
    }
    #[test]
    fn report_state_commit_fault_publishes_nothing_and_reload_repairs() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = saved(dir.path());
        save(&mut s).unwrap();
        let before = fs::read(dir.path().join("output/final/human_decision.json")).unwrap();
        s.snapshot.decision = Some(HumanDecision {
            kind: DecisionKind::Approved,
            name: "Ada".into(),
            note: "OK".into(),
            at: now_utc_rfc3339(),
            after_round: 0,
            revision: 0,
            document_sha256: original_hash(),
        });
        s.snapshot.state = RunState::Approved;
        let result = save_before_commit(&mut s, |stage| {
            assert!(
                fs::read_to_string(stage.join("output/final/human_decision.json"))?.contains("Ada")
            );
            fs::create_dir(dir.path().join("run_state.json.tmp"))?;
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(
            fs::read(dir.path().join("output/final/human_decision.json")).unwrap(),
            before
        );
        assert_eq!(
            read(dir.path()).unwrap().snapshot.state,
            RunState::Completed
        );
        fs::remove_dir(dir.path().join("run_state.json.tmp")).unwrap();
        fs::write(
            dir.path().join("output/final/human_decision.json"),
            "{\"name\":\"uncommitted\"}",
        )
        .unwrap();
        load(dir.path()).unwrap();
        assert_eq!(
            fs::read(dir.path().join("output/final/human_decision.json")).unwrap(),
            before
        );
    }
    #[test]
    fn post_commit_publication_faults_carry_the_decision_and_repair_both_directories() {
        for fault in [PublicationStep::Final, PublicationStep::Audit, PublicationStep::Cleanup] {
            let dir = tempfile::tempdir().unwrap();
            let mut s = saved(dir.path());
            save(&mut s).unwrap();
            s.snapshot.decision = Some(HumanDecision { kind: DecisionKind::Approved, name: "Ada".into(), note: "private decision".into(), at: now_utc_rfc3339(), after_round: 0, revision: 0, document_sha256: original_hash() });
            s.snapshot.state = RunState::Approved;
            let error = save_publication(&mut s, |_| Ok(()), |step, _| {
                if std::mem::discriminant(&step) == std::mem::discriminant(&fault) {
                    return Err(io::Error::new(io::ErrorKind::PermissionDenied, "publication fault").into());
                }
                Ok(())
            }).unwrap_err();
            let incomplete = error.downcast_ref::<PublicationIncomplete>().unwrap();
            assert_eq!(incomplete.snapshot.decision.as_ref().unwrap().name, "Ada");
            assert!(error.to_string().contains("decision committed, publication incomplete"));
            let decision_path = dir.path().join("output/final/human_decision.json");
            fs::remove_file(&decision_path).unwrap();
            fs::create_dir(&decision_path).unwrap();
            let blocked = load(dir.path()).unwrap();
            assert_eq!(blocked.decision.as_ref().unwrap().name, "Ada");
            assert_eq!(blocked.last_error.as_ref().unwrap().kind, "publication");
            assert!(blocked.last_error.unwrap().message.contains("decision committed, publication incomplete"));
            fs::remove_dir(&decision_path).unwrap();
            export_report(dir.path()).unwrap();
            let manifest: serde_json::Value = serde_json::from_slice(&fs::read(dir.path().join("output/audit/run_manifest.json")).unwrap()).unwrap();
            let decision: serde_json::Value = serde_json::from_slice(&fs::read(dir.path().join("output/final/human_decision.json")).unwrap()).unwrap();
            assert_eq!(manifest["human_decision"], decision);
            assert_eq!(load(dir.path()).unwrap().state, RunState::Approved);
        }
    }

    #[test]
    fn read_during_staged_publication_cannot_delete_the_writers_stage() {
        use std::sync::mpsc;
        let dir = tempfile::tempdir().unwrap();
        let mut s = saved(dir.path());
        save(&mut s).unwrap();
        s.snapshot.state = RunState::Approved;
        s.snapshot.decision = Some(HumanDecision { kind: DecisionKind::Approved, name: "Ada".into(), note: "saved".into(), at: now_utc_rfc3339(), after_round: 0, revision: 0, document_sha256: original_hash() });
        let (ready_tx, ready_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let writer = std::thread::spawn(move || save_before_commit(&mut s, |_| {
            ready_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            Ok(())
        }));
        ready_rx.recv().unwrap();
        let path = dir.path().to_owned();
        let (read_tx, read_rx) = mpsc::channel();
        let reader = std::thread::spawn(move || { read_tx.send(read_snapshot(path)).unwrap(); });
        // A reader may observe either committed generation, but it must not destroy
        // the writer's not-yet-committed stage. Give it an opportunity to do so.
        let early = read_rx.recv_timeout(Duration::from_millis(100)).ok();
        release_tx.send(()).unwrap();
        writer.join().unwrap().unwrap();
        let observed = early.unwrap_or_else(|| read_rx.recv().unwrap()).unwrap();
        assert!(matches!(observed.state, RunState::Completed | RunState::Approved));
        reader.join().unwrap();
        assert_eq!(load(dir.path()).unwrap().decision.unwrap().name, "Ada");
    }

    #[test]
    fn incomplete_publication_names_a_decision_only_when_one_is_committed() {
        let dir = tempfile::tempdir().unwrap();
        let s = saved(dir.path());
        let error = PublicationIncomplete { snapshot: s.snapshot };
        assert!(error.to_string().contains("state committed, publication incomplete"));
        assert!(!error.to_string().contains("decision saved"));
    }

}
