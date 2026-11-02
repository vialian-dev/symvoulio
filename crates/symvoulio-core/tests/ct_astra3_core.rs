// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! CARD AST-E (cruel tester): the review's pass-3 backend failures R32, R34, R35, R36, R37, R38.
//! Expectations come from the card, not from the code under test.
//!
//! Rules tested (the card leaves them open, so they are stated here):
//! * R32  committed state is authoritative: a failed `decide` publishes no approval under `output/` that
//!   `run_state.json` does not hold, and a reload repairs derived artifacts that run ahead of the state.
//! * R34  the answer travels WITH its question text; `decide_issue` has no answerer parameter, so no identity is
//!   supplied: `answeredBy` must then not be the final decider's name (nor invented).
//! * R35  a question id is immutable. Reusing an id for a different question is either rejected as Malformed with
//!   nothing written, or both records survive under distinct ids; answering one never touches the other.
//!   Question ids and finding ids share ONE namespace for the whole ledger (both directions are rejected).
//! * R36  no value of the listed credentials, and no arbitrary unknown text, reaches a live `RunEvent::Line`.
//!   Engine-made progress (`Phase`, `State`) is the allowlisted text and must still arrive.
//! * R37  DOCX numbering (start, nesting, restart) is extracted as visible identifiers; plain paragraphs are not
//!   numbered; export -> reimport keeps the identifiers.
//! * R38  a failed package creation leaves nothing behind, and never deletes a pre-existing run.
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use symvoulio_core::{
    engine::{decide, decide_issue, export_report, load, resume, start, stop_idle, RunConfig, RunControl, RunEvent, RunSnapshot},
    extract,
    output::OutputPackage,
    provider::{validate_answer, CallError, CallOutcome, CallRequest, CancelToken, ProviderClient, SchemaKind, StreamLine},
    redact, DecisionKind, HumanChoice, IssueLedger, Policy, Provider, Review, Role, RunState,
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

type Handler = Arc<dyn Fn(&CallRequest, &mut dyn FnMut(StreamLine)) -> Result<CallOutcome, CallError> + Send + Sync>;
struct Fake {
    provider: Provider,
    handler: Handler,
}
impl ProviderClient for Fake {
    fn provider(&self) -> Provider {
        self.provider
    }
    fn call(&self, req: &CallRequest, sink: &mut dyn FnMut(StreamLine), _cancel: &CancelToken) -> Result<CallOutcome, CallError> {
        (self.handler)(req, sink)
    }
}
fn clients(h: Handler) -> (Arc<dyn ProviderClient>, Arc<dyn ProviderClient>) {
    (
        Arc::new(Fake { provider: Provider::Claude, handler: h.clone() }),
        Arc::new(Fake { provider: Provider::Codex, handler: h }),
    )
}
fn handler(
    f: impl Fn(&CallRequest, &mut dyn FnMut(StreamLine)) -> Result<CallOutcome, CallError> + Send + Sync + 'static,
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
    Policy { max_rounds, min_rounds: 1, new_content_limit_pct: 100.0, ..Policy::default() }
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
fn state_json(run: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(run.join("run_state.json")).unwrap()).unwrap()
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
fn resume_and_pause_after_round(run: &Path, h: Handler) -> RunSnapshot {
    let control = Arc::new(RunControl::default());
    let c = control.clone();
    resume(run, clients(h), control, move |e| {
        if matches!(e, RunEvent::RoundCompleted { .. }) {
            c.pause.store(true, Ordering::SeqCst);
        }
    })
    .unwrap()
}

// ---------------------------------------------------------------- VAL-R32

const DECIDER: &str = "Zelda-Approver-4471";

/// A one-round run that completes; `n` low findings widen the report-writing window.
fn completed_run(root: &Path, n: usize) -> PathBuf {
    fs::create_dir_all(root).unwrap();
    let cfg = config(root, policy(1));
    let run = cfg.run_dir.clone();
    let issues: Vec<_> = (1..=n).map(|i| issue(&format!("R01-{i:03}"), "low")).collect();
    let h = handler(move |req, _| {
        Ok(outcome(match req.role {
            Role::Author => author(vec![], req.round),
            Role::Reviewer => review(issues.clone()),
        }))
    });
    let s = start(cfg, clients(h), Arc::default(), |_| {}).unwrap();
    assert_eq!(s.state, RunState::Completed, "setup: {:?}", s.last_error);
    run
}
/// No file under output/ may record an approval, a manifest state or an event that run_state.json does not hold.
fn assert_derived_agrees(run: &Path, when: &str) {
    let st = state_json(run);
    let committed_decision = &st["snapshot"]["decision"];
    let out = run.join("output");
    let read_json = |rel: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(out.join(rel)).unwrap_or_else(|e| panic!("{when}: output/{rel}: {e}"))).unwrap()
    };
    if committed_decision.is_null() {
        let mut all = vec![];
        files_under(&out, &mut all);
        for f in all {
            let text = String::from_utf8_lossy(&fs::read(&f).unwrap()).into_owned();
            assert!(
                !text.contains(DECIDER),
                "{when}: {} records an approval by {DECIDER} that run_state.json does not hold",
                f.strip_prefix(run).unwrap().display()
            );
        }
        assert!(read_json("final/human_decision.json").is_null(), "{when}: human_decision.json is not null");
        assert!(read_json("audit/run_manifest.json")["human_decision"].is_null(), "{when}: manifest holds a decision");
    } else {
        assert_eq!(read_json("final/human_decision.json")["name"], DECIDER, "{when}: committed decision missing in output");
        assert_eq!(read_json("audit/run_manifest.json")["human_decision"]["name"], DECIDER, "{when}: manifest");
    }
    assert_eq!(
        read_json("audit/run_manifest.json")["state"], st["snapshot"]["state"],
        "{when}: manifest state differs from the committed state"
    );
    assert_eq!(
        read_json("audit/timeline.json").as_array().unwrap().len(),
        st["timeline"].as_array().unwrap().len(),
        "{when}: audit timeline has events the committed state does not hold"
    );
}

/// Fault: `run_state.json.tmp` becomes a directory the moment the approval appears in the STAGED decision
/// (`<run>/.publication/output/final/human_decision.json`), i.e. after the report is staged and before the state
/// commit of the SECOND save inside `decide`. The published output is never watched: it only receives the approval
/// after the commit (AST-F 2.5). The test fails when the fault never fired.
#[test]
fn test_dod1_val_r32_failed_state_commit_publishes_no_approval() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let mut faulted = 0;
    for attempt in 1..=30 {
        let dir = tempfile::tempdir().unwrap();
        let run = completed_run(dir.path(), 200);
        let done = Arc::new(AtomicBool::new(false));
        let watcher = {
            let (run, done) = (run.clone(), done.clone());
            thread::spawn(move || {
                let staged = run.join(".publication/output/final/human_decision.json");
                let tmp = run.join("run_state.json.tmp");
                let t0 = Instant::now();
                while !done.load(Ordering::SeqCst) && t0.elapsed() < Duration::from_secs(60) {
                    if fs::read_to_string(&staged).is_ok_and(|t| t.contains(DECIDER)) {
                        let _ = fs::create_dir(&tmp);
                        break;
                    }
                    thread::yield_now();
                }
            })
        };
        let result = decide(&run, DecisionKind::Approved, DECIDER, "ok");
        done.store(true, Ordering::SeqCst);
        watcher.join().unwrap();
        // The fault is proven fired by BOTH the observed error and the obstruction still standing.
        let failed = result.is_err() && run.join("run_state.json.tmp").is_dir();
        if failed {
            faulted += 1;
        }
        assert_derived_agrees(&run, &format!("attempt {attempt}, decide failed={failed}, before any reload"));
        let _ = fs::remove_dir(run.join("run_state.json.tmp")); // the fault is cleared before the reload
        load(&run).unwrap();
        assert_derived_agrees(&run, &format!("attempt {attempt}, decide failed={failed}, after reload"));
        if failed {
            break;
        }
    }
    eprintln!("r32 watcher: fault fired in {faulted} attempt(s)");
    assert!(faulted > 0, "L55: the injected fault never fired in 30 attempts (decide never failed), so nothing was tested");
}

/// Control (green by design): a state file that can never be written. The first save inside decide fails too.
#[test]
fn test_dod1_val_r32_control_persistent_state_fault_publishes_no_approval() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let run = completed_run(dir.path(), 3);
    fs::create_dir(run.join("run_state.json.tmp")).unwrap();
    let r = decide(&run, DecisionKind::Approved, DECIDER, "ok");
    assert!(r.is_err(), "a decide whose state cannot be committed must return an error");
    assert_derived_agrees(&run, "persistent fault");
    fs::remove_dir(run.join("run_state.json.tmp")).unwrap();
    assert_eq!(load(&run).unwrap().state, RunState::Completed);
    assert_derived_agrees(&run, "persistent fault, after reload");
}

/// Crash-equivalent: the process died after the second save wrote the approval under output/ and before the state
/// commit. The image is built from a real approval: output/ of the approved run over the pre-approval state.
#[test]
fn test_dod1_val_r32_reload_repairs_derived_artifacts_that_run_ahead_of_the_state() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let a = completed_run(&dir.path().join("a"), 3);
    let b = dir.path().join("b-run");
    copy_dir(&a, &b);
    decide(&a, DecisionKind::Approved, DECIDER, "ok").unwrap();
    assert_derived_agrees(&a, "control: the approved run itself");
    for rel in ["output/final/human_decision.json", "output/audit/run_manifest.json", "output/final/final_review.html", "output/audit/timeline.json"] {
        fs::copy(a.join(rel), b.join(rel)).unwrap();
    }
    assert!(fs::read_to_string(b.join("output/final/human_decision.json")).unwrap().contains(DECIDER), "harness: crash image is not ahead of the state");
    assert!(state_json(&b)["snapshot"]["decision"].is_null(), "harness: state must be the pre-approval commit");
    let snap = load(&b).unwrap();
    assert_eq!(snap.state, RunState::Completed);
    assert!(snap.decision.is_none());
    assert_derived_agrees(&b, "after load of the crash image");
}

// ---------------------------------------------------------------- VAL-R34

const Q1: &str = "Who owns the capital budget (q-7c1)?";
const A1: &str = "Finance-Dept-7731";

fn prompts_log() -> Arc<Mutex<Vec<(Role, u32, String)>>> {
    Arc::new(Mutex::new(vec![]))
}
/// Round 1 author asks `Q1` as R01-099; every later author answers nothing new. Reviewer is clean. Prompts are logged.
fn qa_handler(log: Arc<Mutex<Vec<(Role, u32, String)>>>) -> Handler {
    handler(move |req, _| {
        log.lock().unwrap().push((req.role, req.round, req.prompt.clone()));
        Ok(outcome(match (req.role, req.round) {
            (Role::Author, 1) => author(vec![question("R01-099", Q1)], 1),
            (Role::Author, n) => author(vec![], n),
            (Role::Reviewer, _) => review(vec![]),
        }))
    })
}
fn answered_run(root: &Path, log: Arc<Mutex<Vec<(Role, u32, String)>>>) -> PathBuf {
    let cfg = config(root, policy(3));
    let run = cfg.run_dir.clone();
    let s = start_and_pause_after_round1(cfg, qa_handler(log));
    assert_eq!(s.state, RunState::Paused, "setup: {:?}", s.last_error);
    assert_eq!(s.human_questions.len(), 1);
    decide_issue(&run, &question_id(&s, Q1), HumanChoice::Accept, A1).unwrap();
    run
}
/// R53 (decision A): the id of a question is assigned by our code, so it is read from the snapshot by its text.
fn question_id(s: &RunSnapshot, text: &str) -> String {
    let id = s.human_questions.iter().find(|q| q.question == text).unwrap_or_else(|| panic!("no question {text:?}: {:?}", s.human_questions)).issue_id.clone();
    assert!(id.starts_with('Q'), "question id not assigned by our code: {id}");
    id
}

#[test]
fn test_dod1_val_r34_later_author_prompts_carry_the_question_text_with_its_answer() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let log = prompts_log();
    let run = answered_run(dir.path(), log.clone());
    let before = log.lock().unwrap().len();
    let s = resume(&run, clients(qa_handler(log.clone())), Arc::default(), |_| {}).unwrap();
    assert_eq!(s.state, RunState::Completed, "{:?}", s.last_error);
    let later: Vec<String> = log.lock().unwrap()[before..].iter().filter(|(r, _, _)| *r == Role::Author).map(|(_, _, p)| p.clone()).collect();
    assert!(!later.is_empty(), "no author call after the answer");
    for prompt in later {
        let q = prompt.find(Q1).unwrap_or_else(|| panic!("the author prompt lacks the question text: {prompt}"));
        let a = prompt.find(A1).unwrap_or_else(|| panic!("the author prompt lacks the answer: {prompt}"));
        assert!(q.abs_diff(a) <= 300, "question (at {q}) and its answer (at {a}) are not one record");
    }
}

#[test]
fn test_dod1_val_r34_report_and_export_carry_question_and_answer_after_reload() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let run = answered_run(dir.path(), prompts_log());
    let s = load(&run).unwrap();
    let q = &s.human_questions[0];
    assert_eq!((q.question.as_str(), q.answer.as_deref()), (Q1, Some(A1)));
    assert!(q.resolved);
    let at = q.answered_at.as_deref().expect("answer timestamp");
    assert!(at.len() == 20 && at.ends_with('Z') && at.as_bytes()[10] == b'T', "not an RFC 3339 UTC timestamp: {at}");
    // run_state.json keeps the camelCase key every released build wrote.
    assert_eq!(state_json(&run)["snapshot"]["humanQuestions"][0]["question"], Q1);
    // Both the report written by the engine's own saves and the regenerated export carry the pair.
    let html = fs::read_to_string(run.join("output/final/final_review.html")).unwrap();
    assert!(html.contains(Q1) && html.contains(A1), "final_review.html (save) lacks the question or the answer");
    fs::remove_file(run.join("output/final/final_review.html")).unwrap();
    export_report(&run).unwrap();
    let html = fs::read_to_string(run.join("output/final/final_review.html")).unwrap();
    assert!(html.contains(Q1), "exported report lacks the question text");
    assert!(html.contains(A1), "exported report lacks the answer");
    let (qi, ai) = (html.find(Q1).unwrap(), html.find(A1).unwrap());
    assert!(qi.abs_diff(ai) <= 300, "question and answer are not one record in the report");
}

/// The answerer is not the final decider. `decide_issue` carries no identity, so none may be recorded.
#[test]
fn test_dod1_val_r34_answerer_identity_is_not_the_final_decider() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path(), policy(3));
    let run = cfg.run_dir.clone();
    let s = start_and_pause_after_round1(cfg, qa_handler(prompts_log()));
    assert_eq!(s.state, RunState::Paused);
    stop_idle(&run).unwrap();
    decide(&run, DecisionKind::Rejected, "Decider-Dora-9902", "no").unwrap();
    decide_issue(&run, &question_id(&s, Q1), HumanChoice::Accept, A1).unwrap();
    let s = load(&run).unwrap();
    let q = &s.human_questions[0];
    assert_eq!(q.answer.as_deref(), Some(A1));
    assert_ne!(q.answered_by.as_deref(), Some("Decider-Dora-9902"), "the final decider is recorded as the answerer");
    assert!(q.answered_by.is_none(), "an identity nobody supplied was recorded: {:?}", q.answered_by);
}

// ---------------------------------------------------------------- VAL-R35

const Q2: &str = "Which vendor takes the contract (q-2e9)?";
const A2: &str = "Security-Office-2290";

#[test]
fn test_dod1_val_r35_reused_question_id_is_rejected_or_distinct_and_answers_stay_apart() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let run = answered_run(dir.path(), prompts_log());
    let first_at = load(&run).unwrap().human_questions[0].answered_at.clone();
    // Round 2: a DIFFERENT question under the SAME id.
    let h = handler(|req, _| {
        Ok(outcome(match (req.role, req.round) {
            (Role::Author, 2) => author(vec![question("R01-099", Q2)], 2),
            (Role::Author, n) => author(vec![], n),
            (Role::Reviewer, _) => review(vec![]),
        }))
    });
    let s = resume_and_pause_after_round(&run, h);
    // R53 (decision A): the model's repeated id is ignored, so the second question is never rejected: it is a distinct record.
    assert!(s.last_error.as_ref().is_none_or(|e| e.kind != "malformed"), "a reused model id stopped the run: {:?}", s.last_error);
    let qs = load(&run).unwrap().human_questions;
    assert_eq!(qs.len(), 2, "the second question was merged or lost: {qs:?}");
    assert_ne!(qs[0].issue_id, qs[1].issue_id, "two different questions share the id {}", qs[0].issue_id);
    assert_eq!((qs[0].question.as_str(), qs[0].answer.as_deref()), (Q1, Some(A1)));
    assert_eq!((qs[1].question.as_str(), qs[1].answer.as_deref()), (Q2, None));
    let second_id = question_id(&s, Q2);
    assert_eq!(second_id, qs[1].issue_id);
    decide_issue(&run, &second_id, HumanChoice::Accept, A2).unwrap();
    let qs = load(&run).unwrap().human_questions;
    assert_eq!((qs[0].question.as_str(), qs[0].answer.as_deref()), (Q1, Some(A1)), "answering the second changed the first");
    assert_eq!(qs[0].answered_at, first_at, "answering the second changed the first timestamp");
    assert_eq!((qs[1].question.as_str(), qs[1].answer.as_deref()), (Q2, Some(A2)));
}

/// R53 (decision A): the model's question id and a reviewer finding id may be equal; our code gives the question its own
/// Q-id, so the finding is NOT rejected, the run does not stop, and both stay visible and apart.
// Old name: test_dod1_val_r35_reviewer_finding_with_the_id_of_an_existing_question_is_rejected
#[test]
fn test_dod1_val_r35_reviewer_finding_with_the_model_question_id_is_accepted_with_distinct_ids() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path(), policy(3));
    let run = cfg.run_dir.clone();
    let h = handler(|req, _| {
        Ok(outcome(match req.role {
            Role::Author => author(vec![question("R01-050", Q1)], req.round),
            Role::Reviewer => review(vec![issue("R01-050", "medium")]),
        }))
    });
    // Round 1 is enough: the old rejection happened in this round's review; the pause is ours, not an error.
    let s = start_and_pause_after_round1(cfg, h);
    assert!(s.last_error.is_none(), "a finding equal to the model's question id stopped the run: {:?}", s.last_error);
    assert!(s.issues.iter().any(|i| i.issue.issue_id == "R01-050"), "the finding was dropped");
    assert_eq!(s.human_questions.len(), 1);
    let qid = question_id(&s, Q1);
    assert_ne!(qid, "R01-050");
    assert!(s.issues.iter().all(|i| i.issue.issue_id != qid), "the question id is also a finding id");
    assert!(run.join("output/reviews/review_01.json").exists(), "the review was not written");
    // Answers stay apart: answering the question leaves the finding as it was.
    let before = s.issues.iter().find(|i| i.issue.issue_id == "R01-050").map(|i| i.issue.finding.clone());
    decide_issue(&run, &qid, HumanChoice::Accept, A1).unwrap();
    let s = load(&run).unwrap();
    assert_eq!(s.human_questions[0].answer.as_deref(), Some(A1));
    assert_eq!(s.issues.iter().find(|i| i.issue.issue_id == "R01-050").map(|i| i.issue.finding.clone()), before);
    // Both stay visible in the export.
    export_report(&run).unwrap();
    let html = fs::read_to_string(run.join("output/final/final_review.html")).unwrap();
    assert!(html.contains(Q1) && html.contains("R01-050"), "the export lost the question or the finding");
}

/// Control (R53, decision A): a model question id equal to a RESOLVED finding of an earlier round is ignored: the answer
/// validates, the question gets a Q-id, and the resolved finding's history is untouched.
// Old name: test_dod1_val_r35_control_question_id_equal_to_a_resolved_finding_is_malformed
#[test]
fn test_dod1_val_r35_control_model_question_id_equal_to_a_resolved_finding_gets_own_q_id_and_history_is_untouched() {
    let mut l = IssueLedger::default();
    let r1: Review = serde_json::from_value(review(vec![issue("R05-001", "medium")])).unwrap();
    l.apply_review(1, &r1);
    let mut resolved = issue("R05-001", "medium");
    resolved["status"] = serde_json::json!("resolved");
    let r2: Review = serde_json::from_value(review(vec![resolved])).unwrap();
    l.apply_review(2, &r2);
    let v = author(vec![question("R05-001", "Who decides?")], 3);
    let r = validate_answer(SchemaKind::AuthorResponse, &v, &l, 3);
    assert!(r.is_ok(), "got {r:?}");
    l.apply_author(3, &serde_json::from_value(v).unwrap());
    assert_eq!(l.human_questions().len(), 1);
    let qid = &l.human_questions()[0].issue_id;
    assert!(qid.starts_with('Q') && qid != "R05-001", "question id not assigned by our code: {qid}");
    assert_eq!(l.history("R05-001").expect("finding kept").resolved_on_arrival(), None);
    assert_eq!(l.history("R05-001").unwrap().reviews.len(), 2, "the finding's history changed");
}

// ---------------------------------------------------------------- VAL-R36

/// (verbatim text from the card, its secret payload); the last one is a shape no pattern in redact.rs names.
const CREDENTIALS: [(&str, &str); 6] = [
    (r#"{"token":"shortSecret"}"#, "shortSecret"),
    ("password: hunter2", "hunter2"),
    ("secret=s3cr3tValue", "s3cr3tValue"),
    (r#""client_secret": "cs_live_abc""#, "cs_live_abc"),
    ("X-Api-Key: k_12345", "k_12345"),
    ("mongodb://admin:Tr0ub4dor@db.internal:27017/app", "Tr0ub4dor"),
];

#[test]
fn test_dod1_val_r36_redact_does_not_pass_the_listed_credential_values() {
    let mut leaked = vec![];
    for (text, payload) in CREDENTIALS {
        let line = serde_json::json!({"type":"assistant","message":{"content":[{"type":"text","text":text}]}}).to_string();
        for form in [text.to_owned(), format!("prefix words {text} trailing words"), line] {
            if redact(&form).contains(payload) {
                leaked.push(format!("{payload:?} survives redact({form:?})"));
            }
        }
    }
    assert!(leaked.is_empty(), "{leaked:#?}");
}

const LIVE_CANARY: &str = "zaphod-live-canary-5527 free words nobody listed";

#[test]
fn test_dod1_val_r36_live_line_events_pass_neither_the_credentials_nor_arbitrary_text() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let h = handler(|req, sink| {
        for (text, _) in CREDENTIALS.iter().map(|(t, p)| (*t, *p)).chain([(LIVE_CANARY, "")]) {
            let raw = serde_json::json!({"type":"assistant","message":{"content":[{"type":"text","text":text}]}}).to_string();
            sink(StreamLine { raw, readable: Some(text.to_owned()), is_stderr: false });
            sink(StreamLine { raw: text.to_owned(), readable: None, is_stderr: true });
            sink(StreamLine { raw: text.to_owned(), readable: None, is_stderr: false });
        }
        Ok(outcome(match req.role {
            Role::Author => author(vec![], req.round),
            Role::Reviewer => review(vec![]),
        }))
    });
    let events = Mutex::new(vec![]);
    let s = start(config(dir.path(), policy(1)), clients(h), Arc::default(), |e| events.lock().unwrap().push(e)).unwrap();
    assert_eq!(s.state, RunState::Completed, "{:?}", s.last_error);
    let events = events.into_inner().unwrap();
    // Control: the engine's own progress (the allowlisted text) still arrives verbatim.
    let phases: Vec<String> = events.iter().filter_map(|e| if let RunEvent::Phase { phase, .. } = e { Some(phase.clone()) } else { None }).collect();
    assert_eq!(phases, ["Creating revision 1", "Reviewing revision 1"], "allowlisted progress text lost");
    assert!(events.iter().any(|e| matches!(e, RunEvent::State { state: RunState::Completed, .. })));
    let lines: Vec<String> = events.iter().filter(|e| matches!(e, RunEvent::Line { .. })).map(|e| format!("{e:?}")).collect();
    let mut leaked = vec![];
    for (_, payload) in CREDENTIALS {
        if lines.iter().any(|l| l.contains(payload)) {
            leaked.push(format!("credential value {payload:?} reached a live Line event"));
        }
    }
    if lines.iter().any(|l| l.contains("zaphod-live-canary-5527")) {
        leaked.push("arbitrary unknown text passed verbatim through a live Line event".to_owned());
    }
    assert!(leaked.is_empty(), "{leaked:#?}");
}

// ---------------------------------------------------------------- VAL-R37

fn numbered_docx(path: &Path) {
    use docx_rs::{AbstractNumbering, Docx, IndentLevel, Level, LevelJc, LevelText, NumberFormat, Numbering, NumberingId, Paragraph, Run, Start};
    let level = |n: usize, start: usize, text: &str| Level::new(n, Start::new(start), NumberFormat::new("decimal"), LevelText::new(text), LevelJc::new("left"));
    let item = |text: &str, num: usize, lvl: usize| Paragraph::new().add_run(Run::new().add_text(text)).numbering(NumberingId::new(num), IndentLevel::new(lvl));
    let doc = Docx::new()
        .add_abstract_numbering(AbstractNumbering::new(2).add_level(level(0, 3, "%1.")).add_level(level(1, 1, "%1.%2")))
        .add_abstract_numbering(AbstractNumbering::new(3).add_level(level(0, 1, "%1.")))
        .add_numbering(Numbering::new(2, 2))
        .add_numbering(Numbering::new(3, 3))
        .add_paragraph(item("First", 2, 0))
        .add_paragraph(item("Second", 2, 0))
        .add_paragraph(item("Nested A", 2, 1))
        .add_paragraph(item("Nested B", 2, 1))
        .add_paragraph(Paragraph::new().add_run(Run::new().add_text("See item 3.")))
        .add_paragraph(item("Again", 3, 0))
        .add_paragraph(item("Again two", 3, 0));
    doc.build().pack(fs::File::create(path).unwrap()).unwrap();
}
const IDENTIFIED: [(&str, &str); 6] = [("3.", "First"), ("4.", "Second"), ("4.1", "Nested A"), ("4.2", "Nested B"), ("1.", "Again"), ("2.", "Again two")];
fn has_identified(text: &str, id: &str, body: &str, line_start: bool) -> bool {
    let lead = if line_start { r"(?m)^[ \t]*" } else { r"(?m)(?:^|\s)" };
    let re = regex::Regex::new(&format!(r"{lead}{}[.)]?[ \t]+{}[ \t]*$", regex::escape(id.trim_end_matches('.')), regex::escape(body))).unwrap();
    re.is_match(text)
}

#[test]
fn test_dod1_val_r37_docx_extraction_keeps_numbering_identifiers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("numbered.docx");
    numbered_docx(&path);
    let text = extract(&path).unwrap().text;
    for (id, body) in IDENTIFIED {
        assert!(has_identified(&text, id, body, true), "the visible identifier {id:?} of {body:?} is lost in:\n{text}");
    }
    // A plain paragraph is not numbered, and the list is not renumbered from 1 (the start 3 and the restart differ).
    assert!(text.lines().any(|l| l == "See item 3."), "plain paragraph changed in:\n{text}");
    assert!(!has_identified(&text, "1.", "First", false), "list start 3 lost in:\n{text}");
    assert!(!has_identified(&text, "5.", "Again", false), "the restarted list continued the first in:\n{text}");
    let pos = |needle: &str| text.find(needle).unwrap_or_else(|| panic!("{needle} missing in:\n{text}"));
    assert!(pos("First") < pos("Second") && pos("Second") < pos("Nested A") && pos("Nested B") < pos("See item 3.") && pos("See item 3.") < pos("Again"));
}

#[test]
fn test_dod1_val_r37_docx_export_then_reimport_round_trips_the_identifiers() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("numbered.docx");
    numbered_docx(&src);
    let text = extract(&src).unwrap().text;
    // Export the extracted representation as a revision .docx, then import that file again.
    let primary = dir.path().join("p.md");
    fs::write(&primary, "# P").unwrap();
    let run = dir.path().join("run");
    let pkg = OutputPackage::create(&run, &primary, &[]).unwrap();
    pkg.write_revision(1, &text).unwrap();
    let again = extract(&run.join("output/revisions/revision_01.docx")).unwrap().text;
    for (id, body) in IDENTIFIED {
        assert!(has_identified(&again, id, body, false), "after export and reimport the identifier {id:?} of {body:?} is lost.\nfirst import:\n{text}\nreimport:\n{again}");
    }
    assert!(again.lines().any(|l| l == "See item 3."), "plain paragraph changed after the round trip:\n{again}");
}

/// Markdown list with start 3 and a nested level, exported by the product, then imported: the numbers survive.
#[test]
fn test_dod1_val_r37_markdown_list_exported_to_docx_is_extracted_with_its_numbers() {
    let dir = tempfile::tempdir().unwrap();
    let primary = dir.path().join("p.md");
    fs::write(&primary, "# P").unwrap();
    let run = dir.path().join("run");
    let pkg = OutputPackage::create(&run, &primary, &[]).unwrap();
    pkg.write_revision(1, "3. First\n4. Second\n   1. Inner\n5. Third\n\nSee item 3.").unwrap();
    let text = extract(&run.join("output/revisions/revision_01.docx")).unwrap().text;
    for (id, body) in [("3.", "First"), ("4.", "Second"), ("1.", "Inner"), ("5.", "Third")] {
        assert!(has_identified(&text, id, body, false), "identifier {id:?} of {body:?} lost in:\n{text}");
    }
    assert!(text.lines().any(|l| l == "See item 3."), "plain paragraph changed:\n{text}");
}

// ---------------------------------------------------------------- VAL-R38

fn two_notes(root: &Path) -> (PathBuf, Vec<PathBuf>) {
    let primary = root.join("primary-confidential.md");
    fs::write(&primary, "# Primary\n\nPRIMARY-CONFIDENTIAL-BYTES-7741").unwrap();
    let (a, b) = (root.join("a"), root.join("b"));
    fs::create_dir_all(&a).unwrap();
    fs::create_dir_all(&b).unwrap();
    fs::write(a.join("notes.md"), "SUPPORT-ONE-CONFIDENTIAL-3302").unwrap();
    fs::write(b.join("notes.md"), "SUPPORT-TWO-CONFIDENTIAL-8815").unwrap();
    (primary, vec![a.join("notes.md"), b.join("notes.md")])
}
fn assert_no_trace(runs_root: &Path, run_dir: &Path) {
    let mut left = vec![];
    files_under(runs_root, &mut left);
    assert!(left.is_empty(), "files remain under the runs dir: {left:?}");
    assert!(!run_dir.exists(), "an orphan run folder remains: {}", run_dir.display());
}

#[test]
fn test_dod1_val_r38_duplicate_supporting_names_leave_no_copy_under_the_runs_dir() {
    let dir = tempfile::tempdir().unwrap();
    let (primary, supporting) = two_notes(dir.path());
    let runs = dir.path().join("runs");
    let run_dir = runs.join("run-1");
    let r = OutputPackage::create(&run_dir, &primary, &supporting);
    assert!(r.is_err(), "two supporting files named notes.md must fail");
    assert_no_trace(&runs, &run_dir);
}

#[test]
fn test_dod1_val_r38_names_equal_up_to_case_also_fail_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let (primary, supporting) = two_notes(dir.path());
    let upper = supporting[1].with_file_name("Notes.MD");
    fs::rename(&supporting[1], &upper).unwrap();
    let runs = dir.path().join("runs");
    let run_dir = runs.join("run-1");
    let r = OutputPackage::create(&run_dir, &primary, &[supporting[0].clone(), upper]);
    assert!(r.is_err());
    assert_no_trace(&runs, &run_dir);
}

#[test]
fn test_dod1_val_r38_unreadable_later_supporting_file_leaves_no_copy() {
    let dir = tempfile::tempdir().unwrap();
    let (primary, supporting) = two_notes(dir.path());
    let missing = dir.path().join("a").join("absent.md");
    let runs = dir.path().join("runs");
    let run_dir = runs.join("run-1");
    let r = OutputPackage::create(&run_dir, &primary, &[supporting[0].clone(), missing]);
    assert!(r.is_err());
    assert_no_trace(&runs, &run_dir);
}

#[test]
fn test_dod1_val_r38_engine_start_with_duplicate_names_leaves_no_run_folder() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let (primary, supporting) = two_notes(dir.path());
    let runs = dir.path().join("runs");
    let mut cfg = config(dir.path(), policy(1));
    cfg.primary = primary;
    cfg.supporting = supporting;
    cfg.run_dir = runs.join("run-1");
    let run_dir = cfg.run_dir.clone();
    let h = handler(|_, _| panic!("no provider call may happen"));
    assert!(start(cfg, clients(h), Arc::default(), |_| {}).is_err());
    assert_no_trace(&runs, &run_dir);
}

/// Cleanup must not become a delete of someone else's run: a package may never overwrite a previous run.
#[test]
fn test_dod1_val_r38_control_failed_creation_over_an_existing_run_keeps_that_run() {
    let dir = tempfile::tempdir().unwrap();
    let (primary, supporting) = two_notes(dir.path());
    let run_dir = dir.path().join("runs").join("old-run");
    fs::create_dir_all(run_dir.join("output")).unwrap();
    fs::write(run_dir.join("output/keep.txt"), "previous completed round").unwrap();
    fs::write(run_dir.join("run_state.json"), "{}").unwrap();
    assert!(OutputPackage::create(&run_dir, &primary, &supporting).is_err());
    assert_eq!(fs::read_to_string(run_dir.join("output/keep.txt")).unwrap(), "previous completed round");
    assert_eq!(fs::read_to_string(run_dir.join("run_state.json")).unwrap(), "{}");
}

// ---------------------------------------------------------------- DoD3

#[test]
fn test_dod3_no_skip_or_ignore_in_this_file() {
    let src = include_str!("ct_astra3_core.rs");
    let code: String = src.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
    for banned in [format!("#[{}", "ignore"), format!("x{}", "fail"), format!("should_{}", "panic"), format!("#[cfg({}", "windows")] {
        assert!(!code.contains(banned.as_str()), "found {banned}");
    }
}
