// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! Code review pass 6, items R55 and R54: recovery regressions of the R48 read fast path. Expectations come from the finding:
//! * R55 an artifact-read failure (denied read ACL, or a sharing violation from another handle) must never hide an
//!   authoritative approved run: `read_snapshot`/`load` return Ok with state Approved, the decision, and, when the
//!   package could not be verified, a `publication` last_error (the product's "publication incomplete" warning);
//! * R54 the consistency check must not call an incompletely repaired package healthy: after an interrupted repair
//!   (stale timeline + mismatching manifest + obstructed `timeline.json.tmp`), the next read must leave every derived
//!   artifact equal to a fresh export, and an existing stale artifact is repaired or reported, never accepted;
//! * control: a healthy approved package is not rewritten by reads (R48 stays).
//!
//! The desktop `runs_list` lists every run for which `engine::load`/`read_snapshot` returns Ok (commands.rs:1025), so
//! the engine-level cases cover it; `AppState::get` is private to the src-tauri crate and has no integration-test seam.
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use symvoulio_core::{
    engine::{decide, export_report, read_snapshot, start, RunConfig, RunSnapshot},
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

const DECIDER: &str = "Zelda-Approver-4471";
const DERIVED: [&str; 7] = [
    "audit/run_manifest.json",
    "audit/timeline.json",
    "final/human_decision.json",
    "final/final_document.md",
    "final/final_document.docx",
    "final/unresolved_issues.json",
    "final/final_review.html",
];

/// An approved run through the real CLI adapters on the fake CLI (its default scenario).
fn approved_run(root: &Path) -> PathBuf {
    let primary = root.join("business.md");
    fs::write(&primary, "# Northwind\nCapital cost $4.2M. Growth 18 %. owner to be confirmed by Operations").unwrap();
    let cfg = RunConfig {
        run_id: "r55-run".into(),
        name: "r55".into(),
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
    let s = decide(&run, DecisionKind::Approved, DECIDER, "ok").unwrap();
    assert_eq!(s.state, RunState::Approved, "setup: {:?}", s.last_error);
    run
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
/// Content of a derived artifact; a DOCX is compared by its document part (the zip container carries no content).
fn content(relative: &str, bytes: Vec<u8>) -> Vec<u8> {
    if !relative.ends_with(".docx") {
        return bytes;
    }
    let Ok(mut zip) = zip::ZipArchive::new(std::io::Cursor::new(bytes)) else {
        return b"<not a docx>".to_vec();
    };
    let mut xml = vec![];
    match zip.by_name("word/document.xml") {
        Ok(mut part) => {
            let _ = part.read_to_end(&mut xml);
        }
        Err(_) => xml = b"<no document part>".to_vec(),
    }
    xml
}
fn package(run: &Path) -> BTreeMap<&'static str, Vec<u8>> {
    DERIVED
        .iter()
        .map(|r| (*r, content(r, fs::read(run.join("output").join(r)).unwrap_or_else(|e| panic!("{r}: {e}")))))
        .collect()
}
fn assert_package_is(run: &Path, expected: &BTreeMap<&'static str, Vec<u8>>, what: &str) {
    let now = package(run);
    let stale: Vec<_> = DERIVED.iter().filter(|r| now[**r] != expected[**r]).collect();
    assert!(stale.is_empty(), "{what}: derived artifacts differ from a fresh export of the committed state: {stale:?}");
}
fn assert_approved(s: &RunSnapshot, what: &str) {
    assert_eq!(s.state, RunState::Approved, "{what}: not the authoritative state");
    assert!(s.decision.as_ref().is_some_and(|d| d.name == DECIDER), "{what}: the saved human decision is missing: {:?}", s.decision);
}
// ---------------------------------------------------------------- the control for the fixture itself

#[test]
fn test_val_r54_control_fresh_export_of_a_copy_matches_the_package() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let run = approved_run(dir.path());
    let committed = package(&run);
    let copy = dir.path().join("copy");
    copy_dir(&run, &copy);
    export_report(&copy).unwrap();
    assert_package_is(&copy, &committed, "explicit export of a copy is not reproducible, the comparison below would be unsound");
}

// ---------------------------------------------------------------- R55 (Windows)

#[cfg(windows)]
mod windows {
    use super::*;
    use std::os::windows::fs::OpenOptionsExt;
    use symvoulio_core::engine::load;

    /// A warning, when present, is the product's publication warning.
    fn assert_only_publication_warning(s: &RunSnapshot, what: &str) {
        if let Some(e) = &s.last_error {
            assert_eq!(e.kind, "publication", "{what}: unexpected last_error {e:?}");
        }
    }

    /// Denies the read of one file to Everyone; restored on drop.
    struct DenyRead(PathBuf);
    impl DenyRead {
        fn new(file: &Path) -> Self {
            let out = std::process::Command::new("icacls").arg(file).args(["/deny", "*S-1-1-0:(RD)"]).output().unwrap();
            assert!(out.status.success(), "icacls deny failed: {}", String::from_utf8_lossy(&out.stdout));
            assert!(fs::read(file).is_err(), "harness: {} is still readable, the deny does not hold", file.display());
            Self(file.to_path_buf())
        }
    }
    impl Drop for DenyRead {
        fn drop(&mut self) {
            let _ = std::process::Command::new("icacls").arg(&self.0).args(["/remove:d", "*S-1-1-0"]).output();
        }
    }
    /// Holds a file open with no sharing at all; every other open fails with a sharing violation.
    fn hold(file: &Path) -> fs::File {
        let handle = fs::OpenOptions::new().read(true).share_mode(0).open(file).unwrap();
        assert!(fs::read(file).is_err(), "harness: {} is still readable, the exclusive handle does not hold", file.display());
        handle
    }

    fn check_reads(run: &Path, what: &str, must_warn: bool) {
        for (via, snapshot) in [("read_snapshot", read_snapshot(run)), ("load", load(run))] {
            let s = snapshot.unwrap_or_else(|e| panic!("{what}: {via} of an approved run failed: {e}"));
            assert_approved(&s, &format!("{what}: {via}"));
            assert_only_publication_warning(&s, &format!("{what}: {via}"));
            if must_warn {
                assert_eq!(
                    s.last_error.as_ref().map(|e| e.kind.as_str()),
                    Some("publication"),
                    "{what}: {via} returned the approved run without the publication-incomplete warning"
                );
            }
        }
    }

    #[test]
    fn test_val_r55_acl_denied_final_markdown_still_returns_approved_with_warning() {
        let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let _e = Env::new();
        let dir = tempfile::tempdir().unwrap();
        let run = approved_run(dir.path());
        let target = run.join("output/final/final_document.md");
        {
            let _deny = DenyRead::new(&target);
            check_reads(&run, "read denied on final_document.md", true);
        }
        // Control: once access returns, the next read is healthy again.
        let s = read_snapshot(&run).unwrap();
        assert_approved(&s, "after restore");
        assert!(s.last_error.is_none(), "after restore the read still reports {:?}", s.last_error);
    }

    #[test]
    fn test_val_r55_sharing_denied_final_markdown_still_returns_approved_with_warning() {
        let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let _e = Env::new();
        let dir = tempfile::tempdir().unwrap();
        let run = approved_run(dir.path());
        let target = run.join("output/final/final_document.md");
        {
            let _held = hold(&target);
            check_reads(&run, "final_document.md held open with share_mode(0)", true);
        }
        let s = read_snapshot(&run).unwrap();
        assert_approved(&s, "after release");
        assert!(s.last_error.is_none(), "after release the read still reports {:?}", s.last_error);
    }

    #[test]
    fn test_val_r55_every_derived_artifact_unreadable_never_hides_the_approved_run() {
        let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let _e = Env::new();
        let dir = tempfile::tempdir().unwrap();
        let base = approved_run(dir.path());
        let mut failures = vec![];
        for (i, relative) in DERIVED.iter().enumerate() {
            for mode in ["acl-deny-read", "share-mode-0"] {
                let run = dir.path().join(format!("copy-{i}-{mode}"));
                copy_dir(&base, &run);
                let target = run.join("output").join(relative);
                let what = format!("{relative} [{mode}]");
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    if mode == "acl-deny-read" {
                        let _deny = DenyRead::new(&target);
                        check_reads(&run, &what, false);
                    } else {
                        let _held = hold(&target);
                        check_reads(&run, &what, false);
                    }
                }));
                if let Err(p) = outcome {
                    failures.push(p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or(what));
                }
            }
        }
        assert!(failures.is_empty(), "approved run hidden by an unreadable derived artifact:\n{}", failures.join("\n"));
    }

    #[test]
    fn test_val_r55_unreadable_committed_revision_still_returns_approved() {
        // The consistency check also reads output/revisions/revision_NN.md to know what final_document.md must hold.
        let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let _e = Env::new();
        let dir = tempfile::tempdir().unwrap();
        let run = approved_run(dir.path());
        let mut revisions = vec![];
        files_under(&run.join("output/revisions"), &mut revisions);
        revisions.retain(|p| p.extension().is_some_and(|x| x == "md"));
        revisions.sort();
        let target = revisions.last().expect("setup: a completed run has a committed revision").clone();
        let _deny = DenyRead::new(&target);
        check_reads(&run, "read denied on the committed revision", false);
    }
}

// ---------------------------------------------------------------- R54

fn mismatching_manifest(run: &Path) {
    let path = run.join("output/audit/run_manifest.json");
    let mut v: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    v["zz_stale_marker"] = serde_json::json!(true);
    fs::write(path, serde_json::to_vec_pretty(&v).unwrap()).unwrap();
}

#[test]
fn test_val_r54_interrupted_repair_second_read_must_not_leave_a_stale_timeline() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let run = approved_run(dir.path());
    let fresh = package(&run);
    let timeline = run.join("output/audit/timeline.json");
    fs::write(&timeline, b"[]").unwrap();
    mismatching_manifest(&run);
    fs::create_dir(run.join("output/audit/timeline.json.tmp")).unwrap();

    let first = read_snapshot(&run).unwrap();
    assert_approved(&first, "first read");
    assert_eq!(
        first.last_error.as_ref().map(|e| e.kind.as_str()),
        Some("publication"),
        "harness: the obstructed repair must report a publication error"
    );

    fs::remove_dir(run.join("output/audit/timeline.json.tmp")).unwrap();
    let second = read_snapshot(&run).unwrap();
    assert_approved(&second, "second read");
    assert_package_is(
        &run,
        &fresh,
        &format!("after the obstruction was removed the second read says {:?} / last_error {:?}", second.state, second.last_error),
    );
    assert!(!run.join("output/audit/timeline.json.tmp").exists());
}

#[test]
fn test_val_r54_stale_existing_artifact_is_repaired_or_reported_never_accepted() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let base = approved_run(dir.path());
    let fresh = package(&base);
    let mut failures = vec![];
    // The artifacts that `artifacts_agree` only tests for existence.
    for (i, relative) in ["audit/timeline.json", "final/final_document.docx", "final/unresolved_issues.json", "final/final_review.html"].iter().enumerate() {
        let run = dir.path().join(format!("stale-{i}"));
        copy_dir(&base, &run);
        let stale: &[u8] = if relative.ends_with(".json") { b"[\"stale\"]" } else { b"stale, from an earlier generation" };
        fs::write(run.join("output").join(relative), stale).unwrap();
        let s = read_snapshot(&run).unwrap();
        assert_approved(&s, relative);
        let repaired = fs::read(run.join("output").join(relative)).ok().map(|b| content(relative, b));
        let current = repaired.as_deref() == Some(fresh[*relative].as_slice());
        let reported = s.last_error.as_ref().is_some_and(|e| e.kind == "publication");
        if !current && !reported {
            failures.push(format!("{relative}: stale file kept and the read is healthy (state {:?}, last_error None)", s.state));
        }
    }
    assert!(failures.is_empty(), "stale derived artifacts silently accepted:\n{}", failures.join("\n"));
}

// ---------------------------------------------------------------- control: healthy reads rewrite nothing

#[test]
fn test_val_r54_control_healthy_approved_package_is_not_rewritten_by_reads() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let run = approved_run(dir.path());
    let stamp = |run: &Path| -> BTreeMap<PathBuf, (std::time::SystemTime, Vec<u8>)> {
        let mut files = vec![];
        files_under(&run.join("output"), &mut files);
        files.into_iter().map(|f| (f.clone(), (fs::metadata(&f).unwrap().modified().unwrap(), fs::read(&f).unwrap()))).collect()
    };
    let before = stamp(&run);
    std::thread::sleep(Duration::from_millis(50));
    for i in 0..5 {
        let s = read_snapshot(&run).unwrap();
        assert_approved(&s, &format!("read {i}"));
        assert!(s.last_error.is_none(), "read {i}: a healthy approved run reports {:?}", s.last_error);
    }
    assert!(before == stamp(&run), "reads of a healthy approved package changed output/ (bytes, mtime or files)");
    let mut all = vec![];
    files_under(&run, &mut all);
    assert!(all.iter().all(|f| f.extension().is_none_or(|x| x != "tmp")), "a .tmp remains");
}

#[test]
fn test_no_skip_or_ignore_in_this_file() {
    let src = include_str!("ct_r55_r54_recovery.rs");
    let code: String = src.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
    for banned in [format!("#[{}", "ignore"), format!("x{}", "fail"), format!("should_{}", "panic")] {
        assert!(!code.contains(banned.as_str()), "found {banned}");
    }
}
