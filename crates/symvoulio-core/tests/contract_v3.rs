// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! Backend contract regression tests, independent of UI ordering.
use std::{
    fs,
    path::Path,
    sync::{atomic::Ordering, Arc, Mutex},
    time::Duration,
};
use symvoulio_core::{
    engine::{export_report, load, resume, start, RunConfig, RunControl, RunEvent, RunSnapshot},
    output::OutputPackage,
    provider::{
        validate_answer, CallError, CallOutcome, CallRequest, CancelToken, ProviderClient,
        SchemaKind, StreamLine,
    },
    HumanChoice, IssueLedger, Policy, Provider, Role,
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

// ---------------------------------------------------------------- scripted providers

type Handler = Arc<
    dyn Fn(&CallRequest, &mut dyn FnMut(StreamLine)) -> Result<CallOutcome, CallError>
        + Send
        + Sync,
>;
struct Fake {
    provider: Provider,
    handler: Handler,
}
impl ProviderClient for Fake {
    fn provider(&self) -> Provider {
        self.provider
    }
    fn call(
        &self,
        req: &CallRequest,
        sink: &mut dyn FnMut(StreamLine),
        _cancel: &CancelToken,
    ) -> Result<CallOutcome, CallError> {
        (self.handler)(req, sink)
    }
}
fn clients(h: Handler) -> (Arc<dyn ProviderClient>, Arc<dyn ProviderClient>) {
    (
        Arc::new(Fake {
            provider: Provider::Claude,
            handler: h.clone(),
        }),
        Arc::new(Fake {
            provider: Provider::Codex,
            handler: h,
        }),
    )
}
fn handler(
    f: impl Fn(&CallRequest, &mut dyn FnMut(StreamLine)) -> Result<CallOutcome, CallError>
        + Send
        + Sync
        + 'static,
) -> Handler {
    Arc::new(f)
}
fn outcome(structured: serde_json::Value) -> CallOutcome {
    CallOutcome {
        structured,
        model: None,
        session_id: None,
        usage: None,
        stdout_lines: vec![],
        stderr: String::new(),
        duration_s: 0.1,
        exit_code: Some(0),
    }
}
fn issue(id: &str, severity: &str) -> serde_json::Value {
    serde_json::json!({
        "issue_id": id, "severity": severity, "criterion": "c", "finding": format!("finding text of {id}"),
        "evidence": "evidence text", "proposed_action": "fix it",
        "human_decision_required": false, "status": "open"
    })
}
fn review(issues: Vec<serde_json::Value>) -> serde_json::Value {
    serde_json::json!({
        "summary": "s", "issues": issues,
        "criteria_results": [{"criterion": "c", "met": true, "note": "n"}]
    })
}
fn author(questions: Vec<serde_json::Value>, round: u32) -> serde_json::Value {
    serde_json::json!({
        "revised_document": format!("# Doc\n\nrevision {round}"),
        "changes_made": ["c"], "responses": [], "new_claims": [],
        "human_decisions_required": questions
    })
}
fn question(id: &str, text: &str) -> serde_json::Value {
    serde_json::json!({"issue_id": id, "question": text})
}
fn policy(max_rounds: u32) -> Policy {
    Policy {
        max_rounds,
        min_rounds: 1,
        new_content_limit_pct: 100.0,
        ..Policy::default()
    }
}
fn config(root: &Path, p: Policy) -> RunConfig {
    let primary = root.join("doc.md");
    fs::write(&primary, "# Doc\n\noriginal").unwrap();
    RunConfig {
        run_id: "ct3-run".into(),
        name: "ct3".into(),
        run_dir: root.join("run"),
        author: Provider::Claude,
        objective: "Improve".into(),
        criteria: vec!["c".into()],
        policy: p,
        primary,
        supporting: vec![],
        call_timeout: Duration::from_secs(5),
        author_model: None,
        reviewer_model: None,
    }
}
/// Runs until a round completes, then pauses the run (so that a human can answer).
fn start_and_pause_after_round1(cfg: RunConfig, h: Handler) -> RunSnapshot {
    let control = Arc::new(RunControl::default());
    let c = control.clone();
    start(cfg, clients(h), control, move |e| {
        if matches!(e, RunEvent::RoundCompleted { .. }) {
            c.pause.store(true, Ordering::SeqCst);
        }
    })
    .unwrap()
}
#[test]
fn ordering_and_explicit_answerer_survive_reload() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path(), policy(3));
    let run = cfg.run_dir.clone();
    let first = start_and_pause_after_round1(
        cfg,
        handler(|req, _| {
            Ok(outcome(match req.role {
                Role::Author => {
                    author(vec![question("R01-099", "Who owns the budget?")], req.round)
                }
                Role::Reviewer => review(vec![]),
            }))
        }),
    );
    // R53 (decision A): the question id is assigned by our code; find it by the question text.
    let qid = first
        .human_questions
        .iter()
        .find(|q| q.question == "Who owns the budget?")
        .expect("question in the snapshot")
        .issue_id
        .clone();
    assert!(qid.starts_with('Q'), "question id not assigned by our code: {qid}");
    let answered = symvoulio_core::engine::decide_issue_with_answerer(
        &run,
        &qid,
        HumanChoice::Accept,
        "Finance",
        Some("Ada"),
    )
    .unwrap();
    assert_eq!(
        answered.human_questions[0].answered_by.as_deref(),
        Some("Ada")
    );
    assert_eq!(answered.generation, first.generation);
    assert!(answered.revision > first.revision);
    let c = Arc::new(RunControl::default());
    c.pause.store(true, Ordering::SeqCst);
    let resumed = resume(
        &run,
        clients(handler(|_, _| panic!("paused before call"))),
        c,
        |_| {},
    )
    .unwrap();
    assert!(resumed.generation > answered.generation);
    assert!(resumed.revision > answered.revision);
    assert_eq!(load(&run).unwrap().revision, resumed.revision);
    export_report(&run).unwrap();
    let report = fs::read_to_string(run.join("output/final/final_review.html")).unwrap();
    for text in [
        "Who owns the budget?",
        "Finance",
        "Ada",
        answered.human_questions[0].answered_at.as_deref().unwrap(),
    ] {
        assert!(report.contains(text));
    }
}
#[test]
fn question_namespace_includes_resolved_questions() {
    let mut ledger = IssueLedger::default();
    let a = author(vec![question("R01-099", "Who?")], 1);
    ledger.apply_author(1, &serde_json::from_value(a.clone()).unwrap());
    // R53 (decision A): the ids live in our namespace (Q-ids), assigned by the ledger, resolved ones included.
    let qid = ledger.human_questions()[0].issue_id.clone();
    assert!(qid.starts_with('Q'), "question id not assigned by our code: {qid}");
    assert!(ledger.apply_human(&qid, HumanChoice::Accept, "Ada", 1));
    // The model's reused id is ignored: a different question is a distinct record and the answered one is untouched.
    let changed = author(vec![question("R01-099", "Who else?")], 2);
    validate_answer(SchemaKind::AuthorResponse, &changed, &ledger, 2).unwrap();
    ledger.apply_author(2, &serde_json::from_value(changed).unwrap());
    let qs = ledger.human_questions();
    assert_eq!(qs.len(), 2);
    assert_ne!(qs[0].issue_id, qs[1].issue_id);
    assert_eq!((qs[0].question.as_str(), qs[0].resolved), ("Who?", true));
    // A reviewer finding cannot take the id of a question, resolved or not; the model's old id is just a finding id now.
    for q in qs {
        assert!(validate_answer(
            SchemaKind::Review,
            &review(vec![issue(&q.issue_id, "low")]),
            &ledger,
            2
        )
        .is_err());
    }
    validate_answer(SchemaKind::Review, &review(vec![issue("R01-099", "low")]), &ledger, 2).unwrap();
}
#[test]
fn identical_question_reasks_preserve_one_answered_record() {
    let mut ledger = IssueLedger::default();
    let first = author(vec![question("R01-099", "Who owns the budget?")], 1);
    validate_answer(SchemaKind::AuthorResponse, &first, &ledger, 1).unwrap();
    ledger.apply_author(1, &serde_json::from_value(first).unwrap());
    let qid = ledger.human_questions()[0].issue_id.clone();
    assert!(ledger.apply_human(&qid, HumanChoice::Accept, "Finance", 1));
    ledger.set_question_answerer(&qid, Some("Ada"));
    let stored = ledger.human_questions().to_vec();
    for round in 2..=3 {
        let repeated = author(vec![question("R01-099", "Who owns the budget?")], round);
        validate_answer(SchemaKind::AuthorResponse, &repeated, &ledger, round).unwrap();
        ledger.apply_author(round, &serde_json::from_value(repeated).unwrap());
        assert_eq!(ledger.human_questions().len(), 1);
        assert_eq!(ledger.human_questions(), stored);
    }
}
#[test]
fn live_summary_does_not_echo_provider_fields() {
    let text = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"password: hunter2"}]}}"#;
    let summary = symvoulio_core::provider::controlled_summary(text);
    assert!(summary.contains("2 words"));
    assert!(!summary.contains("hunter2"));
    assert_eq!(
        symvoulio_core::provider::controlled_summary("unrecognized secret"),
        "Provider event"
    );
}
#[test]
fn failed_package_preflight_creates_no_directory() {
    let dir = tempfile::tempdir().unwrap();
    let primary = dir.path().join("doc.md");
    fs::write(&primary, "private").unwrap();
    let run = dir.path().join("run");
    assert!(OutputPackage::create(&run, &primary, &[dir.path().join("missing.md")]).is_err());
    assert!(!run.exists());
}

#[test]
fn docx_number_formats_overrides_and_level_restarts_are_visible() {
    use docx_rs::*;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("numbered.docx");
    let level = |n, start, fmt, text: &str| {
        Level::new(
            n,
            Start::new(start),
            NumberFormat::new(fmt),
            LevelText::new(text),
            LevelJc::new("left"),
        )
    };
    let item = |text: &str, n| {
        Paragraph::new()
            .numbering(NumberingId::new(7), IndentLevel::new(n))
            .add_run(Run::new().add_text(text))
    };
    Docx::new()
        .add_abstract_numbering(
            AbstractNumbering::new(7)
                .add_level(level(0, 1, "upperRoman", "%1."))
                .add_level(level(1, 1, "lowerLetter", "%1.%2")),
        )
        .add_numbering(Numbering::new(7, 7).add_override(LevelOverride::new(0).start(3)))
        .add_paragraph(item("First", 0))
        .add_paragraph(item("Child", 1))
        .add_paragraph(item("Second", 0))
        .add_paragraph(item("Restarted child", 1))
        .build()
        .pack(fs::File::create(&path).unwrap())
        .unwrap();
    let text = symvoulio_core::extract(&path).unwrap().text;
    for expected in [
        "III. First",
        "III.a Child",
        "IV. Second",
        "IV.a Restarted child",
    ] {
        assert!(text.contains(expected), "{expected} missing in {text}");
    }
}
#[test]
fn raw_opt_in_and_tool_observation_are_transient() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    for kind in [
        "command_execution",
        "mcp_tool_call",
        "web_search",
        "file_change",
    ] {
        assert!(symvoulio_core::provider::is_tool_execution(
            &serde_json::json!({"type":"item.completed","item":{"type":kind}}).to_string()
        ));
    }
    assert!(!symvoulio_core::provider::is_tool_execution(
        r#"{"type":"item.completed","item":{"type":"agent_message","text":"No tool"}}"#
    ));
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path(), policy(1));
    let run = cfg.run_dir.clone();
    let observed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observer = observed.clone();
    let evidence = Arc::new(Mutex::new(Vec::<String>::new()));
    let capture = evidence.clone();
    let mut control = RunControl::default();
    control.raw_observer = Some(Arc::new(move |line| capture.lock().unwrap().push(line.to_owned())));
    control.raw_output.store(true, Ordering::SeqCst);
    control.tool_observer = Some(Arc::new(move || {
        observer.fetch_add(1, Ordering::SeqCst);
    }));
    let lines = Arc::new(Mutex::new(Vec::new()));
    let out = lines.clone();
    start(cfg, clients(handler(|req, sink| {
        sink(StreamLine {raw:r#"{"type":"item.completed","item":{"type":"command_execution","command":"private-command token=known-secret-8812"}}"#.into(), readable:None, is_stderr:false});
        Ok(outcome(match req.role {Role::Author=>author(vec![],1),Role::Reviewer=>review(vec![])}))
    })), Arc::new(control), move |event| {if let RunEvent::Line {line,..}=event {out.lock().unwrap().push(line);}}).unwrap();
    assert_eq!(observed.load(Ordering::SeqCst), 2);
    assert!(evidence.lock().unwrap().iter().all(|line| line.contains("known-secret-8812")));
    assert!(lines
        .lock()
        .unwrap()
        .iter()
        .all(|line| line.raw.contains("private-command")
            && !line.raw.contains("known-secret-8812")
            && line.raw.contains("[REDACTED]")
            && !line.readable.as_ref().unwrap().contains("private-command")));
    fn visit(path: &Path) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(&path);
            } else {
                assert!(
                    !String::from_utf8_lossy(&fs::read(path).unwrap()).contains("private-command")
                );
            }
        }
    }
    visit(&run);
    assert!(!RunControl::default().raw_output.load(Ordering::SeqCst));
}

#[test]
fn return_command_already_has_the_new_generation() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path(), policy(3));
    let run = cfg.run_dir.clone();
    let first = start(
        cfg,
        clients(handler(|req, _| {
            Ok(outcome(match req.role {
                Role::Author => author(vec![], req.round),
                Role::Reviewer => review(vec![]),
            }))
        })),
        Arc::default(),
        |_| {},
    )
    .unwrap();
    let returned = symvoulio_core::engine::decide(
        &run,
        symvoulio_core::DecisionKind::Returned,
        "Ada",
        "Review again",
    )
    .unwrap();
    assert!(returned.generation > first.generation);
    assert!(returned.revision > first.revision);
    let control = Arc::new(RunControl::default());
    control.pause.store(true, Ordering::SeqCst);
    let next = resume(
        &run,
        clients(handler(|_, _| panic!("paused"))),
        control,
        |_| {},
    )
    .unwrap();
    assert_eq!(next.generation, returned.generation);
    assert!(next.revision > returned.revision);
}

#[test]
fn paused_rounds_resume_to_the_saved_final_human_gate_without_duplicate_calls() {
    use symvoulio_core::{RunState, StopReason};
    for cause in ["questions", "contribution", "findings"] {
        for pause_round in 1..=3 {
            let root = tempfile::tempdir().unwrap();
            let mut p = policy(3);
            p.min_rounds = 3;
            if cause == "contribution" { p.new_content_limit_pct = 0.0; }
            let cfg = config(root.path(), p);
            let run = cfg.run_dir.clone();
            let calls = Arc::new(Mutex::new(Vec::new()));
            let record = calls.clone();
            let h = handler(move |req, _| {
                record.lock().unwrap().push((req.round, req.role));
                Ok(outcome(match req.role {
                    Role::Author => {
                        let mut a = author(if cause == "questions" { vec![question("R01-099", "Who verifies this?")] } else { vec![] }, req.round);
                        if cause == "findings" && req.round > 1 {
                            a["responses"] = serde_json::json!([{"issue_id":"R01-001", "response":"human_decision_required", "explanation":"Needs a person", "evidence":"source"}]);
                        }
                        a
                    },
                    Role::Reviewer => review(if cause == "findings" { vec![issue("R01-001", "high")] } else { vec![] }),
                }))
            });
            let control = Arc::new(RunControl::default());
            let pause = control.clone();
            let first = start(cfg, clients(h.clone()), control, move |event| {
                if matches!(event, RunEvent::RoundCompleted { round, .. } if round == pause_round) {
                    pause.pause.store(true, Ordering::SeqCst);
                }
            }).unwrap();
            assert_eq!(first.state, RunState::Paused, "{cause}, round {pause_round}");
            assert_eq!(first.rounds_done, pause_round);
            // Load from disk before resuming, including the final-round saved-gate boundary.
            assert_eq!(load(&run).unwrap().state, RunState::Paused);
            let done = resume(&run, clients(h), Arc::default(), |_| {}).unwrap();
            assert_eq!(done.stop_reason, Some(StopReason::GateHumanReviewRequired));
            assert_eq!(done.rounds_done, 3);
            assert_eq!(done.rounds.last().unwrap().gate.reason, symvoulio_core::gate::GateReason::StoppedHuman);
            // Legacy persistence vocabulary is intentionally retained; the product projects this final gate as completed.
            assert_eq!(done.state, RunState::Stopped);
            assert_eq!(calls.lock().unwrap().len(), 6, "{cause}, round {pause_round}");
            assert_eq!(done.rounds.last().unwrap().contribution_exceeded, cause == "contribution");
        }
    }
}

#[test]
fn html_keeps_all_response_kinds_for_resolved_findings_and_escapes_provider_text() {
    let root = tempfile::tempdir().unwrap();
    let cfg = config(root.path(), policy(2));
    let run = cfg.run_dir.clone();
    let kinds = ["accepted", "rejected_with_evidence", "partially_accepted", "human_decision_required"];
    let payload = "<script>alert('x')</script> & \"source\"";
    let done = start(cfg, clients(handler(move |req, _| {
        Ok(outcome(match req.role {
            Role::Author => {
                let mut a = author(vec![], req.round);
                if req.round == 2 {
                    a["responses"] = kinds.iter().enumerate().map(|(i, kind)| serde_json::json!({
                        "issue_id": format!("R01-{:03}", i + 1), "response": kind,
                        "explanation": payload, "evidence": payload
                    })).collect();
                }
                a
            },
            Role::Reviewer => review((1..=4).map(|i| {
                let mut finding = issue(&format!("R01-{i:03}"), "medium");
                if req.round == 2 { finding["status"] = serde_json::json!("resolved"); }
                finding
            }).collect()),
        }))
    })), Arc::default(), |_| {}).unwrap();
    assert_eq!(done.rounds_done, 2);
    assert_eq!(done.counts.medium, 0);
    export_report(&run).unwrap();
    let html = fs::read_to_string(run.join("output/final/final_review.html")).unwrap();
    let section = html.split("<h2>Author responses</h2>").nth(1).unwrap().split("</table>").next().unwrap();
    assert_eq!(section.matches("<tr>").count(), 5, "header plus every stored response, including resolved findings");
    for (i, kind) in kinds.iter().enumerate() {
        assert!(section.contains(&format!("<td>R01-{:03}</td><td>2</td><td>{kind}</td>", i + 1)));
    }
    assert!(!html.contains("<script>"));
    assert_eq!(section.matches("&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt; &amp; &quot;source&quot;").count(), 8);
}
