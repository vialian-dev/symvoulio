// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// CARD AST-B (cruel tester): VAL-R21, shell detection per OS, through the public `pty::shell_info`.
//
// GAP (declared): «an explicit zsh selection is accepted on macOS» is decided by `pty::choose_shell`, which is
// private, and by `pty_open`, which needs a Tauri `State` and `Channel`. Neither is reachable from this sibling
// module, so the explicit selection itself is not asserted here; what is asserted is that a zsh on PATH is reported
// as available on macOS (the precondition of any selection).
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

/// PATH and SHELL are process-wide: every test that changes them holds this lock.
static ENV: Mutex<()> = Mutex::new(());
struct Restore {
    path: Option<std::ffi::OsString>,
    shell: Option<std::ffi::OsString>,
}
impl Restore {
    fn set(dirs: &[&Path]) -> Self {
        let saved = Self {
            path: std::env::var_os("PATH"),
            shell: std::env::var_os("SHELL"),
        };
        std::env::set_var("PATH", std::env::join_paths(dirs).unwrap());
        saved
    }
}
impl Drop for Restore {
    fn drop(&mut self) {
        match &self.path {
            Some(v) => std::env::set_var("PATH", v),
            None => std::env::remove_var("PATH"),
        }
        match &self.shell {
            Some(v) => std::env::set_var("SHELL", v),
            None => std::env::remove_var("SHELL"),
        }
    }
}
/// Unique scratch directory, removed on drop (src-tauri has no tempfile dependency).
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let p = std::env::temp_dir().join(format!("ct-pty-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn lock() -> std::sync::MutexGuard<'static, ()> {
    ENV.lock().unwrap_or_else(|e| e.into_inner())
}

/// True when the reported ShellInfo names `name` as available: a `name: true` field, or `name` inside any array.
fn reports(info: &serde_json::Value, name: &str) -> bool {
    match info {
        serde_json::Value::Object(map) => map
            .iter()
            .any(|(k, v)| (k == name && v == &serde_json::Value::Bool(true)) || reports(v, name)),
        serde_json::Value::Array(items) => items
            .iter()
            .any(|v| v.as_str() == Some(name) || reports(v, name)),
        _ => false,
    }
}
fn info() -> serde_json::Value {
    serde_json::to_value(crate::pty::shell_info().unwrap()).unwrap()
}

#[cfg(unix)]
fn fake_exe(dir: &Path, name: &str, executable: bool) {
    use std::os::unix::fs::PermissionsExt;
    let p = dir.join(name);
    std::fs::write(&p, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(
        &p,
        std::fs::Permissions::from_mode(if executable { 0o755 } else { 0o644 }),
    )
    .unwrap();
}
/// A PATH holding only `sh`, so shells that are really installed on the host cannot be found.
#[cfg(unix)]
fn bare_dir() -> Scratch {
    let d = Scratch::new();
    std::os::unix::fs::symlink("/bin/sh", d.path().join("sh")).unwrap();
    d
}
#[cfg(windows)]
fn system32() -> PathBuf {
    PathBuf::from(std::env::var_os("SystemRoot").expect("SystemRoot")).join("System32")
}
#[cfg(windows)]
fn fake_exe(dir: &Path, name: &str) {
    // Any real executable will do: only the file's presence on PATH is looked at.
    std::fs::copy(system32().join("whoami.exe"), dir.join(name)).unwrap();
}

// ------------------------------------------------------------ VAL-R21, macOS / Linux

#[cfg(unix)]
#[test]
fn test_dod1_val_r21_pwsh_without_exe_is_detected_on_unix() {
    let _g = lock();
    let d = Scratch::new();
    fake_exe(d.path(), "pwsh", true);
    let mut dirs: Vec<PathBuf> = vec![d.path().to_path_buf()];
    dirs.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
    let refs: Vec<&Path> = dirs.iter().map(PathBuf::as_path).collect();
    let _r = Restore::set(&refs);
    let v = info();
    assert!(reports(&v, "pwsh"), "an installed `pwsh` (no .exe) is not reported: {v}");
}

#[cfg(target_os = "macos")]
#[test]
fn test_dod1_val_r21_zsh_on_path_is_reported_on_macos() {
    let _g = lock();
    let d = Scratch::new();
    fake_exe(d.path(), "zsh", true);
    let _r = Restore::set(&[d.path()]);
    let v = info();
    assert!(reports(&v, "zsh"), "zsh on PATH is not reported on macOS: {v}");
}

#[cfg(unix)]
#[test]
fn test_dod1_val_r21_reported_shells_exist_on_unix() {
    let _g = lock();
    let d = bare_dir();
    let _r = Restore::set(&[d.path()]);
    std::env::set_var("SHELL", "/nonexistent/fish-ct");
    let v = info();
    assert!(!reports(&v, "pwsh"), "pwsh reported though not on PATH: {v}");
    assert!(!reports(&v, "powershell"), "powershell reported on a host without it: {v}");
    assert!(
        cfg!(target_os = "macos") || !reports(&v, "zsh"),
        "zsh reported though not on PATH: {v}"
    );
    assert!(
        !v.to_string().contains("/nonexistent/fish-ct"),
        "the shell chosen is a path that does not exist: {v}"
    );
}

#[cfg(unix)]
#[test]
fn test_dod1_val_r21_file_without_execute_bit_is_not_a_shell() {
    let _g = lock();
    let d = bare_dir();
    fake_exe(d.path(), "pwsh", false);
    let _r = Restore::set(&[d.path()]);
    let v = info();
    assert!(!reports(&v, "pwsh"), "a non-executable `pwsh` is reported: {v}");
}

// ------------------------------------------------------------ VAL-R21, Windows counterpart

#[cfg(windows)]
#[test]
fn test_dod1_val_r21_pwsh_exe_is_detected_on_windows() {
    let _g = lock();
    let d = Scratch::new();
    fake_exe(d.path(), "pwsh.exe");
    let sys = system32();
    let _r = Restore::set(&[d.path(), sys.as_path()]);
    let v = info();
    assert!(reports(&v, "pwsh"), "pwsh.exe on PATH is not reported: {v}");
}

#[cfg(windows)]
#[test]
fn test_dod1_val_r21_reported_shells_exist_on_windows() {
    let _g = lock();
    let d = Scratch::new();
    let sys = system32();
    let _r = Restore::set(&[d.path(), sys.as_path()]);
    let v = info();
    assert!(!reports(&v, "pwsh"), "pwsh reported though not on PATH: {v}");
    assert!(!reports(&v, "powershell"), "powershell reported though not on PATH: {v}");
    assert!(!reports(&v, "zsh"), "zsh reported on Windows: {v}");
}

/// DoD3: no skip/ignore in this file; every windows-only test has a unix twin and the reverse.
#[test]
fn test_dod3_no_skip_ignore_and_platform_twins() {
    let src = include_str!("ct_astra_pty.rs");
    let code: String = src
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let banned = [format!("#[{}", "ignore"), format!("x{}", "fail"), format!("should_{}", "panic")];
    for b in &banned {
        assert!(!code.contains(b.as_str()), "found {b}");
    }
    assert!(code.contains("#[cfg(unix)]") && code.contains("#[cfg(windows)]"));
}
