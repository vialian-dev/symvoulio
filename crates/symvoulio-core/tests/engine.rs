// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
use std::{
    fs,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use symvoulio_core::{
    cli,
    detect::{detect, AuthMethod},
    engine::*,
    prompts::*,
    provider::*,
    *,
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
        for key in [
            "SYMVOULIO_FAKE_CLI",
            "FAKE_CLI_SCENARIO",
            "FAKE_CLI_LOGGED_OUT",
            "FAKE_CLI_LOG",
        ] {
            std::env::remove_var(key);
        }
    }
}
fn clients() -> (Arc<dyn ProviderClient>, Arc<dyn ProviderClient>) {
    (Arc::new(ClaudeCli), Arc::new(CodexCli))
}
fn config(dir: &std::path::Path) -> RunConfig {
    let primary = dir.join("business.md");
    fs::write(
        &primary,
        "# Northwind\nCapital cost $4.2M. Growth 18 %. owner to be confirmed by Operations",
    )
    .unwrap();
    RunConfig {
        run_id: "test-run".into(),
        name: "Northwind".into(),
        run_dir: dir.join("run"),
        author: Provider::Claude,
        objective: "Improve for executive review".into(),
        criteria: vec!["All totals must reconcile.".into()],
        // The fake revisions add a line to a 12-word document: far over the default 10 % new-text limit, which
        // (R11) would keep every run going. Tests about other rules use 100 %; the contribution test sets its own.
        policy: Policy {
            new_content_limit_pct: 100.0,
            ..Policy::default()
        },
        primary,
        supporting: vec![],
        call_timeout: Duration::from_secs(5),
        author_model: None,
        reviewer_model: None,
    }
}
#[test]
fn blank_primary_is_refused_before_any_provider_call() {
    let _guard = ENV.lock().unwrap();
    let _env = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("argv.jsonl");
    std::env::set_var("FAKE_CLI_LOG", &log);
    for text in ["", " \r\n\t\u{2003}", "\u{feff} \n"] {
        let cfg = config(dir.path());
        fs::write(&cfg.primary, text).unwrap();
        let run = cfg.run_dir.clone();
        let error = start(cfg, clients(), Arc::default(), |_| {}).unwrap_err();
        assert!(error.to_string().contains("Primary document is empty or contains only whitespace"));
        assert!(!run.exists(), "rejected input must not create a run");
        assert!(!log.exists(), "rejected input must not invoke a provider");
    }
}
#[test]
fn detection_redacts_and_maps_both_streams() {
    let _guard = ENV.lock().unwrap();
    let _env = Env::new();
    for p in [Provider::Claude, Provider::Codex] {
        let d = detect(p);
        assert!(d.installed);
        assert_eq!(d.connected, Some(true));
        assert_eq!(d.method, Some(AuthMethod::Subscription));
        assert!(!format!("{d:?}{}", serde_json::to_string(&d).unwrap())
            .contains("fake.user@example.com"));
    }
    std::env::set_var("FAKE_CLI_LOGGED_OUT", "1");
    for p in [Provider::Claude, Provider::Codex] {
        assert_eq!(detect(p).connected, Some(false));
    }
}
#[test]
fn full_run_manifest_decisions_and_return() {
    let _guard = ENV.lock().unwrap();
    let _env = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path());
    let run = cfg.run_dir.clone();
    let log = dir.path().join("argv.jsonl");
    std::env::set_var("FAKE_CLI_LOG", &log);
    let s = start(cfg, clients(), Arc::default(), |_| {}).unwrap();
    assert_eq!(s.rounds_done, 3);
    assert_eq!(s.state, RunState::Completed);
    assert_eq!(s.stop_reason, Some(StopReason::GateNoNewIssues));
    assert_eq!(
        (s.counts.critical, s.counts.high, s.counts.medium),
        (0, 0, 2)
    );
    assert_eq!(s.documents.len(), 4);
    let expected = [
        "original/business.md",
        "final/final_document.docx",
        "final/final_document.md",
        "final/final_review.html",
        "final/unresolved_issues.json",
        "final/human_decision.json",
        "audit/run_manifest.json",
        "audit/timeline.json",
    ];
    for file in expected {
        assert!(s.output_tree.contains(&file.into()), "{file}");
    }
    for n in 1..=3 {
        for file in [
            format!("revisions/revision_{n:02}.docx"),
            format!("revisions/revision_{n:02}.md"),
            format!("reviews/author_response_{n:02}.json"),
            format!("reviews/review_{n:02}.json"),
            format!("audit/provider_output/round_{n:02}_author_claude_a01.jsonl"),
            format!("audit/provider_output/round_{n:02}_reviewer_codex_a01.jsonl"),
        ] {
            assert!(s.output_tree.contains(&file), "{file}");
        }
    }
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(run.join("output/audit/run_manifest.json")).unwrap())
            .unwrap();
    let schema =
        serde_json::from_str(include_str!("../../../schemas/run-manifest.schema.json")).unwrap();
    jsonschema::validator_for(&schema)
        .unwrap()
        .validate(&manifest)
        .unwrap();
    assert_eq!(manifest["author"]["model"], "claude-fake-1");
    assert!(manifest["reviewer"]["model"].is_null());
    let calls: Vec<Vec<String>> = fs::read_to_string(&log)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str::<Vec<String>>(s).unwrap())
        .filter(|args| !args.iter().any(|s| s == "--version"))
        .collect();
    assert_eq!(calls.len(), 6);
    let a = &calls[0];
    assert!(a.windows(2).any(|w| w == ["--tools", ""]));
    assert!(a.windows(2).any(|w| w == ["--permission-mode", "dontAsk"]));
    assert!(a.contains(&"--json-schema".into()));
    assert!(!a.contains(&"--bare".into()));
    let c = &calls[1];
    assert!(c.windows(2).any(|w| w == ["--sandbox", "read-only"]));
    assert!(c.contains(&"--skip-git-repo-check".into()));
    let approved = decide(&run, DecisionKind::Approved, "Owner", "Reviewed").unwrap();
    assert_eq!(approved.state, RunState::Approved);
    let decision: serde_json::Value =
        serde_json::from_slice(&fs::read(run.join("output/final/human_decision.json")).unwrap())
            .unwrap();
    assert_eq!(decision["kind"], "approved");
    let timeline = fs::read_to_string(run.join("output/audit/timeline.json")).unwrap();
    assert!(timeline.contains("human_decision"));
    decide(&run, DecisionKind::Returned, "Owner", "One more round").unwrap();
    let s = resume(&run, clients(), Arc::default(), |_| {}).unwrap();
    assert_eq!(s.rounds_done, 4);
    assert_eq!(s.state, RunState::Completed);
    assert_eq!(s.decision_log.len(), 2);
    decide_issue(&run, "R01-005", HumanChoice::Keep, "Accept dated source").unwrap();
    assert_eq!(load(&run).unwrap().counts.medium, 1);
    assert_eq!(list(dir.path()).len(), 1);
}
#[test]
fn auth_and_malformed_pause_then_resume() {
    let _guard = ENV.lock().unwrap();
    let _env = Env::new();
    for scenario in ["auth_fail", "malformed", "crash"] {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(dir.path());
        let run = cfg.run_dir.clone();
        std::env::set_var("FAKE_CLI_SCENARIO", scenario);
        let s = start(cfg, clients(), Arc::default(), |_| {}).unwrap();
        assert_eq!(s.state, RunState::Paused);
        assert_eq!(
            s.last_error.unwrap().kind,
            match scenario {
                "auth_fail" => "auth",
                "malformed" => "malformed",
                _ => "process",
            }
        );
        std::env::remove_var("FAKE_CLI_SCENARIO");
        assert_eq!(
            resume(&run, clients(), Arc::default(), |_| {})
                .unwrap()
                .rounds_done,
            3
        );
    }
}
#[test]
fn pause_between_calls_retries_only_reviewer_and_recovers_interruption() {
    let _guard = ENV.lock().unwrap();
    let _env = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path());
    let run = cfg.run_dir.clone();
    let control = Arc::new(RunControl::default());
    let c = control.clone();
    let s = start(cfg, clients(), control, move |e| {
        if let RunEvent::Line {
            role: Role::Author,
            line,
            ..
        } = e
        {
            if line.readable.as_deref() == Some("Completed") {
                c.pause.store(true, Ordering::SeqCst);
            }
        }
    })
    .unwrap();
    assert_eq!(s.state, RunState::Paused);
    assert_eq!(s.documents.len(), 2);
    assert!(run.join("output/revisions/revision_01.docx").is_file());
    assert!(!run.join("output/reviews/review_01.json").exists());
    let state = run.join("run_state.json");
    let mut raw: serde_json::Value = serde_json::from_slice(&fs::read(&state).unwrap()).unwrap();
    raw["snapshot"]["state"] = serde_json::json!("running");
    fs::write(state, serde_json::to_vec(&raw).unwrap()).unwrap();
    let recovered = load(&run).unwrap();
    assert_eq!(recovered.state, RunState::Paused);
    assert_eq!(recovered.last_error.unwrap().message, "interrupted");
    let s = resume(&run, clients(), Arc::default(), |_| {}).unwrap();
    assert_eq!(s.rounds_done, 3);
    assert_eq!(s.documents.len(), 4);
}
#[test]
fn slow_stop_keeps_first_round_and_kills_within_three_seconds() {
    let _guard = ENV.lock().unwrap();
    let _env = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path());
    let run = cfg.run_dir.clone();
    let control = Arc::new(RunControl::default());
    let c = control.clone();
    let timer = Arc::new(Mutex::new(None));
    let t = timer.clone();
    let s = start(cfg, clients(), control, move |e| match e {
        RunEvent::RoundCompleted { round: 1, .. } => std::env::set_var("FAKE_CLI_SCENARIO", "slow"),
        RunEvent::Phase {
            round: 2,
            role: Role::Author,
            ..
        } => {
            let c = c.clone();
            let t = t.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(250));
                *t.lock().unwrap() = Some(Instant::now());
                c.stop();
            });
        }
        _ => (),
    })
    .unwrap();
    assert!(timer.lock().unwrap().unwrap().elapsed() < Duration::from_secs(3));
    assert_eq!(s.state, RunState::Stopped);
    assert_eq!(s.stop_reason, Some(StopReason::UserStop));
    assert_eq!(s.rounds_done, 1);
    assert!(run.join("output/reviews/review_01.json").exists());
}
#[test]
fn timeout_and_cancel_are_bounded() {
    let _guard = ENV.lock().unwrap();
    let _env = Env::new();
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("FAKE_CLI_SCENARIO", "slow");
    let r = CallRequest {
        role: Role::Reviewer,
        round: 1,
        prompt: "ROUND: 1".into(),
        schema: SchemaKind::Review,
        workdir: dir.path().into(),
        timeout: Duration::from_millis(300),
        model: None,
    };
    let start = Instant::now();
    assert!(matches!(
        CodexCli.call(&r, &mut |_| {}, &Arc::new(AtomicBool::new(false))),
        Err(CallError::Timeout)
    ));
    assert!(start.elapsed() < Duration::from_secs(3));
    assert!(matches!(
        ClaudeCli.call(&r, &mut |_| {}, &Arc::new(AtomicBool::new(true))),
        Err(CallError::Cancelled)
    ));
}
#[test]
fn raw_shim_parsing_matches_measured_windows_launchers() {
    let dir = tempfile::tempdir().unwrap();
    let claude = dir.path().join("claude.cmd");
    let target = dir
        .path()
        .join("node_modules/@anthropic-ai/claude-code/bin/claude.exe");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, "").unwrap();
    fs::write(
        &claude,
        r#""%dp0%\node_modules\@anthropic-ai\claude-code\bin\claude.exe" %*"#,
    )
    .unwrap();
    let c = cli::parse_shim(&claude);
    assert_eq!(c.program, target);
    assert!(c.prefix_args.is_empty());
    let codex = dir.path().join("codex.cmd");
    let script = dir.path().join("node_modules/@openai/codex/bin/codex.js");
    fs::create_dir_all(script.parent().unwrap()).unwrap();
    fs::write(&script, "").unwrap();
    fs::write(dir.path().join("node.exe"), "").unwrap();
    fs::write(
        &codex,
        r#"node "%dp0%\node_modules\@openai\codex\bin\codex.js" %*"#,
    )
    .unwrap();
    let c = cli::parse_shim(&codex);
    assert_eq!(c.program, dir.path().join("node.exe"));
    assert_eq!(c.prefix_args, vec![script.to_string_lossy()]);
    let unknown = dir.path().join("unknown.cmd");
    fs::write(&unknown, "echo unknown").unwrap();
    assert_eq!(cli::parse_shim(&unknown).program, unknown);
}
#[test]
fn reviewer_context_excludes_author_responses() {
    let review: Review =
        serde_json::from_str(include_str!("../fixtures/fake/review-1.json")).unwrap();
    let response: AuthorResponse =
        serde_json::from_str(include_str!("../fixtures/fake/author-2.json")).unwrap();
    let p = reviewer_prompt(
        2,
        "Review",
        &[],
        &Policy::default(),
        "Original document",
        "Current document",
        &[("source.txt".into(), "Sources".into())],
        &review.issues,
    );
    for r in response.responses {
        assert!(!p.contains(&r.explanation));
    }
    assert!(!p.contains("proposed_action"));
    assert!(p.contains("ROUND: 2"));
    assert!(p.contains("<<<FILE source.txt"));
}
#[test]
fn first_resolved_report_is_history_only_and_low_is_not_material() {
    let mut review: Review =
        serde_json::from_str(include_str!("../fixtures/fake/review-1.json")).unwrap();
    review.issues.truncate(1);
    review.issues[0].status = Status::Resolved;
    let mut ledger = IssueLedger::default();
    ledger.apply_review(1, &review);
    assert_eq!(ledger.new_material_in(1), 0);
    assert_eq!(ledger.open_counts(), Counts::default());
    assert_eq!(ledger.history("R01-001").unwrap().reviews.len(), 1);
    let saved = serde_json::to_vec(&ledger).unwrap();
    let ledger: IssueLedger = serde_json::from_slice(&saved).unwrap();
    assert_eq!(ledger.new_material_in(1), 0);
    review.issues[0].status = Status::Open;
    review.issues[0].severity = Severity::Low;
    let mut ledger = IssueLedger::default();
    ledger.apply_review(1, &review);
    assert_eq!(ledger.new_material_in(1), 0);
    assert_eq!(ledger.open_counts().low, 1);
}
#[test]
fn validates_policy_and_provider_independence() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path());
    let result = start(
        cfg,
        (Arc::new(ClaudeCli), Arc::new(ClaudeCli)),
        Arc::default(),
        |_| {},
    );
    assert_eq!(
        result.unwrap_err().to_string(),
        "No model grades its own work."
    );
    let mut cfg = config(dir.path());
    cfg.policy.min_rounds = 6;
    assert!(start(cfg, clients(), Arc::default(), |_| {}).is_err());
}
#[test]
fn utc_timestamp_has_seconds_precision() {
    let s = now_utc_rfc3339();
    assert_eq!(s.len(), 20);
    assert!(s.ends_with('Z'));
}

#[test]
fn reviewer_auth_failure_resumes_without_repeating_author() {
    let _guard = ENV.lock().unwrap();
    let _env = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path());
    let run = cfg.run_dir.clone();
    let switched = AtomicBool::new(false);
    let s = start(cfg, clients(), Arc::default(), |e| {
        if let RunEvent::Phase {
            role: Role::Reviewer,
            ..
        } = e
        {
            if !switched.swap(true, Ordering::SeqCst) {
                std::env::set_var("FAKE_CLI_SCENARIO", "auth_fail");
            }
        }
    })
    .unwrap();
    assert_eq!(s.last_error.unwrap().kind, "auth");
    assert_eq!(s.documents.len(), 2);
    assert!(run
        .join("output/audit/provider_output/round_01_reviewer_codex_a01.jsonl")
        .is_file());
    std::env::remove_var("FAKE_CLI_SCENARIO");
    let s = resume(&run, clients(), Arc::default(), |_| {}).unwrap();
    assert_eq!(s.documents.len(), 4);
    assert_eq!(s.rounds_done, 3);
    let audit = run.join("output/audit/provider_output");
    assert!(
        audit.join("round_01_reviewer_codex_a01.jsonl").exists(),
        "first attempt evidence missing"
    );
    assert!(audit
        .join("round_01_reviewer_codex_a01.stderr.txt")
        .exists());
    assert!(audit.join("round_01_reviewer_codex_a02.jsonl").exists());
    let timeline: serde_json::Value =
        serde_json::from_slice(&fs::read(run.join("output/audit/timeline.json")).unwrap()).unwrap();
    let attempts: Vec<_> = timeline
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "review_started" && e["round"] == 1)
        .map(|e| e["attempt"].as_u64().unwrap())
        .collect();
    assert_eq!(attempts, vec![1, 2]);
}
#[test]
fn crash_after_terminal_review_does_not_start_an_extra_round() {
    let _guard = ENV.lock().unwrap();
    let _env = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path());
    let run = cfg.run_dir.clone();
    start(cfg, clients(), Arc::default(), |_| {}).unwrap();
    let path = run.join("run_state.json");
    let mut saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    saved["snapshot"]["state"] = serde_json::json!("running");
    fs::write(&path, serde_json::to_vec(&saved).unwrap()).unwrap();
    load(&run).unwrap();
    let log = dir.path().join("calls");
    std::env::set_var("FAKE_CLI_LOG", &log);
    let s = resume(&run, clients(), Arc::default(), |_| {}).unwrap();
    assert_eq!(s.rounds_done, 3);
    assert_eq!(s.state, RunState::Completed);
    assert!(!log.exists());
}
#[test]
fn artifact_access_error_is_a_recoverable_timeline_event() {
    let _guard = ENV.lock().unwrap();
    let _env = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path());
    let run = cfg.run_dir.clone();
    let bad = run.join("output/reviews/author_response_01.json.tmp");
    let s = start(cfg, clients(), Arc::default(), |e| {
        if let RunEvent::Phase {
            role: Role::Author,
            round: 1,
            ..
        } = e
        {
            fs::create_dir(&bad).unwrap();
        }
    })
    .unwrap();
    assert_eq!(s.state, RunState::Paused);
    assert_eq!(s.last_error.unwrap().kind, "storage");
    assert!(fs::read_to_string(run.join("output/audit/timeline.json"))
        .unwrap()
        .contains("provider_error"));
    fs::remove_dir(&bad).unwrap();
    let s = resume(&run, clients(), Arc::default(), |_| {}).unwrap();
    assert_eq!(s.rounds_done, 3);
}

/// Stop the run while the reviewer of round `n` works (the author's revision `n` is written).
fn stop_in_review_of(n: u32, control: Arc<RunControl>) -> impl Fn(RunEvent) {
    move |e| {
        if let RunEvent::Phase {
            round,
            role: Role::Reviewer,
            ..
        } = e
        {
            if round == n {
                std::env::set_var("FAKE_CLI_SCENARIO", "slow");
                let c = control.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(250));
                    c.stop();
                });
            }
        }
    }
}
/// Return is refused at the run's OWN max_rounds, not at a fixed 5.
fn completed_at_max(max: u32) -> (tempfile::TempDir, std::path::PathBuf, RunSnapshot) {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = config(dir.path());
    cfg.policy.max_rounds = max;
    cfg.policy.min_rounds = max;
    let run = cfg.run_dir.clone();
    let s = start(cfg, clients(), Arc::default(), |_| {}).unwrap();
    (dir, run, s)
}
#[test]
fn return_is_refused_at_the_runs_own_max_rounds_after_a_completed_last_round() {
    let _guard = ENV.lock().unwrap();
    let _env = Env::new();
    for max in [1, 3] {
        let (_dir, run, s) = completed_at_max(max);
        assert_eq!(s.rounds_done, max);
        let err = decide(&run, DecisionKind::Returned, "Owner", "").unwrap_err();
        assert!(
            err.to_string().contains("Maximum rounds reached"),
            "max {max}: {err}"
        );
        assert_eq!(load(&run).unwrap().decision_log.len(), 0);
        assert_eq!(
            decide(&run, DecisionKind::Approved, "Owner", "")
                .unwrap()
                .state,
            RunState::Approved
        );
    }
}
#[test]
fn return_is_refused_when_stopped_in_the_middle_of_the_runs_last_round() {
    let _guard = ENV.lock().unwrap();
    let _env = Env::new();
    for max in [1, 3] {
        std::env::remove_var("FAKE_CLI_SCENARIO");
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config(dir.path());
        cfg.policy.max_rounds = max;
        cfg.policy.min_rounds = max;
        let run = cfg.run_dir.clone();
        let control = Arc::new(RunControl::default());
        let s = start(
            cfg,
            clients(),
            control.clone(),
            stop_in_review_of(max, control),
        )
        .unwrap();
        assert_eq!(
            (s.state, s.rounds_done),
            (RunState::Stopped, max - 1),
            "max {max}"
        );
        let err = decide(&run, DecisionKind::Returned, "Owner", "").unwrap_err();
        assert!(
            err.to_string().contains("Maximum rounds reached"),
            "max {max}: {err}"
        );
    }
}
#[test]
fn return_is_allowed_below_the_runs_max_rounds() {
    let _guard = ENV.lock().unwrap();
    let _env = Env::new();
    let (_dir, run, s) = completed_at_max(5);
    assert_eq!(s.rounds_done, 5);
    // A run with max 5 that stopped by the gate after round 3 may return for round 4.
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path());
    let run3 = cfg.run_dir.clone();
    let s3 = start(cfg, clients(), Arc::default(), |_| {}).unwrap();
    assert_eq!(s3.rounds_done, 3);
    assert_eq!(
        decide(&run3, DecisionKind::Returned, "Owner", "")
            .unwrap()
            .state,
        RunState::Returned
    );
    assert!(decide(&run, DecisionKind::Returned, "Owner", "").is_err());
}
#[test]
fn return_after_a_mid_round_stop_finishes_that_review_and_keeps_the_revision() {
    let _guard = ENV.lock().unwrap();
    let _env = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path());
    let run = cfg.run_dir.clone();
    let control = Arc::new(RunControl::default());
    let s = start(
        cfg,
        clients(),
        control.clone(),
        stop_in_review_of(2, control),
    )
    .unwrap();
    assert_eq!((s.state, s.rounds_done), (RunState::Stopped, 1));
    let revision = run.join("output/revisions/revision_02.md");
    let written = fs::read_to_string(&revision).unwrap();
    std::env::remove_var("FAKE_CLI_SCENARIO");
    let log = dir.path().join("after-return.jsonl");
    std::env::set_var("FAKE_CLI_LOG", &log);
    decide(&run, DecisionKind::Returned, "Owner", "Finish round 2").unwrap();
    let s = resume(&run, clients(), Arc::default(), |_| {}).unwrap();
    assert_eq!(s.rounds_done, 2);
    assert_eq!(
        fs::read_to_string(&revision).unwrap(),
        written,
        "revision 2 must be kept, not rewritten"
    );
    let calls: Vec<String> = fs::read_to_string(&log)
        .unwrap()
        .lines()
        .map(String::from)
        .collect();
    assert_eq!(calls.len(), 1, "only the reviewer runs again: {calls:?}");
    assert!(calls[0].contains("\"exec\""), "{}", calls[0]);
    assert!(run.join("output/reviews/review_02.json").is_file());
}

#[test]
fn provider_isolation_flags_for_every_role() {
    let _guard = ENV.lock().unwrap();
    let _env = Env::new();
    let dir =
        tempfile::tempdir_in(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"))
            .unwrap();
    let log = dir.path().join("argv.jsonl");
    std::env::set_var("FAKE_CLI_LOG", &log);
    for author in [Provider::Claude, Provider::Codex] {
        let sub = dir.path().join(format!("{author:?}"));
        fs::create_dir_all(&sub).unwrap();
        let mut cfg = config(&sub);
        cfg.author = author;
        let pair = if author == Provider::Claude {
            clients()
        } else {
            (
                Arc::new(CodexCli) as Arc<dyn ProviderClient>,
                Arc::new(ClaudeCli) as Arc<dyn ProviderClient>,
            )
        };
        start(cfg, pair, Arc::default(), |_| {}).unwrap();
    }
    for line in fs::read_to_string(log).unwrap().lines() {
        let args: Vec<String> = serde_json::from_str(line).unwrap();
        if args.iter().any(|a| a == "--version") {
            continue;
        }
        let has = |flag: &str| args.iter().any(|a| a == flag);
        let pair = |flag: &str, value: &str| args.windows(2).any(|w| w[0] == flag && w[1] == value);
        if args[1] == "claude" {
            assert!(
                has("--strict-mcp-config"),
                "Claude strict MCP isolation missing"
            );
            assert!(has("--disable-slash-commands"));
            assert!(pair("--mcp-config", r#"{"mcpServers":{}}"#));
            assert!(pair("--tools", ""));
            let settings: serde_json::Value =
                serde_json::from_str(&args.windows(2).find(|w| w[0] == "--settings").unwrap()[1])
                    .unwrap();
            assert_eq!(
                settings,
                serde_json::json!({"disableAllHooks":true,"permissions":{"blockReadsOutsideWorkingDirectories":true}})
            );
        } else {
            for flag in ["--ignore-user-config", "--ignore-rules", "--ephemeral"] {
                assert!(has(flag), "missing {flag}");
            }
            for feature in [
                "shell_tool",
                "unified_exec",
                "hooks",
                "plugins",
                "apps",
                "browser_use",
                "computer_use",
                "in_app_browser",
            ] {
                assert!(pair("--disable", feature), "missing {feature}");
            }
            assert!(pair("--sandbox", "read-only"));
        }
    }
}
#[test]
fn fake_cli_rejects_unisolated_model_calls() {
    let _guard = ENV.lock().unwrap();
    let _env = Env::new();
    for provider in ["claude", "codex"] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_fake-cli"))
            .args(["--as", provider, "-p"])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert_eq!(
            String::from_utf8(output.stderr).unwrap().trim(),
            "isolation flags missing"
        );
    }
}

#[test]
fn final_uses_committed_revision_during_review_two_and_binds_approval() {
    let _guard = ENV.lock().unwrap();
    let _env = Env::new();
    let dir =
        tempfile::tempdir_in(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"))
            .unwrap();
    let cfg = config(dir.path());
    let run = cfg.run_dir.clone();
    let control = Arc::new(RunControl::default());
    let s = start(
        cfg,
        clients(),
        control.clone(),
        stop_in_review_of(2, control),
    )
    .unwrap();
    assert_eq!(s.rounds_done, 1);
    let final_text = fs::read_to_string(run.join("output/final/final_document.md")).unwrap();
    assert_eq!(
        final_text, s.documents[1],
        "final must be committed revision 1"
    );
    let approved = decide(&run, DecisionKind::Approved, "Owner", "Reviewed").unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(run.join("output/audit/run_manifest.json")).unwrap())
            .unwrap();
    use sha2::{Digest, Sha256};
    assert_eq!(manifest["human_decision"]["revision"], approved.rounds_done);
    assert_eq!(
        manifest["human_decision"]["document_sha256"],
        format!("{:x}", Sha256::digest(final_text.as_bytes()))
    );
}
#[test]
fn final_ignores_orphan_revision_without_saved_state() {
    let _guard = ENV.lock().unwrap();
    let _env = Env::new();
    let dir =
        tempfile::tempdir_in(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"))
            .unwrap();
    let cfg = config(dir.path());
    let run = cfg.run_dir.clone();
    let s = start(cfg, clients(), Arc::default(), |_| {}).unwrap();
    OutputPackage::reopen(&run, s.documents[0].clone())
        .write_revision(99, "Orphan")
        .unwrap();
    decide(&run, DecisionKind::Approved, "Owner", "").unwrap();
    assert_eq!(
        fs::read_to_string(run.join("output/final/final_document.md")).unwrap(),
        s.documents[s.rounds_done as usize],
        "orphan revision must not become final"
    );
}
#[test]
fn idle_stop_after_pause_or_auth_failure_allows_approval() {
    let _guard = ENV.lock().unwrap();
    let _env = Env::new();
    for auth in [false, true] {
        let dir = tempfile::tempdir_in(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"),
        )
        .unwrap();
        let cfg = config(dir.path());
        let run = cfg.run_dir.clone();
        let control = Arc::new(RunControl::default());
        if auth {
            std::env::set_var("FAKE_CLI_SCENARIO", "auth_fail");
        } else {
            control.pause.store(true, Ordering::SeqCst);
        }
        let s = start(cfg, clients(), control, |_| {}).unwrap();
        assert_eq!(s.state, RunState::Paused);
        if auth {
            assert_eq!(s.last_error.unwrap().kind, "auth");
        }
        let stopped = stop_idle(&run).unwrap();
        assert_eq!(
            stopped.state,
            RunState::Stopped,
            "idle Stop must persist Stopped"
        );
        assert_eq!(stopped.stop_reason, Some(StopReason::UserStop));
        assert!(fs::read_to_string(run.join("output/audit/timeline.json"))
            .unwrap()
            .contains("user_stop"));
        assert_eq!(
            decide(&run, DecisionKind::Approved, "Owner", "")
                .unwrap()
                .state,
            RunState::Approved
        );
        assert!(stop_idle(&run).is_err());
        std::env::remove_var("FAKE_CLI_SCENARIO");
    }
}
#[test]
fn original_is_separate_from_changed_revision() {
    let mut fixture: AuthorResponse =
        serde_json::from_str(include_str!("../fixtures/fake/author-1.json")).unwrap();
    fixture.revised_document = "Capital cost $42M.".into();
    let p = reviewer_prompt(
        1,
        "Compare numbers",
        &[],
        &Policy::default(),
        "Capital cost $4.2M.",
        &fixture.revised_document,
        &[],
        &[],
    );
    assert!(
        p.contains("<<<ORIGINAL_DOCUMENT\nCapital cost $4.2M.\nORIGINAL_DOCUMENT>>>"),
        "immutable original missing from reviewer prompt"
    );
    assert!(p.contains(&format!(
        "<<<CURRENT_DOCUMENT\n{}\nCURRENT_DOCUMENT>>>",
        fixture.revised_document
    )));
    let author = author_prompt(
        2,
        "Compare numbers",
        &[],
        &Policy::default(),
        "Capital cost $4.2M.",
        &fixture.revised_document,
        &[],
        &[],
    );
    assert!(author.contains("<<<ORIGINAL_DOCUMENT\nCapital cost $4.2M.\nORIGINAL_DOCUMENT>>>"));
    assert!(author.contains("<<<CURRENT_DOCUMENT\nCapital cost $42M.\nCURRENT_DOCUMENT>>>"));
    assert!(p.contains("Reviewer contribution mode: Correct"));
}
#[test]
fn author_question_reaches_snapshot_and_unresolved_export() {
    let _guard = ENV.lock().unwrap();
    let _env = Env::new();
    std::env::set_var("FAKE_CLI_SCENARIO", "human_question");
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path());
    let run = cfg.run_dir.clone();
    let s = start(cfg, clients(), Arc::default(), |_| {}).unwrap();
    let v = serde_json::to_value(&s).unwrap();
    assert_eq!(
        v["humanQuestions"][0]["question"], "Who owns the capital budget?",
        "author question missing from snapshot"
    );
    let unresolved = fs::read_to_string(run.join("output/final/unresolved_issues.json")).unwrap();
    assert!(unresolved.contains("Who owns the capital budget?"));
    assert!(
        fs::read_to_string(run.join("output/final/final_review.html"))
            .unwrap()
            .contains("Who owns the capital budget?")
    );
    // R53 (decision A): the id is assigned by our code, not the fake model's R01-099; read it from the snapshot.
    let qid = s.human_questions[0].issue_id.clone();
    assert!(qid.starts_with('Q'), "question id not assigned by our code: {qid}");
    let resolved = decide_issue(&run, &qid, HumanChoice::Accept, "Finance owns it").unwrap();
    assert_eq!(
        serde_json::to_value(resolved).unwrap()["humanQuestions"][0]["resolved"],
        true
    );
}
fn assert_malformed_scenario(scenario: &str, detail: &str) {
    let _guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _env = Env::new();
    std::env::set_var("FAKE_CLI_SCENARIO", scenario);
    let dir = tempfile::tempdir().unwrap();
    let s = start(config(dir.path()), clients(), Arc::default(), |_| {}).unwrap();
    assert_eq!(
        s.last_error.as_ref().map(|e| e.kind.as_str()),
        Some("malformed"),
        "{scenario} must give Malformed"
    );
    assert!(s.last_error.unwrap().message.contains(detail));
}
#[test]
fn validation_duplicate_id_is_malformed() {
    assert_malformed_scenario("duplicate_id", "duplicate issue id");
}
#[test]
fn validation_bad_id_is_malformed() {
    assert_malformed_scenario("bad_id", "issue_id");
}
#[test]
fn validation_missing_response_is_malformed() {
    assert_malformed_scenario("missing_response", "missing response");
}
#[test]
fn validation_empty_evidence_is_malformed() {
    assert_malformed_scenario("empty_evidence", "non-empty evidence");
}
#[test]
fn validation_unknown_resolved_is_recorded_closed() {
    let _guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _env = Env::new();
    std::env::set_var("FAKE_CLI_SCENARIO", "unknown_resolved");
    let dir = tempfile::tempdir().unwrap();
    let s = start(config(dir.path()), clients(), Arc::default(), |_| {}).unwrap();
    assert!(s.last_error.is_none(), "{:?}", s.last_error);
    assert!(s.issues.iter().any(|i| i.resolved_on_arrival.is_some() && i.issue.status == Status::Resolved));
}
#[test]
fn grandchild_pipe_timeout_is_bounded() {
    let _guard = ENV.lock().unwrap();
    let _env = Env::new();
    std::env::set_var("FAKE_CLI_SCENARIO", "grandchild");
    let dir = tempfile::tempdir().unwrap();
    let request = CallRequest {
        role: Role::Reviewer,
        round: 1,
        prompt: "ROUND: 1".into(),
        schema: SchemaKind::Review,
        workdir: dir.path().into(),
        timeout: Duration::from_millis(300),
        model: None,
    };
    let started = Instant::now();
    assert!(matches!(
        CodexCli.call(&request, &mut |_| {}, &Arc::default()),
        Err(CallError::Timeout)
    ));
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "grandchild held pipes beyond 3 seconds: {:?}",
        started.elapsed()
    );
}
#[test]
fn zero_contribution_limit_forces_continue_and_human_at_cap() {
    let _guard = ENV.lock().unwrap();
    let _env = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = config(dir.path());
    let run = cfg.run_dir.clone();
    cfg.policy.new_content_limit_pct = 0.0;
    cfg.policy.max_rounds = 2;
    cfg.policy.min_rounds = 1;
    cfg.policy.stop.no_new = false;
    cfg.policy.stop.no_critical = false;
    cfg.policy.stop.no_high = false;
    let s = start(cfg, clients(), Arc::default(), |_| {}).unwrap();
    assert!(s.rounds[0].contribution_exceeded);
    assert_eq!(
        serde_json::to_value(s.rounds[0].gate.reason).unwrap(),
        "continue_contribution",
        "exceeded contribution must force ContinueContribution"
    );
    assert_eq!(s.rounds[0].gate.kind, GateKind::Continue);
    assert_eq!(
        s.rounds.last().unwrap().gate.reason,
        GateReason::StoppedHuman
    );
    assert_eq!(s.state, RunState::Stopped);
    let report = fs::read_to_string(run.join("output/final/final_review.html")).unwrap();
    assert!(report.contains("Contribution limit exceeded"));
}
#[test]
fn secrets_never_reach_audit_state_or_events() {
    let _guard = ENV.lock().unwrap();
    let _env = Env::new();
    std::env::set_var("FAKE_CLI_SCENARIO", "secrets");
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path());
    let run = cfg.run_dir.clone();
    let events = Mutex::new(String::new());
    let s = start(cfg, clients(), Arc::default(), |e| {
        events.lock().unwrap().push_str(&format!("{e:?}\n"))
    })
    .unwrap();
    assert!(s.last_error.is_some());
    let mut surfaces = vec![
        ("events".into(), events.into_inner().unwrap()),
        (
            "run_state.json".into(),
            fs::read_to_string(run.join("run_state.json")).unwrap(),
        ),
    ];
    fn collect(path: &std::path::Path, out: &mut Vec<(String, String)>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                collect(&path, out);
            } else {
                out.push((
                    path.to_string_lossy().into(),
                    fs::read_to_string(&path).unwrap(),
                ));
            }
        }
    }
    collect(&run.join("output/audit"), &mut surfaces);
    let secrets = [
        "sk-testSyntheticKey",
        "sk-ant-testSyntheticKey",
        "syntheticBearerToken",
        "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0In0.syntheticSignature",
        "shortApiSecret",
        "shortAccessSecret",
        "shortRefreshSecret",
        "shortPassword",
        "ABCDEF0123456789ABCDEF0123456789",
        "QWxhZGRpbjpvcGVuIHNlc2FtZVN5bnRoZXRpYw==",
    ];
    let leaks: Vec<_> = surfaces
        .iter()
        .filter(|(_, text)| secrets.iter().any(|secret| text.contains(secret)))
        .map(|(name, _)| name.as_str())
        .collect();
    assert!(leaks.is_empty(), "synthetic secrets leaked to: {leaks:?}");
    let live = &surfaces[0].1;
    assert!(live.contains("Line {"), "provider events must still stream");
    assert!(
        live.contains("raw: \"\""),
        "raw output must be disabled by default"
    );
    assert!(
        !live.contains("api_key"),
        "provider text must not survive in controlled output"
    );
}

#[test]
fn aggregate_extracted_input_cap_lists_files() {
    let _guard = ENV.lock().unwrap();
    let _env = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = config(dir.path());
    fs::write(&cfg.primary, vec![b'x'; 4 * 1024 * 1024]).unwrap();
    let supporting = dir.path().join("support.txt");
    fs::write(&supporting, vec![b'x'; 4 * 1024 * 1024 + 1]).unwrap();
    cfg.supporting.push(supporting);
    let result = start(cfg, clients(), Arc::default(), |_| {});
    assert!(result.is_err(), "aggregate inputs over 8 MB were accepted");
    let message = result.unwrap_err().to_string();
    assert!(
        message.contains("business.md") && message.contains("support.txt"),
        "{message}"
    );
}

#[test]
fn complete_prompt_cap_pauses_before_provider_spawn() {
    let _guard = ENV.lock().unwrap();
    let _env = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = config(dir.path());
    cfg.objective = "x".repeat(9 * 1024 * 1024);
    let s = start(cfg, clients(), Arc::default(), |_| {}).unwrap();
    assert_eq!(
        s.state,
        RunState::Paused,
        "oversized prompt reached provider"
    );
    let error = s.last_error.unwrap();
    assert_eq!(error.kind, "input_too_large");
    assert!(error.message.contains("business.md"));
}

#[test]
fn moved_run_rebases_load_resume_and_saved_paths() {
    let _guard = ENV.lock().unwrap();
    let _env = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let mut cfg = config(dir.path());
    let support = dir.path().join("reference.txt");
    fs::write(&support, "Reference evidence").unwrap();
    cfg.supporting.push(support);
    let old = cfg.run_dir.clone();
    let control = Arc::new(RunControl::default());
    control.pause.store(true, Ordering::SeqCst);
    start(cfg, clients(), control, |_| {}).unwrap();
    let state = old.join("run_state.json");
    let mut saved: serde_json::Value = serde_json::from_slice(&fs::read(&state).unwrap()).unwrap();
    saved["snapshot"]["state"] = serde_json::json!("running");
    fs::write(state, serde_json::to_vec(&saved).unwrap()).unwrap();
    let moved = destination.path().join("portable");
    fs::rename(&old, &moved).unwrap();
    let loaded = load(&moved);
    assert!(
        loaded.is_ok(),
        "relocated run cannot load: {:?}",
        loaded.err()
    );
    let resumed = resume(&moved, clients(), Arc::default(), |_| {}).unwrap();
    assert_eq!(resumed.state, RunState::Completed);
    assert!(!old.exists(), "writes returned to old run root");
    assert!(moved.join("output/revisions/revision_01.md").exists());
    let saved: serde_json::Value =
        serde_json::from_slice(&fs::read(moved.join("run_state.json")).unwrap()).unwrap();
    assert_eq!(saved["config"]["run_dir"], ".");
    assert_eq!(saved["config"]["primary"], "inputs/business.md");
    assert_eq!(
        saved["config"]["supporting"][0],
        "inputs/supporting/reference.txt"
    );
}

#[test]
fn contract_question_answer_is_required_and_durable() {
    let _guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _env = Env::new();
    std::env::set_var("FAKE_CLI_SCENARIO", "human_question");
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path());
    let run = cfg.run_dir.clone();
    let control = Arc::new(RunControl::default());
    let pause = control.clone();
    let first = start(cfg, clients(), control, |event| {
        if matches!(event, RunEvent::RoundCompleted { round: 1, .. }) {
            pause.pause.store(true, Ordering::SeqCst);
        }
    })
    .unwrap();
    // R53 (decision A): the question id is assigned by our code; find it by the question text.
    let qid = first
        .human_questions
        .iter()
        .find(|q| q.question == "Who owns the capital budget?")
        .expect("question in the snapshot")
        .issue_id
        .clone();
    assert!(qid.starts_with('Q'), "question id not assigned by our code: {qid}");
    assert!(decide_issue(&run, &qid, HumanChoice::Accept, " ")
        .unwrap_err()
        .to_string()
        .contains("An answer is required"));
    decide_issue(&run, &qid, HumanChoice::Accept, "Finance").unwrap();
    let saved: serde_json::Value =
        serde_json::from_slice(&fs::read(run.join("run_state.json")).unwrap()).unwrap();
    // run_state.json keeps the camelCase key every released build wrote.
    let durable = &saved["snapshot"]["humanQuestions"][0];
    assert_eq!(durable["answer"], "Finance");
    assert!(durable["answeredAt"].as_str().is_some());
    assert!(durable["answeredBy"].is_null());
    let ledger: IssueLedger = serde_json::from_value(saved["ledger"].clone()).unwrap();
    let asked = format!("Answers from the human:\n- Q ({qid}): Who owns the capital budget?");
    assert!(
        human_instructions(&ledger)
            .contains(&asked)
            && human_instructions(&ledger).contains("\n  A: Finance\n  answered by unknown at ")
    );
    struct Capture(Arc<Mutex<Vec<String>>>);
    impl ProviderClient for Capture {
        fn provider(&self) -> Provider {
            Provider::Claude
        }
        fn call(
            &self,
            request: &CallRequest,
            sink: &mut dyn FnMut(StreamLine),
            cancel: &CancelToken,
        ) -> std::result::Result<CallOutcome, CallError> {
            self.0.lock().unwrap().push(request.prompt.clone());
            ClaudeCli.call(request, sink, cancel)
        }
    }
    let prompts = Arc::new(Mutex::new(Vec::new()));
    load(&run).unwrap();
    resume(
        &run,
        (Arc::new(Capture(prompts.clone())), Arc::new(CodexCli)),
        Arc::default(),
        |_| {},
    )
    .unwrap();
    let prompts = prompts.lock().unwrap();
    assert!(!prompts.is_empty());
    assert!(prompts.iter().all(|p| p
        .contains(&asked)
        && p.contains("\n  A: Finance\n  answered by unknown at ")));
    assert!(
        fs::read_to_string(run.join("output/final/final_review.html"))
            .unwrap()
            .contains("Finance")
    );
}

#[test]
fn contract_phase_and_pending_revision_are_authoritative() {
    let _guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _env = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path());
    let run = cfg.run_dir.clone();
    let control = Arc::new(RunControl::default());
    let c = control.clone();
    let s = start(cfg, clients(), control, |event| {
        if matches!(
            event,
            RunEvent::Phase {
                role: Role::Reviewer,
                ..
            }
        ) {
            let v: serde_json::Value =
                serde_json::from_slice(&fs::read(run.join("run_state.json")).unwrap()).unwrap();
            assert_eq!(v["snapshot"]["phase"], "reviewer");
            assert_eq!(v["snapshot"]["pendingRevision"], 1);
            c.stop();
        }
    })
    .unwrap();
    let v = serde_json::to_value(s).unwrap();
    assert!(v["phase"].is_null());
    assert_eq!(v["pendingRevision"], 1);
}

#[test]
fn contract_storage_loss_still_sends_terminal_events() {
    let _guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _env = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path());
    let run = cfg.run_dir.clone();
    let events = Mutex::new(Vec::new());
    let _ = start(cfg, clients(), Arc::default(), |event| {
        if matches!(event, RunEvent::Phase { .. }) && run.is_dir() {
            fs::remove_dir_all(&run).unwrap();
            fs::write(&run, "storage unavailable").unwrap();
        }
        events.lock().unwrap().push(event);
    });
    let events = events.lock().unwrap();
    assert!(events.iter().any(
        |e| matches!(e, RunEvent::ProviderError {kind, role: Role::Author, ..} if kind == "storage")
    ));
    assert!(events.iter().any(|e| matches!(
        e,
        RunEvent::State {
            state: RunState::Paused,
            stop_reason: None
        }
    )));
}
