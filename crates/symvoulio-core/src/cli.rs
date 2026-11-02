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
/// Panel PATH includes the same fallback locations used by S1 detection.
pub fn panel_path() -> std::ffi::OsString {
    #[cfg(windows)]
    let base = refreshed_path();
    #[cfg(not(windows))]
    let base = env::var_os("PATH").unwrap_or_default();
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
    let paths = env::var_os("PATH").unwrap_or_default();
    let directories: Vec<PathBuf> = env::split_paths(&paths).collect();
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
