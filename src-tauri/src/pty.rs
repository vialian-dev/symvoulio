// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::{
    collections::{HashMap, VecDeque},
    io::{Read, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::{ipc::Channel, State};

const BACKLOG_BYTES: usize = 64 * 1024;
type EventSink = Box<dyn Fn(PtyEvent) + Send>;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind")]
pub enum PtyEvent {
    Data { data: String },
    Exit { code: u32 },
    CommandTimeout,
    CommandExit { code: Option<i32>, last: String },
}
#[derive(Serialize)]
pub struct PtyInfo {
    shell: String,
    reattached: bool,
}
#[derive(Serialize)]
pub struct ShellInfo {
    os: String,
    pwsh: bool,
    powershell: bool,
    chosen: String,
    available: Vec<String>,
}

#[derive(Default)]
struct Output {
    backlog: VecDeque<char>,
    backlog_bytes: usize,
    sink: Option<EventSink>,
    exit: Option<u32>,
    /// When the shell last printed; a paste waits for the shell to settle (see `wait_until_settled`).
    last_data: Option<Instant>,
    input: bool,
    input_line: String,
    input_edited: bool,
    install_line: Option<String>,
    submitted_install: bool,
    busy: bool,
    submitted: Option<Instant>,
    diagnostic: String,
    discard_line: bool,
    protocol: String,
    marker: String,
    cursor_answered: bool,
    prompt_seen: bool,
}
fn strip_ansi(text: &str) -> String {
    static ANSI: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    ANSI.get_or_init(|| regex::Regex::new(r"\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07]*(?:\x07)|[@-_])").expect("ANSI regex")).replace_all(text, "").into_owned()
}
impl Output {
    fn emit(&mut self, event: PtyEvent) {
        // OSC 133 D is emitted by the shell prompt hook after a submitted command.
        // It is a completion signal, never inferred from silence, prompt text or shell termination.
        if let PtyEvent::Data { data } = &event {
            self.protocol.push_str(data);
            if self.busy {
                if self.discard_line {
                    if let Some(end) = data.find('\n') { self.discard_line = false; self.diagnostic.push_str(&data[end + 1..]); }
                } else { self.diagnostic.push_str(data); }
            }
            while let Some(start) = (!self.marker.is_empty()).then(|| self.protocol.find(&self.marker)).flatten() {
                let rest = &self.protocol[start + self.marker.len()..];
                let Some(end) = rest.find('\x07') else { break };
                let code = rest[..end].parse::<i32>().ok();
                let first_prompt = !self.prompt_seen;
                self.prompt_seen = true;
                let before = self.diagnostic.split(&self.marker).next().unwrap_or("");
                let last = symvoulio_core::redact(strip_ansi(before).lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim());
                self.protocol = rest[end + 1..].to_owned();
                if self.busy && !first_prompt {
                    self.input = false;
                    self.input_line.clear();
                    self.input_edited = false;
                    self.busy = false;
                    self.diagnostic.clear();
                    if self.submitted_install {
                        self.submitted_install = false;
                        if let Some(sink) = &self.sink { sink(PtyEvent::CommandExit { code, last }); }
                    }
                }
            }
            if self.protocol.len() > 8192 { self.protocol = self.protocol.chars().rev().take(1024).collect::<String>().chars().rev().collect(); }
            if self.busy {
                // Bound memory; diagnostics are redacted before leaving the backend.
                while self.diagnostic.len() > 8192 {
                    if let Some(end) = self.diagnostic.find('\n') { self.diagnostic.drain(..=end); }
                    else { self.diagnostic.clear(); self.discard_line = true; break; }
                }
            }
        }
        match &event {
            PtyEvent::Data { data } => {
                self.last_data = Some(Instant::now());
                for character in data.chars() {
                    self.backlog.push_back(character);
                    self.backlog_bytes += character.len_utf8();
                    while self.backlog_bytes > BACKLOG_BYTES {
                        self.backlog_bytes -= self
                            .backlog
                            .pop_front()
                            .expect("nonempty backlog")
                            .len_utf8();
                    }
                }
            }
            PtyEvent::Exit { code } => {
                self.exit = Some(*code);
                self.busy = false;
                self.submitted_install = false;
                self.submitted = None;
            },
            PtyEvent::CommandExit { .. } | PtyEvent::CommandTimeout => {},
        }
        if let Some(sink) = &self.sink {
            sink(event);
        }
    }
    fn attach(&mut self, sink: EventSink) {
        if !self.backlog.is_empty() {
            // The replay must not repeat the shell's cursor-position query (ESC[6n): xterm would answer it again,
            // and that stray answer reached PSReadLine as input and ate the next key (measured 30.10: a paste after a
            // re-attach showed «laude auth login»). Only a live query deserves an answer.
            let replay: String = self.backlog.iter().collect();
            sink(PtyEvent::Data {
                data: if self.cursor_answered { replay.replace("\x1b[6n", "") } else { replay },
            });
        }
        if let Some(code) = self.exit {
            sink(PtyEvent::Exit { code });
        }
        self.sink = Some(sink);
    }
}
struct Session {
    path: std::ffi::OsString,
    hooks: Option<tempfile::TempDir>,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
    shell: String,
    output: Arc<Mutex<Output>>,
}
impl Session {
    fn terminate(&mut self) -> Result<(), String> {
        let mut child = self.child.lock().map_err(|e| e.to_string())?;
        if child.try_wait().map_err(|e| e.to_string())?.is_none() {
            child.kill().map_err(|e| e.to_string())?;
            child.wait().map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.terminate();
    }
}
#[derive(Default)]
pub struct PtyManager {
    sessions: Mutex<HashMap<String, Session>>,
}
impl PtyManager {
    pub fn close_all(&self) {
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.clear();
        }
    }
}
fn panel_id(panel: &str) -> Result<(), String> {
    if matches!(panel, "claude" | "codex") {
        Ok(())
    } else {
        Err("Unknown provider panel".into())
    }
}
fn size(cols: u16, rows: u16) -> Result<PtySize, String> {
    if cols == 0 || rows == 0 {
        return Err("Terminal dimensions must be positive".into());
    }
    Ok(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    })
}
#[derive(Default)]
struct Utf8Decoder {
    tail: Vec<u8>,
}
impl Utf8Decoder {
    fn decode(&mut self, bytes: &[u8], eof: bool) -> String {
        self.tail.extend_from_slice(bytes);
        let mut result = String::new();
        let mut consumed = 0;
        while consumed < self.tail.len() {
            match std::str::from_utf8(&self.tail[consumed..]) {
                Ok(text) => {
                    result.push_str(text);
                    consumed = self.tail.len();
                }
                Err(error) => {
                    let valid = error.valid_up_to();
                    result.push_str(
                        std::str::from_utf8(&self.tail[consumed..consumed + valid])
                            .expect("validated UTF-8"),
                    );
                    consumed += valid;
                    match error.error_len() {
                        Some(len) => {
                            result.push('\u{fffd}');
                            consumed += len;
                        }
                        None if eof => {
                            result.push('\u{fffd}');
                            consumed = self.tail.len();
                        }
                        None => break,
                    }
                }
            }
        }
        self.tail.drain(..consumed);
        result
    }
}
fn read_output(mut reader: Box<dyn Read + Send>, sink: EventSink) {
    let mut decoder = Utf8Decoder::default();
    let mut buffer = [0; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                let data = decoder.decode(&buffer[..n], false);
                if !data.is_empty() {
                    sink(PtyEvent::Data { data });
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    let data = decoder.decode(&[], true);
    if !data.is_empty() {
        sink(PtyEvent::Data { data });
    }
}
fn spawn_session(
    mut command: CommandBuilder,
    dimensions: PtySize,
    shell: String,
    sink: EventSink,
) -> Result<Session, String> {
    let token = uuid::Uuid::new_v4().simple().to_string();
    command.env("SYMVOULIO_PROMPT_TOKEN", &token);
    let pair = native_pty_system()
        .openpty(dimensions)
        .map_err(|e| e.to_string())?;
    let reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let child = pair
        .slave
        .spawn_command(command)
        .map_err(|e| e.to_string())?;
    drop(pair.slave);
    let child = Arc::new(Mutex::new(child));
    let output = Arc::new(Mutex::new(Output {
        sink: Some(sink),
        marker: format!("\x1b]133;D;{token};"),
        ..Output::default()
    }));
    let read_sink = Arc::clone(&output);
    std::thread::spawn(move || {
        read_output(
            reader,
            Box::new(move |event| {
                if let Ok(mut output) = read_sink.lock() {
                    output.emit(event);
                }
            }),
        )
    });
    let wait_child = Arc::clone(&child);
    let wait_sink = Arc::clone(&output);
    std::thread::spawn(move || loop {
        let status = match wait_child.lock() {
            Ok(mut child) => child.try_wait(),
            Err(_) => break,
        };
        match status {
            Ok(Some(status)) => {
                if let Ok(mut output) = wait_sink.lock() {
                    output.emit(PtyEvent::Exit {
                        code: status.exit_code(),
                    });
                }
                break;
            }
            Err(_) => break,
            Ok(None) => std::thread::sleep(Duration::from_millis(30)),
        }
    });
    Ok(Session {
        path: symvoulio_core::cli::panel_path(),
        hooks: None,
        master: pair.master,
        writer,
        child,
        shell,
        output,
    })
}
fn executable(path: &std::path::Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        return path
            .metadata()
            .is_ok_and(|m| m.permissions().mode() & 0o111 != 0);
    }
    #[cfg(not(unix))]
    {
        true
    }
}
fn find_on_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|dir| dir.join(name))
            .find(|path| executable(path))
    })
}
#[tauri::command]
pub fn shell_info() -> Result<ShellInfo, String> {
    let pwsh = find_on_path(if cfg!(windows) { "pwsh.exe" } else { "pwsh" }).is_some();
    let powershell = find_on_path(if cfg!(windows) {
        "powershell.exe"
    } else {
        "powershell"
    })
    .is_some();
    let mut available: Vec<String> = Vec::new();
    if pwsh {
        available.push("pwsh".into());
    }
    if powershell {
        available.push("powershell".into());
    }
    if !cfg!(windows) {
        for shell in ["zsh", "bash", "sh"] {
            if find_on_path(shell).is_some() {
                available.push(shell.into());
            }
        }
    }
    let chosen = if cfg!(windows) {
        if pwsh {
            "pwsh".into()
        } else {
            "powershell".into()
        }
    } else if cfg!(target_os = "macos") && available.iter().any(|s| s == "zsh") {
        "zsh".into()
    } else {
        std::env::var("SHELL")
            .ok()
            .filter(|s| executable(std::path::Path::new(s)))
            .unwrap_or_else(|| {
                available
                    .iter()
                    .find(|s| matches!(s.as_str(), "zsh" | "bash" | "sh"))
                    .cloned()
                    .unwrap_or_default()
            })
    };
    Ok(ShellInfo {
        os: if cfg!(windows) { "windows" } else if cfg!(target_os = "macos") { "macos" } else { "linux" }.into(),
        pwsh,
        powershell,
        chosen,
        available,
    })
}
fn choose_shell(saved: Option<&str>, info: &ShellInfo) -> Result<String, String> {
    match saved {
        Some("pwsh") if info.pwsh => Ok("pwsh".into()),
        Some("powershell") if info.powershell => Ok("powershell".into()),
        None | Some("" | "pwsh" | "powershell") => Ok(info.chosen.clone()),
        Some("zsh" | "bash" | "sh") if !cfg!(windows) => {
            let shell = saved.expect("selected shell");
            info.available
                .iter()
                .any(|available| available == shell)
                .then(|| shell.to_owned())
                .ok_or_else(|| "Shell is unavailable".into())
        }
        Some(_) => Err("Shell is unavailable".into()),
    }
}
fn home_directory() -> Result<PathBuf, String> {
    std::env::home_dir().ok_or_else(|| "Cannot locate the user's home directory".into())
}
#[tauri::command]
pub fn pty_open(
    manager: State<'_, PtyManager>,
    panel: String,
    cwd: Option<String>,
    shell: Option<String>,
    cols: u16,
    rows: u16,
    on_event: Channel<PtyEvent>,
) -> Result<PtyInfo, String> {
    panel_id(&panel)?;
    let dimensions = size(cols, rows)?;
    let chosen_shell = choose_shell(shell.as_deref(), &shell_info()?)?;
    let mut sessions = manager.sessions.lock().map_err(|e| e.to_string())?;
    let sink: EventSink = Box::new(move |event| {
        let _ = on_event.send(event);
    });
    if sessions.get(&panel).is_some_and(|session| {
        session.child.lock().is_ok_and(|mut child| child.try_wait().is_ok_and(|exit| exit.is_some()))
            || session.output.lock().is_ok_and(|o| o.exit.is_some() || (session.path != symvoulio_core::cli::panel_path() && !o.busy && !o.input && !o.input_edited))
    }) {
        // Only replace an idle prompt; never discard pending input or a running process.
        sessions.remove(&panel);
    }
    if let Some(session) = sessions.get_mut(&panel) {
        session
            .master
            .resize(dimensions)
            .map_err(|e| e.to_string())?;
        session
            .output
            .lock()
            .map_err(|e| e.to_string())?
            .attach(sink);
        return Ok(PtyInfo {
            shell: session.shell.clone(),
            reattached: true,
        });
    }
    let shell = chosen_shell;
    let mut command = CommandBuilder::new(&shell);
    #[cfg(debug_assertions)]
    if std::env::var_os("FAKE_CLI_ABSENT").is_some() {
        if let Some(fake) = std::env::var_os("SYMVOULIO_FAKE_CLI") {
            command = CommandBuilder::new(fake);
            command.arg("--install-shell");
        }
    }
    if cfg!(windows) && !(cfg!(debug_assertions) && std::env::var_os("FAKE_CLI_ABSENT").is_some()) {
        command.args(["-NoExit", "-Command", r#"$global:SymvoulioOriginalPrompt = $function:prompt; $global:SymvoulioPreviousExit = $global:LASTEXITCODE; function global:prompt { $ok = $?; $savedExit = $global:LASTEXITCODE; $c = 0; if (!$ok) { if ($null -ne $savedExit -and $savedExit -ne 0 -and $savedExit -ne $global:SymvoulioPreviousExit) { $c = $savedExit } else { $c = '' } }; [Console]::Write(([char]27).ToString() + ']133;D;' + $env:SYMVOULIO_PROMPT_TOKEN + ';' + $c + [char]7); try { & $global:SymvoulioOriginalPrompt } finally { $global:LASTEXITCODE = $savedExit; $global:SymvoulioPreviousExit = $savedExit } }"#]);
    }
    // CommandBuilder imports registry variables on Windows and supplies SHELL
    // on Unix. Preserve exactly the app's inherited environment instead.
    command.env_clear();
    for (key, value) in std::env::vars_os() {
        command.env(key, value);
    }
    command.env("PATH", symvoulio_core::cli::panel_path());
    let mut hook_dir = None;
    if !cfg!(windows) {
        let name = std::path::Path::new(&shell).file_name().and_then(|s| s.to_str()).unwrap_or(&shell);
        // tempfile atomically creates a random directory with mode 0700 on Unix.
        hook_dir = Some(tempfile::Builder::new().prefix("symvoulio-shell-").tempdir().map_err(|e| e.to_string())?);
        let hooks = hook_dir.as_ref().expect("created hook directory").path();
        if name == "bash" {
            let rc = hooks.join("bashrc");
            std::fs::write(&rc, r#"[ ! -f "$HOME/.bashrc" ] || . "$HOME/.bashrc"
__symvoulio_prompt() { local c=$?; printf '\033]133;D;%s;%s\007' "$SYMVOULIO_PROMPT_TOKEN" "$c"; }
if declare -p PROMPT_COMMAND 2>/dev/null | grep -q 'declare -a'; then
  PROMPT_COMMAND=(__symvoulio_prompt "${PROMPT_COMMAND[@]}")
else
  PROMPT_COMMAND="__symvoulio_prompt${PROMPT_COMMAND:+; $PROMPT_COMMAND}"
fi
"#).map_err(|e| e.to_string())?;
            command.arg("--rcfile"); command.arg(rc); command.arg("-i");
        } else if name == "zsh" {
            for file in [".zshenv", ".zprofile"] {
                std::fs::write(hooks.join(file), format!(r#"__symvoulio_hooks=$ZDOTDIR
if [[ $SYMVOULIO_ZDOTDIR_SET == 1 ]]; then
  export ZDOTDIR=$SYMVOULIO_ORIGINAL_ZDOTDIR
else
  unset ZDOTDIR
fi
[[ ! -f "$SYMVOULIO_ORIGINAL_ZDOTDIR/{file}" ]] || source "$SYMVOULIO_ORIGINAL_ZDOTDIR/{file}"
# Startup files may intentionally redirect (or unset) ZDOTDIR. Carry that
# setting to the next original startup file and restore it in .zshrc.
if (( ${{+ZDOTDIR}} )); then
  export SYMVOULIO_ORIGINAL_ZDOTDIR=$ZDOTDIR SYMVOULIO_ZDOTDIR_SET=1
else
  export SYMVOULIO_ORIGINAL_ZDOTDIR=$HOME SYMVOULIO_ZDOTDIR_SET=0
fi
ZDOTDIR=$__symvoulio_hooks
"#)).map_err(|e| e.to_string())?;
            }
            command.env("SYMVOULIO_ZDOTDIR_SET", if std::env::var_os("ZDOTDIR").is_some() { "1" } else { "0" });
            std::fs::write(hooks.join(".zshrc"), r#"if [[ $SYMVOULIO_ZDOTDIR_SET == 1 ]]; then
  export ZDOTDIR=$SYMVOULIO_ORIGINAL_ZDOTDIR
else
  unset ZDOTDIR
fi
[[ ! -f "$SYMVOULIO_ORIGINAL_ZDOTDIR/.zshrc" ]] || source "$SYMVOULIO_ORIGINAL_ZDOTDIR/.zshrc"
__symvoulio_precmd() { local c=$?; printf '\033]133;D;%s;%s\007' "$SYMVOULIO_PROMPT_TOKEN" "$c"; }
precmd_functions=(__symvoulio_precmd $precmd_functions)
"#).map_err(|e| e.to_string())?;
            command.env("SYMVOULIO_ORIGINAL_ZDOTDIR", std::env::var_os("ZDOTDIR").unwrap_or(home_directory()?.into_os_string()));
            command.env("ZDOTDIR", hooks); command.args(["-l", "-i"]);
        }
    }
    let cwd = match cwd {
        Some(path) if std::path::Path::new(&path).is_dir() => PathBuf::from(path),
        _ => home_directory()?,
    };
    command.cwd(cwd);
    command.env("SYMVOULIO_PANEL", &panel);
    let mut session = spawn_session(command, dimensions, shell.clone(), sink)?;
    session.hooks = hook_dir;
    sessions.insert(panel, session);
    Ok(PtyInfo {
        shell,
        reattached: false,
    })
}
#[tauri::command]
pub fn pty_write(
    manager: State<'_, PtyManager>,
    panel: String,
    data: String,
) -> Result<(), String> {
    validate_keystrokes(&data)?;
    panel_id(&panel)?;
    let mut sessions = manager.sessions.lock().map_err(|e| e.to_string())?;
    let session = sessions.get_mut(&panel).ok_or("Panel has no PTY session")?;
    let cursor_report = data.strip_prefix("\x1b[").and_then(|s| s.strip_suffix('R')).and_then(|s| s.split_once(';')).is_some_and(|(row, col)| {
        !row.is_empty() && !col.is_empty() && row.bytes().all(|c| c.is_ascii_digit()) && col.bytes().all(|c| c.is_ascii_digit())
    });
    if cursor_report {
        session.output.lock().map_err(|e| e.to_string())?.cursor_answered = true;
    }
    let protocol_report = cursor_report || matches!(data.as_str(), "\x1b[I" | "\x1b[O");
    if !protocol_report && data.chars().any(|c| c.is_control() && !matches!(c, '\r' | '\n' | '\x03' | '\x15' | '\x08' | '\x7f')) {
        session.output.lock().map_err(|e| e.to_string())?.input_edited = true;
    }
    if !data.starts_with('\x1b') && !data.is_empty() {
        let mut output = session.output.lock().map_err(|e| e.to_string())?;
        if data == "\r" || data == "\n" {
            if !output.busy && (output.input || output.input_edited) {
                output.submitted_install = !output.input_edited && output.install_line.as_ref() == Some(&output.input_line);
                output.install_line = None;
                output.input_line.clear();
                output.busy = true; output.diagnostic.clear();
                let submitted = Instant::now(); output.submitted = Some(submitted);
                let pending = Arc::clone(&session.output);
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_secs(15));
                    if let Ok(mut output) = pending.lock() {
                        if output.busy && output.submitted == Some(submitted) {
                            output.emit(PtyEvent::CommandTimeout);
                        }
                    }
                });
            }
        } else {
            for ch in data.chars() {
                match ch {
                    '\x03' => { output.input_line.clear(); output.input_edited = false; }
                    '\x15' => { output.input_line.clear(); }
                    '\x08' | '\x7f' => { output.input_line.pop(); }
                    c if !c.is_control() => output.input_line.push(c),
                    _ => {}
                }
            }
            output.input = !output.input_line.is_empty();
        }
        debug_input("pty_write", &data);
    }
    session
        .writer
        .write_all(data.as_bytes())
        .and_then(|()| session.writer.flush())
        .map_err(|e| e.to_string())
}
#[cfg(windows)]
fn escape_powershell_literal(value: &str) -> String {
    value.chars().flat_map(|c| if matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201a}' | '\u{201b}') { vec![c, c] } else { vec![c] }).collect()
}
#[cfg(windows)]
fn resolve_login_command(_panel: &str, command: String, guard: Option<bool>) -> String {
    let provider_args = command.strip_prefix("codex ").map(|args| (symvoulio_core::Provider::Codex, args))
        .or_else(|| command.strip_prefix("claude ").map(|args| (symvoulio_core::Provider::Claude, args)));
    if let Some((provider, args)) = provider_args.filter(|_| guard != Some(true)) {
        if let Some(cli) = symvoulio_core::cli::resolve(provider).filter(|cli| cli.shim.is_some() || cli.prefix_args.is_empty()) {
            let executable = escape_powershell_literal(&cli.shim.as_ref().unwrap_or(&cli.program).to_string_lossy());
            format!("& '{executable}' {args}")
        } else { command }
    } else { command }
}
pub fn validate_paste(command: &str) -> Result<&str, String> {
    if command.chars().any(|c| c < '\u{20}' || c == '\u{7f}') {
        return Err("Pasted commands cannot contain control characters".into());
    }
    if command.chars().count() > 512 {
        return Err("Pasted commands cannot exceed 512 characters".into());
    }
    Ok(command)
}
#[tauri::command]
pub async fn pty_paste(
    manager: State<'_, PtyManager>,
    panel: String,
    command: String,
    guard: Option<bool>,
    clipboard: Option<bool>,
) -> Result<(), String> {
    validate_paste(&command)?;
    panel_id(&panel)?;
    // Existing panels retain their parent environment. Resolve login afresh so
    // a native installer changing HKCU PATH is usable without restarting the app.
    #[cfg(windows)]
    let command = if clipboard == Some(true) { command } else { resolve_login_command(&panel, command, guard) };
    validate_paste(&command)?;
    let output = {
        let sessions = manager.sessions.lock().map_err(|e| e.to_string())?;
        sessions
            .get(&panel)
            .ok_or("Panel has no PTY session")?
            .output
            .clone()
    };
    // Measured 30.10 on Windows: a paste right after a fresh pwsh started lost its first character, typed before
    // PSReadLine took input. Wait until the shell has printed and then been quiet for a moment (at most 5 s).
    tauri::async_runtime::spawn_blocking(move || {
        if clipboard == Some(true) && output.lock().is_ok_and(|o| o.prompt_seen) { true }
        else { wait_until_settled(&output, SETTLE, Duration::from_secs(5)) }
    })
    .await
    .map_err(|e| e.to_string())?;
    let mut sessions = manager.sessions.lock().map_err(|e| e.to_string())?;
    let session = sessions.get_mut(&panel).ok_or("Panel has no PTY session")?;
    let mut output = session.output.lock().map_err(|e| e.to_string())?;
    if clipboard != Some(true) && (output.input || output.busy || output.input_edited) {
        return Err(if guard == Some(true) {
            "The panel has pending input or is busy. Finish or clear it before installing."
        } else {
            "The panel has pending input or is busy. Finish or clear it before pasting a login or status command."
        }.into());
    }
    output.input = true;
    output.input_line.push_str(&command);
    if guard == Some(true) { output.install_line = Some(command.clone()); }
    debug_input("pty_paste", &command);
    session.writer.write_all(command.as_bytes()).and_then(|()| session.writer.flush()).map_err(|e| e.to_string())
}
const SETTLE: Duration = Duration::from_millis(300);
/// True once the shell has printed something and then printed nothing for `quiet`; false at `limit`.
fn wait_until_settled(output: &Arc<Mutex<Output>>, quiet: Duration, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    loop {
        let settled = output
            .lock()
            .ok()
            .and_then(|o| o.last_data)
            .is_some_and(|t| t.elapsed() >= quiet);
        if settled {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}
#[tauri::command]
pub fn pty_resize(
    manager: State<'_, PtyManager>,
    panel: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    panel_id(&panel)?;
    let dimensions = size(cols, rows)?;
    let sessions = manager.sessions.lock().map_err(|e| e.to_string())?;
    sessions
        .get(&panel)
        .ok_or("Panel has no PTY session")?
        .master
        .resize(dimensions)
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn pty_close(manager: State<'_, PtyManager>, panel: String) -> Result<(), String> {
    panel_id(&panel)?;
    if let Some(mut session) = manager
        .sessions
        .lock()
        .map_err(|e| e.to_string())?
        .remove(&panel)
    {
        session.terminate()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exited_shell_clears_busy_without_reporting_install_success() {
        let mut output = Output { busy: true, submitted_install: true, submitted: Some(Instant::now()), ..Output::default() };
        output.emit(PtyEvent::Exit { code: 1 });
        assert!(!output.busy);
        assert!(!output.submitted_install);
        assert_eq!(output.submitted, None);
        assert_eq!(output.exit, Some(1));
    }

    #[test]
    fn first_and_idle_prompt_markers_preserve_pending_pastes() {
        let mut output = Output { marker: "\x1b]133;D;private;".into(), input: true, input_line: "installer".into(), install_line: Some("installer".into()), ..Output::default() };
        for _ in 0..2 {
            output.emit(PtyEvent::Data { data: "\x1b]133;D;private;0\x07".into() });
            assert!(output.input);
            assert_eq!(output.input_line, "installer");
            assert_eq!(output.install_line.as_deref(), Some("installer"));
        }
        output.busy = true;
        output.emit(PtyEvent::Data { data: "\x1b]133;D;private;0\x07".into() });
        assert!(!output.busy);
        assert!(!output.input);
        assert!(output.input_line.is_empty());
    }

    #[test]
    #[cfg(windows)]
    fn smart_quote_path_runs_in_real_powershell() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("O\u{2019}Connor");
        std::fs::create_dir(&folder).unwrap();
        let exe = folder.join("probe.exe");
        std::fs::copy(std::env::var("COMSPEC").unwrap(), &exe).unwrap();
        let escaped = escape_powershell_literal(&exe.to_string_lossy());
        let command = format!("& '{escaped}' /c echo QUOTE_PATH_OK");
        for shell in ["powershell.exe", "pwsh.exe"] {
            let result = std::process::Command::new(shell).args(["-NoProfile", "-Command", &command]).output().unwrap();
            assert!(result.status.success(), "{shell}: {:?}", result);
            assert!(String::from_utf8_lossy(&result.stdout).contains("QUOTE_PATH_OK"));
        }
    }

    #[test]
    #[cfg(windows)]
    #[ignore = "station measurement temporarily updates HKCU PATH; run alone"]
    fn r69_real_shell_fresh_user_path() {
        use winreg::{enums::*, RegKey};
        let shells: Vec<_> = ["powershell.exe", "pwsh.exe"].into_iter().filter_map(find_on_path).collect();
        let inherited = std::env::var_os("PATH").unwrap_or_default();
        let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE).unwrap();
        let original = key.get_raw_value("Path").ok();
        struct Restore(winreg::RegKey, Option<winreg::RegValue>);
        impl Drop for Restore { fn drop(&mut self) { if let Some(value) = &self.1 { self.0.set_raw_value("Path", value).unwrap(); } else { let _ = self.0.delete_value("Path"); } } }
        let restore = Restore(key, original);
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("codex.cmd"), "@echo R69_FRESH_USER_PATH_VISIBLE\r\n").unwrap();
        // Put the measurement shim first only in the registry, leaving app PATH stale.
        let old: String = restore.0.get_value("Path").unwrap_or_default();
        restore.0.set_value("Path", &format!("{};{old}", dir.path().display())).unwrap();
        // Suppress real Codex candidates on this station by measuring resolver output:
        // the selected path must be the new registry entry, not inherited PATH.
        std::env::set_var("PATH", std::env::var("SystemRoot").map(|r| format!("{r}\\System32")).unwrap());
        let command = resolve_login_command("codex", "codex login".into(), None);
        std::env::set_var("PATH", inherited);
        assert!(command.contains(&dir.path().to_string_lossy().to_string()), "resolved: {command}");
        assert!(!shells.is_empty(), "no real PowerShell available");
        for shell_path in shells {
            let shell = shell_path.to_str().unwrap();
            let result = std::process::Command::new(shell).args(["-NoProfile", "-Command", &command]).output().unwrap();
            println!("R69 {shell}: status={} stdout={}", result.status, String::from_utf8_lossy(&result.stdout));
            assert!(result.status.success());
            assert!(String::from_utf8_lossy(&result.stdout).contains("R69_FRESH_USER_PATH_VISIBLE"));
            let panel = std::process::Command::new(shell).env("PATH", symvoulio_core::cli::panel_path()).args(["-NoProfile", "-Command", "codex login"]).output().unwrap();
            println!("R69 new panel {shell}: {}", String::from_utf8_lossy(&panel.stdout));
            assert!(panel.status.success());
            assert!(String::from_utf8_lossy(&panel.stdout).contains("R69_FRESH_USER_PATH_VISIBLE"));
        }
    }

    #[test]
    #[cfg(windows)]
    fn real_powershell_hook_measurement() {
        for shell in ["powershell.exe", "pwsh.exe"] {
            if find_on_path(shell).is_none() { println!("{shell}: SKIPPED (shell not installed)"); continue; }
            let mut command = CommandBuilder::new(shell);
            command.args(["-NoExit", "-Command", r#"$global:SymvoulioOriginalPrompt = $function:prompt; $global:SymvoulioPreviousExit = $global:LASTEXITCODE; function global:prompt { $ok = $?; $savedExit = $global:LASTEXITCODE; $c = 0; if (!$ok) { if ($null -ne $savedExit -and $savedExit -ne 0 -and $savedExit -ne $global:SymvoulioPreviousExit) { $c = $savedExit } else { $c = '' } }; [Console]::Write(([char]27).ToString() + ']133;D;' + $env:SYMVOULIO_PROMPT_TOKEN + ';' + $c + [char]7); try { & $global:SymvoulioOriginalPrompt } finally { $global:LASTEXITCODE = $savedExit; $global:SymvoulioPreviousExit = $savedExit } }"#]);
            let (tx, rx) = std::sync::mpsc::channel();
            let mut session = spawn_session(command, size(100, 30).unwrap(), shell.into(), Box::new(move |event| { let _ = tx.send(event); })).unwrap();
            let expected = format!("{}3\x07", session.output.lock().unwrap().marker);
            let deadline = Instant::now() + Duration::from_secs(25);
            let mut bytes = String::new();
            let mut submitted = false;
            while Instant::now() < deadline {
                if let Ok(PtyEvent::Data { data }) = rx.recv_timeout(Duration::from_millis(200)) {
                    if data.contains("\x1b[6n") { session.writer.write_all(b"\x1b[1;1R").unwrap(); session.writer.flush().unwrap(); }
                    bytes.push_str(&data);
                }
                if !submitted && bytes.contains("> ") {
                    std::thread::sleep(Duration::from_secs(1));
                    bytes.clear();
                    session.writer.write_all(b"cmd /c exit 3\r").unwrap(); session.writer.flush().unwrap(); submitted = true;
                }
                if submitted && bytes.contains(&expected) { break; }
            }
            println!("REAL {shell}: submitted={submitted} marker={} bytes={bytes:?}", bytes.contains(&expected));
            assert!(submitted);
            assert!(bytes.contains(&expected), "{shell}: completion marker missing");
            let success = format!("{}0\x07", session.output.lock().unwrap().marker);
            session.writer.write_all(b"Write-Output $LASTEXITCODE\r").unwrap(); session.writer.flush().unwrap();
            bytes.clear();
            let deadline = Instant::now() + Duration::from_secs(15);
            while Instant::now() < deadline && !bytes.contains(&success) {
                if let Ok(PtyEvent::Data { data }) = rx.recv_timeout(Duration::from_millis(200)) { bytes.push_str(&data); }
            }
            println!("REAL {shell}: cmdlet after native failure marker={} bytes={bytes:?}", bytes.contains(&success));
            assert!(bytes.contains(&success), "successful cmdlet inherited stale native exit");
            session.terminate().unwrap();
        }
    }

    #[test]
    fn only_authenticated_marker_completes_install() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut output = Output { marker: "\x1b]133;D;private;".into(), prompt_seen: true, busy: true, submitted_install: true, sink: Some(Box::new(move |e| { tx.send(e).unwrap(); })), ..Output::default() };
        output.emit(PtyEvent::Data { data: "\x1b]133;D;7\x07".into() });
        assert!(output.busy);
        assert!(!rx.try_iter().any(|e| matches!(e, PtyEvent::CommandExit { .. })));
        output.emit(PtyEvent::Data { data: "\r\n\x1b[31mtoken=sk-ant-FAKESECRET123456\x1b[0m\r\n\x1b]133;D;private;7\x07".into() });
        assert!(!output.busy);
        assert!(rx.try_iter().any(|e| matches!(e, PtyEvent::CommandExit { code: Some(7), last } if !last.contains("FAKESECRET") && !last.contains('\x1b'))));
    }
    #[test]
    fn oversized_diagnostic_never_exposes_a_suffix() {
        let mut output = Output { busy: true, ..Output::default() };
        output.emit(PtyEvent::Data { data: "x".repeat(9000) });
        output.emit(PtyEvent::Data { data: "SECRET_SUFFIX".into() });
        assert!(output.diagnostic.is_empty());
        output.emit(PtyEvent::Data { data: "\nSafe line\n".into() });
        assert_eq!(output.diagnostic, "Safe line\n");
    }
    #[test]
    fn saved_shell_choice() {
        let info = ShellInfo {
            os: "windows".into(),
            pwsh: true,
            powershell: true,
            chosen: "pwsh".into(),
            available: vec!["pwsh".into(), "powershell".into()],
        };
        assert_eq!(
            choose_shell(Some("powershell"), &info).unwrap(),
            "powershell"
        );
        assert_eq!(choose_shell(None, &info).unwrap(), "pwsh");
        assert_eq!(choose_shell(Some(""), &info).unwrap(), "pwsh");
        let fallback = ShellInfo {
            os: "windows".into(),
            pwsh: false,
            powershell: true,
            chosen: "powershell".into(),
            available: vec!["powershell".into()],
        };
        assert_eq!(choose_shell(Some("pwsh"), &fallback).unwrap(), "powershell");
        assert!(choose_shell(Some("cmd /c evil"), &info).is_err());
        let mac = ShellInfo {
            os: "macos".into(),
            pwsh: false,
            powershell: false,
            chosen: "zsh".into(),
            available: vec!["zsh".into()],
        };
        assert_eq!(choose_shell(Some("pwsh"), &mac).unwrap(), "zsh");
    }

    #[test]
    fn paste_waits_for_a_shell_that_printed_and_went_quiet() {
        let output = Arc::new(Mutex::new(Output::default()));
        // Silent shell (not started yet): give up at the limit.
        assert!(!wait_until_settled(
            &output,
            Duration::from_millis(50),
            Duration::from_millis(200)
        ));
        // The shell prints its prompt: settled once quiet.
        output.lock().unwrap().emit(PtyEvent::Data {
            data: "PS C:\\> ".into(),
        });
        let t = Instant::now();
        assert!(wait_until_settled(
            &output,
            Duration::from_millis(100),
            Duration::from_secs(2)
        ));
        assert!(t.elapsed() >= Duration::from_millis(90));
    }
    #[test]
    fn reattach_replays_an_unanswered_cursor_query() {
        let mut output = Output::default();
        output.emit(PtyEvent::Data { data: "\x1b[6n".into() });
        let (tx, rx) = std::sync::mpsc::channel();
        output.attach(Box::new(move |e| { tx.send(e).unwrap(); }));
        assert!(matches!(rx.recv().unwrap(), PtyEvent::Data { data } if data == "\x1b[6n"));
    }
    #[test]
    fn replay_drops_the_cursor_position_query() {
        let mut output = Output { cursor_answered: true, ..Output::default() };
        output.emit(PtyEvent::Data {
            data: "\x1b[6n\x1b[?25lPS C:\\> ".into(),
        });
        let (tx, rx) = std::sync::mpsc::channel();
        output.attach(Box::new(move |e| {
            let _ = tx.send(e);
        }));
        match rx.recv().unwrap() {
            PtyEvent::Data { data } => assert_eq!(data, "\x1b[?25lPS C:\\> "),
            PtyEvent::Exit { .. } | PtyEvent::CommandExit { .. } | PtyEvent::CommandTimeout => panic!("expected data"),
        }
    }
    #[test]
    fn decoder_preserves_split_characters() {
        let mut decoder = Utf8Decoder::default();
        assert_eq!(decoder.decode(&[b'a', 0xf0, 0x9f], false), "a");
        assert_eq!(decoder.decode(&[0x98, 0x80, 0xe2], false), "\u{1f600}");
        assert_eq!(decoder.decode(&[0x82, 0xac], false), "\u{20ac}");
        assert_eq!(decoder.decode(&[0xff, 0xc2], false), "\u{fffd}");
        assert_eq!(decoder.decode(&[], true), "\u{fffd}");
    }
    #[test]
    fn decoder_handles_every_split_boundary() {
        let text = "ASCII \u{e9} \u{20ac} \u{1f600} end";
        for split in 0..=text.len() {
            let mut decoder = Utf8Decoder::default();
            let mut decoded = decoder.decode(&text.as_bytes()[..split], false);
            decoded.push_str(&decoder.decode(&text.as_bytes()[split..], true));
            assert_eq!(decoded, text);
        }
    }
    #[test]
    fn panels_are_restricted_to_providers() {
        assert!(panel_id("claude").is_ok());
        assert!(panel_id("codex").is_ok());
        for panel in ["", "other", "Claude", "../codex"] {
            assert!(panel_id(panel).is_err());
        }
    }
    #[test]
    fn paste_accepts_login_commands() {
        for command in ["claude auth login", "codex login --with-api-key"] {
            assert_eq!(validate_paste(command).unwrap(), command);
        }
        assert!(validate_paste(&"\u{e9}".repeat(512)).is_ok());
    }
    #[test]
    fn paste_rejects_controls_and_long_commands() {
        for command in ["x\r", "x\n", "\x03", "x\t", "x\x7f", &"x".repeat(513)] {
            assert!(validate_paste(command).is_err());
        }
    }
    #[test]
    fn backlog_is_bounded_and_replayed_before_live_data() {
        let mut output = Output::default();
        output.emit(PtyEvent::Data {
            data: "\u{20ac}".repeat(BACKLOG_BYTES),
        });
        assert!(output.backlog_bytes <= BACKLOG_BYTES);
        let (tx, rx) = std::sync::mpsc::channel();
        output.attach(Box::new(move |event| {
            tx.send(event).unwrap();
        }));
        output.emit(PtyEvent::Data {
            data: "live".into(),
        });
        assert!(
            matches!(rx.recv().unwrap(), PtyEvent::Data { data } if data.starts_with('\u{20ac}'))
        );
        assert!(matches!(rx.recv().unwrap(), PtyEvent::Data { data } if data == "live"));
    }
    #[test]
    fn attaching_replaces_the_old_sink_and_replays_exit() {
        let mut output = Output::default();
        let (old_tx, old_rx) = std::sync::mpsc::channel();
        output.attach(Box::new(move |event| {
            old_tx.send(event).unwrap();
        }));
        output.emit(PtyEvent::Data {
            data: "before".into(),
        });
        output.emit(PtyEvent::Exit { code: 7 });
        let (new_tx, new_rx) = std::sync::mpsc::channel();
        output.attach(Box::new(move |event| {
            new_tx.send(event).unwrap();
        }));
        assert!(matches!(new_rx.recv().unwrap(), PtyEvent::Data { data } if data == "before"));
        assert!(matches!(new_rx.recv().unwrap(), PtyEvent::Exit { code: 7 }));
        assert_eq!(old_rx.try_iter().count(), 2);
    }
    #[test]
    fn portable_pty_spawns_and_reads_marker() {
        let mut command = CommandBuilder::new(if cfg!(windows) { "cmd.exe" } else { "sh" });
        command.args(if cfg!(windows) {
            vec!["/c", "echo symvoulio-pty-ok"]
        } else {
            vec!["-c", "echo symvoulio-pty-ok"]
        });
        let (tx, rx) = std::sync::mpsc::channel();
        let mut session = spawn_session(
            command,
            size(80, 24).unwrap(),
            "test".into(),
            Box::new(move |event| {
                let _ = tx.send(event);
            }),
        )
        .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut output = String::new();
        let mut cursor_replied = false;
        loop {
            let event = rx
                .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                .unwrap_or_else(|error| {
                    panic!("PTY marker within 10 seconds: {error}; output: {output:?}")
                });
            if let PtyEvent::Data { data } = event {
                output.push_str(&data);
            }
            // ConPTY asks the terminal for its initial cursor position. In the
            // app xterm.js answers through onData; this test has no emulator.
            // The protocol response contains no Enter or command execution.
            if !cursor_replied && output.contains("\x1b[6n") {
                session.writer.write_all(b"\x1b[1;1R").unwrap();
                session.writer.flush().unwrap();
                cursor_replied = true;
            }
            if output.contains("symvoulio-pty-ok") {
                break;
            }
        }
    }
}

/// Raw input permits keys and terminal protocol, never multi-line or bracketed clipboard paste.
pub fn validate_keystrokes(data: &str) -> Result<&str, String> {
    if (data.chars().count() > 1 && data.contains(['\r', '\n']))
        || data.contains("\x1b[200~")
        || data.contains("\x1b[201~")
    {
        return Err("Pasted text must use the safe paste endpoint".into());
    }
    Ok(data)
}
#[cfg(test)]
mod keystroke_tests {
    use super::validate_keystrokes;
    #[test]
    fn raw_input_rejects_paste_but_preserves_keys_and_terminal_protocol() {
        for data in [
            "hello\r",
            "one\ntwo",
            "\r\n",
            "\x1b[200~hello",
            "hello\x1b[201~",
        ] {
            assert!(
                validate_keystrokes(data).is_err(),
                "unsafe paste accepted: {data:?}"
            );
        }
        for data in [
            "",
            "a",
            "\u{e9}",
            "\r",
            "\n",
            "\x1b[A",
            "\x1b[1;1R",
            "plain text",
        ] {
            assert_eq!(validate_keystrokes(data).unwrap(), data);
        }
    }
}

// Fixture evidence is debug-only and contains only entry-point names plus the safe paste bytes.
#[cfg(debug_assertions)]
static DEBUG_LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());
fn debug_input(kind: &str, data: &str) {
    #[cfg(debug_assertions)]
    {
        if std::env::var_os("FAKE_CLI_PTY_BYTES_LOG").is_some() {
            if let Ok(mut log) = DEBUG_LOG.lock() { log.push(kind.into()); }
            {
                if let (Some(root), Some(name)) = (std::env::var_os("SYMVOULIO_DATA_DIR"), std::env::var_os("FAKE_CLI_PTY_BYTES_LOG")) {
                    let path = PathBuf::from(root).join(name);
                    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) { let _ = file.write_all(data.as_bytes()); }
                }
            }
        }
    }
    #[cfg(not(debug_assertions))]
    let _ = (kind, data);
}
#[cfg(debug_assertions)]
#[tauri::command]
pub async fn pty_debug_log() -> String {
    // UI clicks start an asynchronous guarded paste. Let the debug observer wait
    // for its first recorded write rather than race the shell-settle barrier.
    tauri::async_runtime::spawn_blocking(|| {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let log = DEBUG_LOG.lock().map(|log| log.join("\n")).unwrap_or_default();
            if !log.is_empty() || Instant::now() >= deadline { return log; }
            std::thread::sleep(Duration::from_millis(10));
        }
    }).await.unwrap_or_default()
}
