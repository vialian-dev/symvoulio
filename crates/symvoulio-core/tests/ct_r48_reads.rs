// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! The external review's pass-5 finding R48: reading a healthy run must not rewrite its package. Expectations come from
//! the finding, not from the code under test:
//! * VAL-R48-healthy  a consistent, completed run: `read_snapshot` x 50 changes no file under output/ (mtime, bytes),
//!   creates no `.tmp` at any moment, and leaves run_state.json alone;
//! * VAL-R48-readonly the same consistent run with output/audit, then the whole output/, read-only: `read_snapshot`
//!   returns Ok with the authoritative snapshot (state, no error), not PermissionDenied. Windows: the read-only attribute
//!   on every file plus a deny-write ACL on every directory; unix: chmod (skipped with a printed reason as root);
//! * VAL-R48-repair   control: a deliberately inconsistent package (human_decision.json says Approved, run_state.json
//!   does not) IS repaired on read.
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, SystemTime},
};
use symvoulio_core::{
    engine::{decide, read_snapshot, start, RunConfig},
    provider::{ClaudeCli, CodexCli, ProviderClient},
    DecisionKind, Policy, Provider, RunState,
};

static ENV: Mutex<()> = Mutex::new(());
struct Env;
impl Env {
    fn new() -> Self {
        std::env::set_var("SYMVOULIO_FAKE_CLI", env!("CARGO_BIN_EXE_fake-cli"));
        for key in ["FAKE_CLI_SCENARIO", "FAKE_CLI_LOGGED_OUT", "FAKE_CLI_LOG"] {
            std::env::remove_var(key);
        }
        Self
    }
}
impl Drop for Env {
    fn drop(&mut self) {
        std::env::remove_var("SYMVOULIO_FAKE_CLI");
    }
}

/// A completed run through the real CLI adapters on the fake CLI (its default scenario).
fn completed_run(root: &Path) -> PathBuf {
    let primary = root.join("business.md");
    fs::write(&primary, "# Northwind\nCapital cost $4.2M. Growth 18 %. owner to be confirmed by Operations").unwrap();
    let cfg = RunConfig {
        run_id: "r48-run".into(),
        name: "r48".into(),
        run_dir: root.join("run"),
        author: Provider::Claude,
        objective: "Improve for executive review".into(),
        criteria: vec!["All totals must reconcile.".into()],
        policy: Policy { new_content_limit_pct: 100.0, ..Policy::default() },
        primary,
        supporting: vec![],
        call_timeout: Duration::from_secs(10),
        author_model: None,
        reviewer_model: None,
    };
    let run = cfg.run_dir.clone();
    let clients: (Arc<dyn ProviderClient>, Arc<dyn ProviderClient>) = (Arc::new(ClaudeCli), Arc::new(CodexCli));
    let s = start(cfg, clients, Arc::default(), |_| {}).unwrap();
    assert_eq!(s.state, RunState::Completed, "setup: {:?}", s.last_error);
    run
}
fn files_under(root: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(root) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                files_under(&p, out);
            } else {
                out.push(p);
            }
        }
    }
}
fn dirs_under(root: &Path, out: &mut Vec<PathBuf>) {
    out.push(root.to_path_buf());
    if let Ok(entries) = fs::read_dir(root) {
        for e in entries.flatten() {
            if e.path().is_dir() {
                dirs_under(&e.path(), out);
            }
        }
    }
}
fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for e in fs::read_dir(from).unwrap().flatten() {
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if src.is_dir() {
            copy_dir(&src, &dst);
        } else {
            fs::copy(&src, &dst).unwrap();
        }
    }
}
/// mtime and bytes of every file under `dir`.
fn fingerprint(dir: &Path) -> BTreeMap<PathBuf, (SystemTime, Vec<u8>)> {
    let mut files = vec![];
    files_under(dir, &mut files);
    files
        .into_iter()
        .map(|f| {
            let m = fs::metadata(&f).unwrap().modified().unwrap();
            let b = fs::read(&f).unwrap();
            (f.strip_prefix(dir).unwrap().to_path_buf(), (m, b))
        })
        .collect()
}

// ---------------------------------------------------------------- VAL-R48-healthy

#[test]
fn test_dod1_val_r48_healthy_reads_rewrite_nothing() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let run = completed_run(dir.path());
    let before = fingerprint(&run.join("output"));
    let state_before = fs::read(run.join("run_state.json")).unwrap();
    assert!(before.len() >= 8, "setup: a completed run has a package, found {} files", before.len());
    thread::sleep(Duration::from_millis(50)); // a rewrite would carry a later mtime

    let done = Arc::new(AtomicBool::new(false));
    let seen_tmp = Arc::new(Mutex::new(Vec::<String>::new()));
    let watcher = {
        let (run, done, seen_tmp) = (run.clone(), done.clone(), seen_tmp.clone());
        thread::spawn(move || {
            while !done.load(Ordering::SeqCst) {
                let mut all = vec![];
                files_under(&run, &mut all);
                for f in all {
                    if f.extension().is_some_and(|x| x == "tmp") {
                        seen_tmp.lock().unwrap().push(f.display().to_string());
                    }
                }
                thread::yield_now();
            }
        })
    };
    for i in 0..50 {
        let s = read_snapshot(&run).unwrap_or_else(|e| panic!("read {i}: {e}"));
        assert_eq!(s.state, RunState::Completed, "read {i}");
    }
    done.store(true, Ordering::SeqCst);
    watcher.join().unwrap();

    let after = fingerprint(&run.join("output"));
    let mut changed = vec![];
    for (path, (m, b)) in &before {
        match after.get(path) {
            None => changed.push(format!("{} disappeared", path.display())),
            Some((m2, b2)) => {
                if b != b2 {
                    changed.push(format!("{} bytes changed", path.display()));
                } else if m != m2 {
                    changed.push(format!("{} rewritten (mtime changed, same bytes)", path.display()));
                }
            }
        }
    }
    for path in after.keys().filter(|p| !before.contains_key(*p)) {
        changed.push(format!("{} appeared", path.display()));
    }
    assert!(changed.is_empty(), "50 reads of a healthy run changed output/: {changed:#?}");
    assert_eq!(fs::read(run.join("run_state.json")).unwrap(), state_before, "run_state.json changed by reads");
    let tmp = seen_tmp.lock().unwrap().clone();
    assert!(tmp.is_empty(), "a .tmp appeared during the reads: {tmp:?}");
}

// ---------------------------------------------------------------- VAL-R48-readonly

/// Makes `dir` unwritable; restores everything on drop.
struct Locked {
    files: Vec<PathBuf>,
    dirs: Vec<PathBuf>,
}
impl Locked {
    fn new(dir: &Path) -> Self {
        let (mut files, mut dirs) = (vec![], vec![]);
        files_under(dir, &mut files);
        dirs_under(dir, &mut dirs);
        for f in &files {
            let mut p = fs::metadata(f).unwrap().permissions();
            #[cfg(unix)]
            std::os::unix::fs::PermissionsExt::set_mode(&mut p, 0o444);
            #[cfg(not(unix))]
            p.set_readonly(true);
            fs::set_permissions(f, p).unwrap();
        }
        for d in &dirs {
            #[cfg(unix)]
            fs::set_permissions(d, std::os::unix::fs::PermissionsExt::from_mode(0o555)).unwrap();
            #[cfg(windows)]
            {
                let out = std::process::Command::new("icacls").arg(d).args(["/deny", "*S-1-1-0:(WD,AD,DC)"]).output().unwrap();
                assert!(out.status.success(), "icacls deny failed: {}", String::from_utf8_lossy(&out.stdout));
            }
        }
        Self { files, dirs }
    }
}
impl Drop for Locked {
    fn drop(&mut self) {
        for d in &self.dirs {
            #[cfg(unix)]
            let _ = fs::set_permissions(d, std::os::unix::fs::PermissionsExt::from_mode(0o755));
            #[cfg(windows)]
            {
                let _ = std::process::Command::new("icacls").arg(d).args(["/remove:d", "*S-1-1-0"]).output();
            }
            let _ = d;
        }
        for f in &self.files {
            if let Ok(meta) = fs::metadata(f) {
                let mut p = meta.permissions();
                #[cfg(unix)]
                std::os::unix::fs::PermissionsExt::set_mode(&mut p, 0o644);
                #[cfg(not(unix))]
                #[allow(clippy::permissions_set_readonly_false)]
                p.set_readonly(false);
                let _ = fs::set_permissions(f, p);
            }
        }
    }
}

fn readonly_reads(run: &Path, what: &str, target: &Path) {
    let st: serde_json::Value = serde_json::from_slice(&fs::read(run.join("run_state.json")).unwrap()).unwrap();
    let _lock = Locked::new(target);
    // Proof that the lock holds (L55): writing into the locked directory fails.
    let probe = target.join("probe.tmp");
    assert!(fs::write(&probe, b"x").is_err(), "{what}: the directory is still writable, the lock does not hold");
    let s = read_snapshot(run).unwrap_or_else(|e| panic!("{what}: read_snapshot of a consistent, read-only run failed: {e}"));
    assert_eq!(s.state, RunState::Completed, "{what}");
    assert_eq!(serde_json::to_value(s.state).unwrap(), st["snapshot"]["state"], "{what}: not the authoritative state");
    assert!(s.last_error.is_none(), "{what}: a healthy read-only run reports an error: {:?}", s.last_error);
    assert_eq!(s.rounds_done, st["snapshot"]["roundsDone"].as_u64().unwrap() as u32, "{what}: not the authoritative rounds");
}

#[cfg(windows)]
#[test]
fn test_dod1_val_r48_readonly_package_reads_ok_windows() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let run = completed_run(dir.path());
    readonly_reads(&run, "output/audit read-only", &run.join("output/audit"));
    readonly_reads(&run, "the whole output/ read-only", &run.join("output"));
}

#[cfg(unix)]
#[test]
fn test_dod1_val_r48_readonly_package_reads_ok_unix() {
    // SAFETY: geteuid has no preconditions.
    if unsafe { libc::geteuid() } == 0 {
        eprintln!("SKIPPED (printed reason): running as root, file permissions are ignored, so a read-only package cannot be built");
        return;
    }
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let run = completed_run(dir.path());
    readonly_reads(&run, "output/audit read-only", &run.join("output/audit"));
    readonly_reads(&run, "the whole output/ read-only", &run.join("output"));
}

// ---------------------------------------------------------------- VAL-R48-repair (control)

const DECIDER: &str = "Zelda-Approver-4471";

#[test]
fn test_dod1_val_r48_repair_control_inconsistent_package_is_repaired_on_read() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("a")).unwrap();
    let a = completed_run(&dir.path().join("a"));
    let b = dir.path().join("b-run");
    copy_dir(&a, &b);
    decide(&a, DecisionKind::Approved, DECIDER, "ok").unwrap();
    // b: run_state.json has no decision, but its human_decision.json says Approved.
    fs::copy(a.join("output/final/human_decision.json"), b.join("output/final/human_decision.json")).unwrap();
    let ahead = fs::read_to_string(b.join("output/final/human_decision.json")).unwrap();
    assert!(ahead.contains(DECIDER), "harness: the inconsistent package is not ahead of the state");
    let st: serde_json::Value = serde_json::from_slice(&fs::read(b.join("run_state.json")).unwrap()).unwrap();
    assert!(st["snapshot"]["decision"].is_null(), "harness: run_state.json must not hold the decision");

    let s = read_snapshot(&b).unwrap();
    assert_eq!(s.state, RunState::Completed);
    assert!(s.decision.is_none());
    let repaired = fs::read_to_string(b.join("output/final/human_decision.json")).unwrap();
    assert!(!repaired.contains(DECIDER), "the inconsistent package was not repaired on read: {repaired}");
    let manifest: serde_json::Value = serde_json::from_slice(&fs::read(b.join("output/audit/run_manifest.json")).unwrap()).unwrap();
    assert!(manifest["human_decision"].is_null(), "the manifest holds a decision the state does not");
}

#[test]
fn test_dod3_no_skip_or_ignore_in_this_file() {
    let src = include_str!("ct_r48_reads.rs");
    let code: String = src.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
    for banned in [format!("#[{}", "ignore"), format!("x{}", "fail"), format!("should_{}", "panic"), format!("catch_{}", "unwind")] {
        assert!(!code.contains(banned.as_str()), "found {banned}");
    }
}
