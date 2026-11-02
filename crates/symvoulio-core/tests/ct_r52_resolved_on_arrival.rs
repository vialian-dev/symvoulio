// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! R52: a real reviewer, in round 1, reported a NEW issue (R01-003) with status "resolved": the problem was already
//! fixed in the revision it reviewed (the object is `.hardening/r52/real_review.json`, copied byte for byte to
//! `tests/fixtures/r52_real_review.json`, because the Linux build ships only crates/). The product rule:
//! * a new id reported as resolved is RECORDED AS CLOSED: not rejected, not downgraded to open;
//! * it stays visible: run_state.json (the issue list) and final_review.html say it was «reported already resolved in
//!   round N»;
//! * other malformed answers still reject the review: an unknown enum value, a missing field.
//!   The exact name of the ledger marker is not fixed by the rule: the test looks for the words «already resolved» (or
//!   `resolvedOnArrival` / `already_resolved`) in the JSON of that issue's entry, and for «reported already resolved in
//!   round 1» in the HTML. The reviewer answer is served by the fake CLI's opt-in knob FAKE_CLI_REVIEWER_FILE.
use std::{fs, path::Path, sync::Arc, sync::Mutex, time::Duration};
use symvoulio_core::{
    engine::{start, RunConfig},
    provider::{ClaudeCli, CodexCli, ProviderClient},
    Policy, Provider, RunState,
};

const REAL_REVIEW: &str = include_str!("fixtures/r52_real_review.json");

static ENV: Mutex<()> = Mutex::new(());
struct Env;
impl Env {
    fn new(reviewer_file: &Path) -> Self {
        std::env::set_var("SYMVOULIO_FAKE_CLI", env!("CARGO_BIN_EXE_fake-cli"));
        std::env::set_var("FAKE_CLI_REVIEWER_FILE", reviewer_file);
        std::env::remove_var("FAKE_CLI_SCENARIO");
        Self
    }
}
impl Drop for Env {
    fn drop(&mut self) {
        std::env::remove_var("SYMVOULIO_FAKE_CLI");
        std::env::remove_var("FAKE_CLI_REVIEWER_FILE");
    }
}

/// One round (Claude author, fake Codex reviewer serving `review_json`). Returns the snapshot and the run directory.
fn round_one(root: &Path, review_json: &str) -> (symvoulio_core::engine::RunSnapshot, std::path::PathBuf) {
    let served = root.join("reviewer_answer.json");
    fs::write(&served, review_json).unwrap();
    let _env = Env::new(&served);
    let primary = root.join("business.md");
    fs::write(&primary, "# Northwind\nCapital cost $4.6M. Growth 18 %.").unwrap();
    let cfg = RunConfig {
        run_id: "r52-run".into(),
        name: "r52".into(),
        run_dir: root.join("run"),
        author: Provider::Claude,
        objective: "Improve for executive review".into(),
        criteria: vec!["All factual claims must be supported.".into(), "All totals must reconcile.".into()],
        policy: Policy { max_rounds: 1, min_rounds: 1, new_content_limit_pct: 100.0, ..Policy::default() },
        primary,
        supporting: vec![],
        call_timeout: Duration::from_secs(10),
        author_model: None,
        reviewer_model: None,
    };
    let run = cfg.run_dir.clone();
    let clients: (Arc<dyn ProviderClient>, Arc<dyn ProviderClient>) = (Arc::new(ClaudeCli), Arc::new(CodexCli));
    (start(cfg, clients, Arc::default(), |_| {}).unwrap(), run)
}

#[test]
fn test_dod1_val_r52_new_issue_reported_resolved_is_recorded_as_closed_and_stays_visible() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let (s, run) = round_one(dir.path(), REAL_REVIEW);

    // The round completes; the review is not rejected.
    assert_ne!(s.state, RunState::Paused, "the round paused: {:?}", s.last_error);
    assert!(s.last_error.is_none(), "the review was rejected: {:?}", s.last_error);
    assert_eq!(s.rounds_done, 1, "round 1 did not complete");
    assert!(run.join("output/reviews/review_01.json").exists(), "the review was not stored");

    // The ledger (run_state.json): R01-003 closed, R01-001 and R01-002 open.
    let st: serde_json::Value = serde_json::from_slice(&fs::read(run.join("run_state.json")).unwrap()).unwrap();
    let issues = st["snapshot"]["issues"].as_array().expect("issue list");
    let find = |id: &str| issues.iter().find(|i| i["issue"]["issue_id"] == id).unwrap_or_else(|| panic!("{id} is not in the issue list: {issues:#?}"));
    assert_eq!(find("R01-001")["issue"]["status"], "open");
    assert_eq!(find("R01-002")["issue"]["status"], "open");
    let closed = find("R01-003");
    assert_eq!(closed["issue"]["status"], "resolved", "R01-003 must be recorded as closed, not downgraded to open");
    let entry = closed.to_string();
    let marker = regex::Regex::new(r"(?i)already[ _-]?resolved|resolved[ _-]?on[ _-]?arrival").unwrap();
    assert!(marker.is_match(&entry), "R01-003 carries no «reported already resolved» marker: {entry}");
    assert_eq!(s.counts.high, 1, "open counts: R01-001 (high) is open");
    assert_eq!(s.counts.low, 0, "the closed low issue must not count as open");

    // The export.
    let html = fs::read_to_string(run.join("output/final/final_review.html")).unwrap();
    assert!(html.contains("R01-003"), "final_review.html does not show R01-003");
    assert!(
        regex::Regex::new(r"(?i)reported already resolved in round 1").unwrap().is_match(&html),
        "final_review.html lacks «reported already resolved in round 1»"
    );
}

#[test]
fn test_dod1_val_r52_control_invalid_severity_enum_is_still_rejected() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let mut v: serde_json::Value = serde_json::from_str(REAL_REVIEW).unwrap();
    v["issues"][0]["severity"] = serde_json::json!("urgent");
    let (s, run) = round_one(dir.path(), &v.to_string());
    assert_eq!(s.state, RunState::Paused, "an unknown severity was accepted");
    assert_eq!(s.last_error.as_ref().map(|e| e.kind.as_str()), Some("malformed"), "{:?}", s.last_error);
    assert!(s.issues.is_empty(), "the rejected review entered the ledger");
    assert!(!run.join("output/reviews/review_01.json").exists(), "a rejected review was written");
}

#[test]
fn test_dod1_val_r52_control_missing_issues_field_is_still_rejected() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let mut v: serde_json::Value = serde_json::from_str(REAL_REVIEW).unwrap();
    v.as_object_mut().unwrap().remove("issues");
    let (s, run) = round_one(dir.path(), &v.to_string());
    assert_eq!(s.state, RunState::Paused, "a review without `issues` was accepted");
    assert_eq!(s.last_error.as_ref().map(|e| e.kind.as_str()), Some("malformed"), "{:?}", s.last_error);
    assert!(s.issues.is_empty());
    assert!(!run.join("output/reviews/review_01.json").exists(), "a rejected review was written");
}

#[test]
fn test_dod3_no_skip_or_ignore_in_this_file() {
    let src = include_str!("ct_r52_resolved_on_arrival.rs");
    let code: String = src.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
    for banned in [format!("#[{}", "ignore"), format!("x{}", "fail"), format!("should_{}", "panic"), format!("catch_{}", "unwind")] {
        assert!(!code.contains(banned.as_str()), "found {banned}");
    }
}
