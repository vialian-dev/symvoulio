// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! Resolve executables and npm shims without passing JSON through cmd.exe.
use crate::Provider;
use std::{
    env, fs,
    path::{Path, PathBuf},
};
#[derive(Debug, Clone)]
pub struct ResolvedCli {
    pub program: PathBuf,
    pub prefix_args: Vec<String>,
    pub shim: Option<PathBuf>,
}
pub fn provider_name(p: Provider) -> &'static str {
    match p {
        Provider::Claude => "claude",
        Provider::Codex => "codex",
    }
}
/// Fresh registry PATH for newly opened Windows panels, without changing app globals.
#[cfg(windows)]
pub fn refreshed_path() -> std::ffi::OsString {
    use winreg::{enums::*, RegKey};
    let variables = regex::Regex::new(r"%([^%]+)%").expect("environment variable regex");
    let mut directories = Vec::new();
    for (root, subkey) in [(HKEY_LOCAL_MACHINE, r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment"), (HKEY_CURRENT_USER, "Environment")] {
        if let Ok(key) = RegKey::predef(root).open_subkey(subkey) {
            if let Ok(path) = key.get_value::<String, _>("Path") {
                let expanded = variables.replace_all(&path, |capture: &regex::Captures<'_>| env::var(&capture[1]).unwrap_or_else(|_| capture[0].to_owned()));
                directories.extend(env::split_paths(expanded.as_ref()));
            }
        }
    }
    #[cfg(debug_assertions)]
    if let Some(file) = env::var_os("SYMVOULIO_USER_PATH_FILE") {
        if let Ok(path) = fs::read_to_string(file) { directories.extend(env::split_paths(&path)); }
    }
    // Retain explicit launch-time additions (developer tools, portable shells).
    directories.extend(env::split_paths(&env::var_os("PATH").unwrap_or_default()));
    env::join_paths(directories).unwrap_or_else(|_| env::var_os("PATH").unwrap_or_default())
}
/// Marker that separates the login shell's PATH from anything its startup files print.
const LOGIN_PATH_MARKER: &str = "__SYMVOULIO_LOGIN_PATH__=";
/// The most login-shell output the probe reads; the PATH line comes last and is far shorter.
#[cfg(not(windows))]
const LOGIN_PATH_MAX_OUTPUT: u64 = 64 * 1024;
/// How long the one login-shell PATH probe may take before the app gives up on it.
#[cfg(not(windows))]
const LOGIN_PATH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// The Codex standalone package's command folder, relative to CODEX_HOME (default ~/.codex) (F1).
const CODEX_STANDALONE_BIN: &str = ".codex/packages/standalone/current/bin";
/// How long one login-shell PATH probe stays valid; «Check again» after an install that edited ~/.zprofile sees the
/// new PATH without restarting the app (F1).
#[cfg(not(windows))]
const LOGIN_PATH_TTL: std::time::Duration = std::time::Duration::from_secs(30);

/// Directories searched for provider CLIs on macOS and Linux, in order, without duplicates (MAC-05).
///
/// An app opened from Finder or the Dock gets launchd's PATH (`/usr/bin:/bin:/usr/sbin:/sbin`), not the PATH of
/// the user's shell, so Homebrew, npm global and the official installers' folders are added explicitly:
/// 1. the PATH the app was launched with (an explicit choice wins);
/// 2. the login shell's PATH (`~/.zprofile`, `brew shellenv`, nvm/volta set-ups live there);
/// 3. Homebrew on Apple Silicon and on Intel, then the per-user folders of the official installers and npm.
///
/// Pure: the caller supplies both PATH values and the home folder, so it is testable on any OS.
pub fn unix_search_dirs(
    launch: &std::ffi::OsStr,
    login: Option<&std::ffi::OsStr>,
    home: Option<&Path>,
) -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = env::split_paths(launch).collect();
    if let Some(login) = login {
        candidates.extend(env::split_paths(login));
    }
    candidates.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    if let Some(home) = home {
        candidates.extend(
            [".local/bin", ".npm-global/bin", ".volta/bin", ".bun/bin"].map(|d| home.join(d)),
        );
        // The official Codex installer (https://chatgpt.com/codex/install.sh, read 2026-11-08) links
        // ~/.local/bin/codex to ~/.codex/packages/standalone/current/bin/codex; the package folder itself is the
        // fallback when the link is missing (F1).
        candidates.push(home.join(CODEX_STANDALONE_BIN));
        // nvm keeps one folder per Node version (v22.23.4); newest version first, compared as numbers.
        if let Ok(entries) = fs::read_dir(home.join(".nvm/versions/node")) {
            let mut versions: Vec<(Option<Vec<u64>>, PathBuf)> = entries
                .flatten()
                .map(|e| (node_version(&e.file_name().to_string_lossy()), e.path().join("bin")))
                .collect();
            // Parsable versions first, highest first; then any other names, in reverse name order.
            versions.sort_by(|(va, pa), (vb, pb)| vb.cmp(va).then_with(|| pb.cmp(pa)));
            candidates.extend(versions.into_iter().map(|(_, bin)| bin));
        }
    }
    let mut directories: Vec<PathBuf> = Vec::new();
    for dir in candidates {
        // Relative entries (including the empty one) would resolve against the app's working folder.
        if dir.has_root() && !directories.contains(&dir) {
            directories.push(dir);
        }
    }
    directories
}

/// `v22.23.4` -> [22, 23, 4]; `None` for a name that is not a dotted number.
fn node_version(name: &str) -> Option<Vec<u64>> {
    name.strip_prefix('v')
        .unwrap_or(name)
        .split('.')
        .map(|part| part.parse().ok())
        .collect()
}

/// Extract the PATH printed after [`LOGIN_PATH_MARKER`]; startup files may print banners before it.
pub fn parse_login_path(stdout: &str) -> Option<std::ffi::OsString> {
    stdout
        .lines()
        .rev()
        .find_map(|line| line.strip_prefix(LOGIN_PATH_MARKER))
        .map(str::trim_end)
        .filter(|path| !path.is_empty())
        .map(Into::into)
}

/// The login shell's PATH, only on macOS (MAC-05), probed again when the last probe is older than
/// [`LOGIN_PATH_TTL`] (F1: an installer that adds a folder to ~/.zprofile is seen without restarting the app).
///
/// Runs `$SHELL -l -c` (zsh when SHELL is missing) with no input and a deadline, so a slow or interactive
/// startup file can never hang detection. `None` when the probe fails; the fixed folders still apply.
#[cfg(not(windows))]
pub fn login_shell_path() -> Option<std::ffi::OsString> {
    static CACHE: std::sync::Mutex<Option<(std::time::Instant, Option<std::ffi::OsString>)>> =
        std::sync::Mutex::new(None);
    if !cfg!(target_os = "macos") {
        return None;
    }
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, path)) = cache.as_ref().filter(|(at, _)| at.elapsed() < LOGIN_PATH_TTL) {
        return path.clone();
    }
    let shell = env::var_os("SHELL")
        .map(PathBuf::from)
        .filter(|s| s.is_absolute() && s.is_file())
        .unwrap_or_else(|| PathBuf::from("/bin/zsh"));
    let path = probe_login_path(&shell, LOGIN_PATH_TIMEOUT);
    *cache = Some((std::time::Instant::now(), path.clone()));
    path
}

/// Ask `shell` (as a login shell) for its PATH. Public for the macOS test of a real login shell.
///
/// The deadline covers the whole probe, reading included: the shell runs in its own process group, its output is
/// read line by line on a helper thread, and the answer is taken as soon as the shell has exited and printed the
/// marker line, even if something its start-up files left in the background still holds the pipe open. At the
/// deadline the whole group is killed and the shell reaped. Background programs that start-up files leave behind
/// after a normal exit are left alone, as after any login.
#[cfg(not(windows))]
pub fn probe_login_path(shell: &Path, timeout: std::time::Duration) -> Option<std::ffi::OsString> {
    use std::{
        io::{BufRead, BufReader},
        os::unix::process::CommandExt,
        process::Stdio,
        sync::mpsc,
        time::Instant,
    };
    let mut child = std::process::Command::new(shell)
        .args(["-l", "-c", &format!("printf '\\n{LOGIN_PATH_MARKER}%s\\n' \"$PATH\"")])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let (tx, rx) = mpsc::channel::<String>();
    // Detached on purpose: it ends when the last holder of the pipe closes it. At most 64 KiB is read, so a chatty
    // start-up file cannot fill memory before the deadline (review note).
    std::thread::spawn(move || {
        use std::io::Read as _;
        for line in BufReader::new(stdout.take(LOGIN_PATH_MAX_OUTPUT)).lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let deadline = Instant::now() + timeout;
    let mut found = None;
    let mut exited = false;
    let mut pipe_closed = false;
    while Instant::now() < deadline {
        match rx.recv_timeout(std::time::Duration::from_millis(20)) {
            Ok(line) => {
                if let Some(path) = parse_login_path(&line) {
                    found = Some(path);
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => pipe_closed = true,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if !exited {
            exited = matches!(child.try_wait(), Ok(Some(_)));
        }
        if exited && (found.is_some() || pipe_closed) {
            return found;
        }
    }
    // Deadline: end the shell and everything it started, then reap the shell.
    unsafe {
        libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
    None
}

/// Search folders for this process on macOS and Linux: launch PATH, login-shell PATH, then the fixed folders.
#[cfg(not(windows))]
pub fn search_dirs() -> Vec<PathBuf> {
    let mut directories = unix_search_dirs(
        &env::var_os("PATH").unwrap_or_default(),
        login_shell_path().as_deref(),
        std::env::home_dir().as_deref(),
    );
    // The Codex installer honours CODEX_INSTALL_DIR (link folder) and CODEX_HOME (package root) (F1).
    let codex_home = env::var_os("CODEX_HOME").map(|h| PathBuf::from(h).join("packages/standalone/current/bin"));
    for dir in [env::var_os("CODEX_INSTALL_DIR").map(PathBuf::from), codex_home].into_iter().flatten() {
        if dir.has_root() && !directories.contains(&dir) {
            directories.push(dir);
        }
    }
    directories
}

/// True when `dir` is one of the entries of the PATH value `path` (F1, F2: a panel opened before the CLI was installed
/// has an older PATH; the login command is then typed with the CLI's full path).
pub fn path_contains(path: &std::ffi::OsStr, dir: &Path) -> bool {
    env::split_paths(path).any(|entry| entry == dir)
}

/// PATH for provider child processes. On macOS and Linux it is [`search_dirs`] plus the CLI's own folder, so an
/// npm-installed CLI (`#!/usr/bin/env node`) finds `node` even when the app was opened from Finder (MAC-05).
/// `None` on Windows: the inherited environment is kept unchanged there (MAC-02).
pub fn child_path(cli: &ResolvedCli) -> Option<std::ffi::OsString> {
    #[cfg(windows)]
    {
        let _ = cli;
        None
    }
    #[cfg(not(windows))]
    {
        let mut directories = search_dirs();
        // The CLI's own folder always comes first, even when it is already listed further down (an nvm version
        // folder): its `node` is the one the CLI was installed with.
        if let Some(parent) = cli.program.parent().filter(|p| p.is_absolute()) {
            directories.retain(|d| d != parent);
            directories.insert(0, parent.into());
        }
        env::join_paths(directories).ok()
    }
}

/// Panel PATH includes the same fallback locations used by S1 detection.
pub fn panel_path() -> std::ffi::OsString {
    #[cfg(windows)]
    let base = refreshed_path();
    #[cfg(not(windows))]
    let base = env::join_paths(search_dirs())
        .unwrap_or_else(|_| env::var_os("PATH").unwrap_or_default());
    let mut directories: Vec<PathBuf> = env::split_paths(&base).collect();
    for name in ["claude", "codex"] {
        if let Some(path) = search(name) {
            if let Some(parent) = path.parent() {
                if !directories.iter().any(|d| d == parent) { directories.push(parent.into()); }
            }
        }
    }
    env::join_paths(directories).unwrap_or(base)
}
fn search(name: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    let directories: Vec<PathBuf> = env::split_paths(&env::var_os("PATH").unwrap_or_default()).collect();
    // Finder and Dock launches carry no shell PATH on macOS (MAC-05).
    #[cfg(not(windows))]
    let directories: Vec<PathBuf> = search_dirs();
    #[cfg(any(windows, debug_assertions))]
    let mut directories = directories;
    #[cfg(windows)]
    {
        // Resolve afresh on every detection (including Check again), without
        // mutating the process environment while provider threads are running.
        if let Ok(key) = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER).open_subkey("Environment") {
            if let Ok(path) = key.get_value::<String, _>("Path") {
                let variables = regex::Regex::new(r"%([^%]+)%").expect("environment variable regex");
                let expanded = variables.replace_all(&path, |capture: &regex::Captures<'_>| env::var(&capture[1]).unwrap_or_else(|_| capture[0].to_owned()));
                directories.extend(env::split_paths(expanded.as_ref()));
            }
        }
        if let Some(local) = env::var_os("LOCALAPPDATA") { directories.push(PathBuf::from(local).join("Programs/OpenAI/Codex/bin")); }
        if let Some(custom) = env::var_os("CODEX_INSTALL_DIR") { directories.push(PathBuf::from(custom)); }
    }
    #[cfg(debug_assertions)]
    if env::var_os("FAKE_CLI_INSTALL_NO_PATH").is_some() {
        if let Some(root) = env::var_os("SYMVOULIO_DATA_DIR").map(PathBuf::from) {
            let bin = root.join("fake_bin");
            if bin.is_dir() { directories.insert(0, bin); }
        }
    }
    for dir in directories {
        #[cfg(windows)]
        {
            let extensions = env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT".into());
            for ext in extensions
                .split(';')
                .filter(|s| [".exe", ".cmd", ".bat"].contains(&s.to_ascii_lowercase().as_str()))
            {
                let p = dir.join(format!("{name}{ext}"));
                if p.is_file() {
                    return Some(p);
                }
            }
        }
        #[cfg(not(windows))]
        {
            let p = dir.join(name);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    if let Some(home) = std::env::home_dir() {
        for dir in [home.join(".local/bin"), home.join(".npm-global/bin"), home.join("AppData/Roaming/npm"), home.join("AppData/Local/Microsoft/WinGet/Links")] {
            for ext in if cfg!(windows) { vec![".exe", ".cmd"] } else { vec![""] } {
                let candidate = dir.join(format!("{name}{ext}"));
                if candidate.is_file() { return Some(candidate); }
            }
        }
    }
    None
}
pub fn parse_shim(path: &Path) -> ResolvedCli {
    let fallback = || ResolvedCli {
        program: path.into(),
        prefix_args: vec![],
        shim: Some(path.into()),
    };
    let Ok(text) = fs::read_to_string(path) else {
        return fallback();
    };
    let target = text.lines().rev().find_map(|line| {
        line.split('"')
            .enumerate()
            .filter(|(i, s)| i % 2 == 1 && s.to_ascii_lowercase().starts_with("%dp0%\\"))
            .map(|(_, s)| s)
            .last()
    });
    let Some(target) = target else {
        return fallback();
    };
    let target = path
        .parent()
        .unwrap_or(Path::new("."))
        .join(target[6..].replace('\\', "/"));
    if !target.is_file() {
        return fallback();
    }
    match target
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "exe" => ResolvedCli {
            program: target,
            prefix_args: vec![],
            shim: Some(path.into()),
        },
        "js" => {
            let local = path.parent().unwrap_or(Path::new(".")).join("node.exe");
            let Some(node) = local.is_file().then_some(local).or_else(|| search("node")) else {
                return fallback();
            };
            ResolvedCli {
                program: node,
                prefix_args: vec![target.to_string_lossy().into()],
                shim: Some(path.into()),
            }
        }
        _ => fallback(),
    }
}
pub fn resolve(provider: Provider) -> Option<ResolvedCli> {
    if let Some(file) = env::var_os("SYMVOULIO_FAKE_CLI")
        .map(PathBuf::from)
        .filter(|p| p.is_file())
    {
        return Some(ResolvedCli {
            program: file,
            prefix_args: vec!["--as".into(), provider_name(provider).into()],
            shim: None,
        });
    }
    let program = search(provider_name(provider))?;
    if program
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"))
    {
        Some(parse_shim(&program))
    } else {
        Some(ResolvedCli {
            program,
            prefix_args: vec![],
            shim: None,
        })
    }
}
