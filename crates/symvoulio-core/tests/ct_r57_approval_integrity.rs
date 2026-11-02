// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! Code review pass 7, item R57 (blocker): an edited revision file must never be published under an existing approval.
//! Expectations come from the finding, not from the code under test. After a run is approved and
//! `output/revisions/revision_01.{md,docx}` is edited or deleted, `read_snapshot` and then `export_report`
//! * must not publish the changed bytes: final/final_document.md, the document part of final_document.docx and
//!   final_review.html keep the approved content, OR the operation returns/records a named integrity error
//!   (kind publication/integrity, message naming the revision);
//! * must never rebind the approval: decision.document_sha256 is unchanged and equals the sha256 of what final/ holds
//!   (in both the snapshot and final/human_decision.json);
//! * control: a healthy approved run reads and exports unchanged.
use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};
use sha2::{Digest, Sha256};
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
const MARKER: &str = "Unreviewed claim after approval";
const FINAL: [&str; 3] = ["final/final_document.md", "final/final_document.docx", "final/final_review.html"];

/// An approved run through the real CLI adapters on the fake CLI (its default scenario).
fn approved_run(root: &Path) -> (PathBuf, RunSnapshot) {
    let primary = root.join("business.md");
    fs::write(&primary, "# Northwind\nCapital cost $4.2M. Growth 18 %. owner to be confirmed by Operations").unwrap();
    let cfg = RunConfig {
        run_id: "r57-run".into(),
        name: "r57".into(),
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
    assert!(s.rounds_done >= 1, "setup: the run has a committed revision");
    let s = decide(&run, DecisionKind::Approved, DECIDER, "ok").unwrap();
    assert_eq!(s.state, RunState::Approved, "setup: {:?}", s.last_error);
    assert!(revision(&run, &s, "md").is_file() && revision(&run, &s, "docx").is_file());
    (run, s)
}
/// The committed revision the approved final is rendered from (the default scenario runs several rounds).
fn revision(run: &Path, s: &RunSnapshot, ext: &str) -> PathBuf {
    run.join(format!("output/revisions/revision_{:02}.{ext}", s.rounds_done))
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
/// Content of a final artifact; a DOCX is compared by its document part (the zip container carries no content).
fn content(relative: &str, bytes: Vec<u8>) -> Vec<u8> {
    if !relative.ends_with(".docx") {
        return bytes;
    }
    let Ok(mut zip) = zip::ZipArchive::new(Cursor::new(bytes)) else {
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
fn finals(run: &Path) -> BTreeMap<&'static str, Vec<u8>> {
    FINAL.iter().map(|r| (*r, content(r, fs::read(run.join("output").join(r)).unwrap_or_default()))).collect()
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle.as_bytes())
}
/// A named integrity error: a publication/integrity kind, or any error, whose message names the revision.
fn names_revision(message: &str) -> bool {
    message.to_lowercase().contains("revision")
}
fn edited_docx() -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    docx_rs::Docx::new()
        .add_paragraph(docx_rs::Paragraph::new().add_run(docx_rs::Run::new().add_text(MARKER)))
        .build()
        .pack(&mut out)
        .unwrap();
    out.into_inner()
}

/// Runs read_snapshot then export_report and checks the R57 property after each.
fn assert_integrity(run: &Path, approved: &RunSnapshot, approved_finals: &BTreeMap<&'static str, Vec<u8>>, what: &str) {
    let approved_hash = approved.decision.as_ref().unwrap().document_sha256.clone();
    let read = read_snapshot(run);
    check(run, &approved_hash, approved_finals, what, "read_snapshot", read.as_ref().map(|s| s.last_error.clone()).map_err(|e| e.to_string()));
    if let Ok(s) = &read {
        assert_eq!(s.state, RunState::Approved, "{what}: read_snapshot changed the authoritative state");
        assert_eq!(s.decision.as_ref().unwrap().document_sha256, approved_hash, "{what}: read_snapshot rebound the approval hash");
    }
    let export = export_report(run).map_err(|e| e.to_string());
    check(run, &approved_hash, approved_finals, what, "export_report", export.map(|_| None));
    let after = read_snapshot(run);
    if let Ok(s) = &after {
        assert_eq!(s.decision.as_ref().unwrap().document_sha256, approved_hash, "{what}: the approval hash was rebound");
    }
}
fn check(
    run: &Path,
    approved_hash: &str,
    approved_finals: &BTreeMap<&'static str, Vec<u8>>,
    what: &str,
    op: &str,
    outcome: Result<Option<symvoulio_core::engine::LastError>, String>,
) {
    let now = finals(run);
    let changed: Vec<_> = FINAL.iter().filter(|r| now[**r] != approved_finals[**r]).collect();
    let edited: Vec<_> = FINAL.iter().filter(|r| contains(&now[**r], MARKER) || contains(&now[**r], "Changed after approval")).collect();
    let named = match &outcome {
        Err(message) => names_revision(message),
        Ok(Some(e)) => matches!(e.kind.as_str(), "publication" | "integrity") && names_revision(&e.message),
        Ok(None) => false,
    };
    assert!(edited.is_empty(), "{what}: {op} published the edited revision text into {edited:?} (outcome {outcome:?})");
    assert!(
        changed.is_empty() || named,
        "{what}: {op} changed the approved final {changed:?} without a named integrity error (outcome {outcome:?})"
    );
    let on_disk = sha(&fs::read(run.join("output/final/final_document.md")).unwrap_or_default());
    assert_eq!(on_disk, approved_hash, "{what}: {op}: the approval hash no longer equals the sha256 of final/final_document.md");
    let decision: serde_json::Value = serde_json::from_slice(&fs::read(run.join("output/final/human_decision.json")).unwrap()).unwrap();
    assert_eq!(
        decision["document_sha256"].as_str(),
        Some(approved_hash),
        "{what}: {op}: final/human_decision.json carries a different approval hash: {decision}"
    );
}

#[test]
fn test_val_r57_edited_revision_markdown_is_not_published_under_the_approval() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let (run, approved) = approved_run(dir.path());
    let before = finals(&run);
    let state_before = fs::read(run.join("run_state.json")).unwrap();
    fs::write(revision(&run, &approved, "md"), format!("# Changed after approval\n\n{MARKER}")).unwrap();
    assert_integrity(&run, &approved, &before, "edited committed revision .md");
    assert_eq!(fs::read(run.join("run_state.json")).unwrap(), state_before, "run_state.json was rewritten");
}

#[test]
fn test_val_r57_edited_revision_docx_only_is_not_published_under_the_approval() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let (run, approved) = approved_run(dir.path());
    let before = finals(&run);
    fs::write(revision(&run, &approved, "docx"), edited_docx()).unwrap();
    assert_integrity(&run, &approved, &before, "edited committed revision .docx");
}

#[test]
fn test_val_r57_deleted_revision_markdown_is_not_published_under_the_approval() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let (run, approved) = approved_run(dir.path());
    let before = finals(&run);
    fs::remove_file(revision(&run, &approved, "md")).unwrap();
    assert_integrity(&run, &approved, &before, "deleted committed revision .md");
}

#[test]
fn test_val_r57_control_healthy_approved_run_reads_and_exports_unchanged() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let (run, approved) = approved_run(dir.path());
    let stamp = |run: &Path| -> BTreeMap<PathBuf, (SystemTime, Vec<u8>)> {
        let mut files = vec![];
        files_under(&run.join("output"), &mut files);
        files.into_iter().map(|f| (f.clone(), (fs::metadata(&f).unwrap().modified().unwrap(), fs::read(&f).unwrap()))).collect()
    };
    let before = stamp(&run);
    let finals_before = finals(&run);
    std::thread::sleep(Duration::from_millis(50));
    for i in 0..3 {
        let s = read_snapshot(&run).unwrap();
        assert_eq!(s.state, RunState::Approved, "read {i}");
        assert!(s.last_error.is_none(), "read {i}: a healthy approved run reports {:?}", s.last_error);
        assert_eq!(s.decision.as_ref().unwrap().document_sha256, approved.decision.as_ref().unwrap().document_sha256);
    }
    assert!(before == stamp(&run), "reads of a healthy approved run changed output/ (bytes, mtime or files)");
    export_report(&run).unwrap();
    assert_eq!(finals(&run), finals_before, "an export of a healthy approved run changed the final artifacts");
    let s = read_snapshot(&run).unwrap();
    assert!(s.last_error.is_none() && s.state == RunState::Approved, "after export: {:?} {:?}", s.state, s.last_error);
    let mut all = vec![];
    files_under(&run, &mut all);
    assert!(all.iter().all(|f| f.extension().is_none_or(|x| x != "tmp")), "a .tmp remains");
}
