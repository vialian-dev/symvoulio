// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! CARD AST-B (cruel tester): the review's backend failure scenarios R02, R07 race, R12, R19, R31.
//! Expectations come from the card, not from the code under test.
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use symvoulio_core::{
    engine::{load, start, RunConfig, RunControl, RunEvent, RunSnapshot},
    output::OutputPackage,
    process,
    provider::{
        validate_answer, CallError, CallOutcome, CallRequest, CancelToken, ProviderClient,
        SchemaKind, StreamLine,
    },
    IssueLedger, Policy, Provider, Review, Role, RunState,
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

type Script = Arc<
    dyn Fn(Role, u32, &mut dyn FnMut(StreamLine)) -> Result<CallOutcome, CallError> + Send + Sync,
>;
fn mk(
    f: impl Fn(Role, u32, &mut dyn FnMut(StreamLine)) -> Result<CallOutcome, CallError> + Send + Sync + 'static,
) -> Script {
    Arc::new(f)
}
struct Scripted {
    provider: Provider,
    script: Script,
}
impl ProviderClient for Scripted {
    fn provider(&self) -> Provider {
        self.provider
    }
    fn call(
        &self,
        req: &CallRequest,
        sink: &mut dyn FnMut(StreamLine),
        _cancel: &CancelToken,
    ) -> Result<CallOutcome, CallError> {
        (self.script)(req.role, req.round, sink)
    }
}
fn clients(script: Script) -> (Arc<dyn ProviderClient>, Arc<dyn ProviderClient>) {
    (
        Arc::new(Scripted { provider: Provider::Claude, script: script.clone() }),
        Arc::new(Scripted { provider: Provider::Codex, script }),
    )
}
fn outcome(structured: serde_json::Value, stdout: Vec<String>, stderr: &str) -> CallOutcome {
    CallOutcome {
        structured,
        model: None,
        session_id: None,
        usage: None,
        stdout_lines: stdout,
        stderr: stderr.into(),
        duration_s: 0.1,
        exit_code: Some(0),
    }
}
fn issue(id: &str) -> serde_json::Value {
    serde_json::json!({
        "issue_id": id, "severity": "medium", "criterion": "c", "finding": "finding text",
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
fn response(id: &str) -> serde_json::Value {
    serde_json::json!({"issue_id": id, "response": "accepted", "explanation": "done", "evidence": ""})
}
fn author(
    responses: Vec<serde_json::Value>,
    questions: Vec<serde_json::Value>,
    round: u32,
) -> serde_json::Value {
    serde_json::json!({
        "revised_document": format!("# Doc\n\nrevision {round}"),
        "changes_made": ["c"], "responses": responses, "new_claims": [],
        "human_decisions_required": questions
    })
}
fn config(dir: &Path, policy: Policy) -> RunConfig {
    let primary = dir.join("doc.md");
    fs::write(&primary, "# Doc\n\noriginal").unwrap();
    RunConfig {
        run_id: "ct-run".into(),
        name: "ct".into(),
        run_dir: dir.join("run"),
        author: Provider::Claude,
        objective: "Improve".into(),
        criteria: vec!["c".into()],
        policy,
        primary,
        supporting: vec![],
        call_timeout: Duration::from_secs(5),
        author_model: None,
        reviewer_model: None,
    }
}
fn one_round() -> Policy {
    Policy { max_rounds: 1, min_rounds: 1, new_content_limit_pct: 100.0, ..Policy::default() }
}
fn three_rounds() -> Policy {
    Policy { max_rounds: 3, min_rounds: 1, new_content_limit_pct: 100.0, ..Policy::default() }
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

// ---------------------------------------------------------------- VAL-R02

/// (verbatim string from the card, its secret payload)
const SECRETS: [(&str, &str); 5] = [
    ("password=hunter2", "hunter2"),
    ("Cookie: session=abc123", "abc123"),
    (r#"{"apiKey":"shortSecret"}"#, "shortSecret"),
    ("Authorization: Basic dXNlcjpwYXNz", "dXNlcjpwYXNz"),
    ("token=ghp_short1", "ghp_short1"),
];
fn json_escaped(s: &str) -> String {
    let q = serde_json::to_string(s).unwrap();
    q[1..q.len() - 1].to_owned()
}
/// Every place a secret may rest: all files of the run folder (audit, run_state.json, ...), the loaded snapshot,
/// last_error and every RunEvent.
fn leaks(run_dir: &Path, events: &[String]) -> Vec<String> {
    let mut hay: Vec<(String, String)> = vec![];
    let mut files = vec![];
    files_under(run_dir, &mut files);
    for f in files {
        let mut bytes = vec![];
        fs::File::open(&f).unwrap().read_to_end(&mut bytes).unwrap();
        hay.push((f.display().to_string(), String::from_utf8_lossy(&bytes).into_owned()));
    }
    if let Ok(snapshot) = load(run_dir) {
        hay.push(("snapshot".into(), serde_json::to_string(&snapshot).unwrap()));
        hay.push(("last_error".into(), format!("{:?}", snapshot.last_error)));
    }
    for e in events {
        hay.push(("RunEvent".into(), e.clone()));
    }
    let mut found = vec![];
    for (full, payload) in SECRETS {
        for (place, text) in &hay {
            if text.contains(full) || text.contains(&json_escaped(full)) || text.contains(payload) {
                found.push(format!("{full:?} in {place}"));
            }
        }
    }
    found
}
fn secret_lines() -> Vec<String> {
    SECRETS.iter().map(|(s, _)| (*s).to_owned()).collect()
}
fn run_collect(cfg: RunConfig, script: Script) -> (RunSnapshot, Vec<String>, PathBuf) {
    let run = cfg.run_dir.clone();
    let events = Mutex::new(vec![]);
    let s = start(cfg, clients(script), Arc::default(), |e| {
        events.lock().unwrap().push(format!("{e:?}"))
    })
    .unwrap();
    (s, events.into_inner().unwrap(), run)
}
fn emit(sink: &mut dyn FnMut(StreamLine), lines: &[String], is_stderr: bool) {
    for l in lines {
        sink(StreamLine { raw: l.clone(), readable: None, is_stderr });
    }
}

#[test]
fn test_dod1_val_r02_control_benign_run_reports_no_needles() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let script = mk(|role, round, sink| {
        emit(sink, &["plain line".into()], false);
        emit(sink, &["plain err".into()], true);
        Ok(match role {
            Role::Author => outcome(author(vec![], vec![], round), vec!["plain line".into()], "plain err"),
            Role::Reviewer => outcome(review(vec![]), vec!["plain line".into()], "plain err"),
        })
    });
    let (s, events, run) = run_collect(config(dir.path(), one_round()), script);
    assert_eq!(s.state, RunState::Completed, "{:?}", s.last_error);
    assert_eq!(leaks(&run, &events), Vec::<String>::new());
}

#[test]
fn test_dod1_val_r02_stderr_secrets_not_persisted() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let script = mk(|_, _, sink| {
        emit(sink, &secret_lines(), true);
        Err(CallError::ProcessFailed { exit_code: Some(1), stderr_tail: secret_lines().join("\n") })
    });
    let (s, events, run) = run_collect(config(dir.path(), one_round()), script);
    assert_eq!(s.state, RunState::Paused);
    assert_eq!(s.last_error.as_ref().unwrap().kind, "process");
    assert_eq!(leaks(&run, &events), Vec::<String>::new());
}

#[test]
fn test_dod1_val_r02_stdout_secrets_not_persisted() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let script = mk(|_, _, sink| {
        emit(sink, &secret_lines(), false);
        Err(CallError::ProcessFailed { exit_code: Some(1), stderr_tail: String::new() })
    });
    let (s, events, run) = run_collect(config(dir.path(), one_round()), script);
    assert_eq!(s.state, RunState::Paused);
    assert_eq!(leaks(&run, &events), Vec::<String>::new());
}

#[test]
fn test_dod1_val_r02_error_messages_not_persisted() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let body = secret_lines().join(" ");
    type Make = fn(String) -> CallError;
    let makers: [(&str, Make); 4] = [
        ("auth", |b| CallError::AuthFailed(b)),
        ("malformed", |b| CallError::Malformed(b)),
        ("io", |b| CallError::Io(b)),
        ("process", |b| CallError::ProcessFailed { exit_code: Some(2), stderr_tail: b }),
    ];
    for (kind, make) in makers {
        let dir = tempfile::tempdir().unwrap();
        let b = body.clone();
        let script = mk(move |_, _, _| Err(make(b.clone())));
        let (s, events, run) = run_collect(config(dir.path(), one_round()), script);
        assert_eq!(s.last_error.as_ref().unwrap().kind, kind);
        assert_eq!(leaks(&run, &events), Vec::<String>::new(), "error kind {kind}");
    }
}

#[test]
fn test_dod1_val_r02_success_path_outputs_not_persisted() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let script = mk(|role, round, sink| {
        emit(sink, &secret_lines(), false);
        emit(sink, &secret_lines(), true);
        let stderr = secret_lines().join("\n");
        Ok(match role {
            Role::Author => outcome(author(vec![], vec![], round), secret_lines(), &stderr),
            Role::Reviewer => outcome(review(vec![]), secret_lines(), &stderr),
        })
    });
    let (s, events, run) = run_collect(config(dir.path(), one_round()), script);
    assert_eq!(s.state, RunState::Completed, "{:?}", s.last_error);
    assert_eq!(leaks(&run, &events), Vec::<String>::new());
}

/// SUB1: an allowlist, not a regex per test string. A diagnostic body with no credential shape at all must not be
/// persisted either (audit files, run_state.json, last_error, timeline).
#[test]
fn test_dod1_val_r02_sub1_arbitrary_body_is_not_persisted() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    const CANARY: &str = "zaphod-canary-body-8841";
    let dir = tempfile::tempdir().unwrap();
    let script = mk(|_, _, sink| {
        emit(sink, &[format!("stderr says {CANARY}")], true);
        emit(sink, &[format!("stdout says {CANARY}")], false);
        Err(CallError::ProcessFailed { exit_code: Some(1), stderr_tail: format!("tail {CANARY}") })
    });
    let (s, _events, run) = run_collect(config(dir.path(), one_round()), script);
    assert_eq!(s.state, RunState::Paused);
    let mut files = vec![];
    files_under(&run, &mut files);
    let mut hits = vec![];
    for f in files {
        if String::from_utf8_lossy(&fs::read(&f).unwrap()).contains(CANARY) {
            hits.push(f.display().to_string());
        }
    }
    let snap = serde_json::to_string(&load(&run).unwrap()).unwrap();
    if snap.contains(CANARY) {
        hits.push("snapshot".into());
    }
    assert!(hits.is_empty(), "arbitrary provider body persisted in: {hits:?}");
}

// ---------------------------------------------------------------- VAL-R07race

/// Stop arrives AFTER the worker emitted State::Paused and BEFORE it exits. The stop is injected from another thread
/// while the event callback is still running, so it strictly falls inside that window; delays are randomized.
#[test]
fn test_dod1_val_r07race_stop_after_paused_before_exit_ends_stopped() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let mut seed: u64 = 0x2545F4914F6CDD1D;
    let mut rnd = move |max_ms: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % (max_ms + 1)
    };
    let mut failures = vec![];
    for i in 0..60 {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config(dir.path(), three_rounds());
        let run = cfg.run_dir.clone();
        let control = Arc::new(RunControl::default());
        // Pause is requested before the first provider call: the worker pauses and emits State::Paused.
        control.pause.store(true, Ordering::SeqCst);
        let (arrive_tx, arrive_rx) = mpsc::channel::<()>();
        let (ack_tx, ack_rx) = mpsc::channel::<()>();
        let (pre_delay, post_delay) = (rnd(12), rnd(12));
        let stopper = {
            let control = control.clone();
            thread::spawn(move || {
                arrive_rx.recv_timeout(Duration::from_secs(10)).unwrap();
                thread::sleep(Duration::from_millis(pre_delay));
                control.stop();
                let _ = ack_tx.send(());
            })
        };
        let (arrive_tx, ack_rx) = (Mutex::new(arrive_tx), Mutex::new(ack_rx));
        let script = mk(|_, _, _| panic!("no provider call is expected while paused"));
        let snapshot = start(cfg, clients(script), control.clone(), |e| {
            if let RunEvent::State { state: RunState::Paused, .. } = e {
                arrive_tx.lock().unwrap().send(()).unwrap();
                ack_rx.lock().unwrap().recv_timeout(Duration::from_secs(10)).unwrap();
                thread::sleep(Duration::from_millis(post_delay));
            }
        })
        .unwrap();
        stopper.join().unwrap();
        let persisted = load(&run).unwrap().state;
        if snapshot.state != RunState::Stopped || persisted != RunState::Stopped {
            failures.push(format!(
                "iteration {i} (stop +{pre_delay} ms, exit +{post_delay} ms): returned {:?}, persisted {:?}",
                snapshot.state, persisted
            ));
        }
    }
    assert!(failures.is_empty(), "{} of 60 runs ended Paused instead of Stopped:\n{}", failures.len(), failures.join("\n"));
}

// ---------------------------------------------------------------- VAL-R12

fn ledger_with(ids: &[&str]) -> IssueLedger {
    let mut l = IssueLedger::default();
    let r: Review = serde_json::from_value(review(ids.iter().map(|i| issue(i)).collect())).unwrap();
    l.apply_review(1, &r);
    l
}
fn malformed(r: Result<(), CallError>) -> bool {
    matches!(r, Err(CallError::Malformed(_)))
}

#[test]
fn test_dod1_val_r12_duplicate_response_for_one_issue_is_malformed() {
    let l = ledger_with(&["R05-001", "R05-002"]);
    let v = author(vec![response("R05-001"), response("R05-001"), response("R05-002")], vec![], 2);
    let r = validate_answer(SchemaKind::AuthorResponse, &v, &l, 2);
    assert!(malformed(r), "two entries for R05-001 must be Malformed");
}

#[test]
fn test_dod1_val_r12_response_for_unknown_id_is_malformed() {
    let l = ledger_with(&["R05-001", "R05-002"]);
    let v = author(vec![response("R05-001"), response("R05-002"), response("R05-999")], vec![], 2);
    let r = validate_answer(SchemaKind::AuthorResponse, &v, &l, 2);
    assert!(malformed(r), "a response for R05-999 must be Malformed");
}

#[test]
fn test_dod1_val_r12_complete_unique_answer_is_accepted_control() {
    let l = ledger_with(&["R05-001", "R05-002"]);
    let v = author(vec![response("R05-001"), response("R05-002")], vec![], 2);
    validate_answer(SchemaKind::AuthorResponse, &v, &l, 2).expect("control answer must pass");
}

/// Rule tested (R53, decision A): the question id is assigned by our code, so a model id equal to an issue id cannot
/// collide: the answer validates, the question gets its own Q-id, and the issue history is untouched.
// Old name: test_dod1_val_r12_question_id_colliding_with_issue_id_is_malformed
#[test]
fn test_dod1_val_r12_question_id_colliding_with_issue_id_is_accepted_with_distinct_ids() {
    let mut l = ledger_with(&["R05-001", "R05-002"]);
    let q = serde_json::json!({"issue_id": "R05-001", "question": "Who decides?"});
    let v = author(vec![response("R05-001"), response("R05-002")], vec![q], 2);
    validate_answer(SchemaKind::AuthorResponse, &v, &l, 2).expect("a model id equal to an issue id is ignored, not rejected");
    l.apply_author(2, &serde_json::from_value(v).unwrap());
    let qs = l.human_questions();
    assert_eq!(qs.len(), 1);
    assert_eq!(qs[0].question, "Who decides?");
    assert!(qs[0].issue_id.starts_with('Q'), "question id not assigned by our code: {}", qs[0].issue_id);
    assert!(l.history(&qs[0].issue_id).is_none(), "the question id is also an issue id");
    assert_eq!(l.history("R05-001").expect("issue kept").responses.len(), 1, "the response stayed with the issue");
    assert!(l.history("R05-002").is_some());
}

#[test]
fn test_dod1_val_r12_question_with_new_id_is_allowed_control() {
    let l = ledger_with(&["R05-001", "R05-002"]);
    let q = serde_json::json!({"issue_id": "R05-777", "question": "Who owns the budget?"});
    let v = author(vec![response("R05-001"), response("R05-002")], vec![q], 2);
    validate_answer(SchemaKind::AuthorResponse, &v, &l, 2).expect("a question with a new id is allowed");
}

fn assert_nothing_written(run: &Path, round: u32) {
    let out = run.join("output");
    for f in [
        format!("revisions/revision_{round:02}.md"),
        format!("revisions/revision_{round:02}.docx"),
        format!("reviews/author_response_{round:02}.json"),
    ] {
        assert!(!out.join(&f).exists(), "rejected answer left output/{f}");
    }
    let mut all = vec![];
    files_under(&out, &mut all);
    let marker = format!("_{round:02}");
    for f in all {
        let rel = f.strip_prefix(&out).unwrap().to_string_lossy().replace('\\', "/");
        let is_audit = rel.starts_with("audit/");
        assert!(is_audit || !rel.contains(&marker), "rejected answer left output/{rel}");
    }
}
fn two_round_script(round2_answer: serde_json::Value) -> Script {
    mk(move |role, round, _| {
        Ok(match (role, round) {
            (Role::Reviewer, 1) => outcome(review(vec![issue("R05-001"), issue("R05-002")]), vec![], ""),
            (Role::Reviewer, _) => outcome(review(vec![]), vec![], ""),
            (Role::Author, 1) => outcome(author(vec![], vec![], 1), vec![], ""),
            (Role::Author, _) => outcome(round2_answer.clone(), vec![], ""),
        })
    })
}

#[test]
fn test_dod1_val_r12_rejected_duplicate_writes_nothing_to_output() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let bad = author(vec![response("R05-001"), response("R05-001"), response("R05-002")], vec![], 2);
    let (s, _, run) = run_collect(config(dir.path(), three_rounds()), two_round_script(bad));
    assert_eq!(s.state, RunState::Paused);
    assert_eq!(s.last_error.as_ref().unwrap().kind, "malformed");
    assert_nothing_written(&run, 2);
}

#[test]
fn test_dod1_val_r12_rejected_unknown_id_writes_nothing_to_output() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let bad = author(vec![response("R05-001"), response("R05-002"), response("R05-999")], vec![], 2);
    let (s, _, run) = run_collect(config(dir.path(), three_rounds()), two_round_script(bad));
    assert_eq!(s.state, RunState::Paused);
    assert_eq!(s.last_error.as_ref().unwrap().kind, "malformed");
    assert_nothing_written(&run, 2);
}

/// R53 (decision A): "collision rejected, nothing written" can no longer arise. The same answer is accepted: the run
/// does not stop as malformed, round 2 is written, and the question is recorded under a Q-id distinct from the issue ids.
// Old name: test_dod1_val_r12_rejected_question_collision_writes_nothing_and_records_no_question
#[test]
fn test_dod1_val_r12_question_collision_is_written_and_question_gets_own_q_id() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _e = Env::new();
    let dir = tempfile::tempdir().unwrap();
    let q = serde_json::json!({"issue_id": "R05-002", "question": "Who decides?"});
    let ok = author(vec![response("R05-001"), response("R05-002")], vec![q], 2);
    let (s, _, run) = run_collect(config(dir.path(), three_rounds()), two_round_script(ok));
    assert!(s.last_error.as_ref().is_none_or(|e| e.kind != "malformed"), "a colliding model id stopped the run: {:?}", s.last_error);
    assert_eq!(s.human_questions.len(), 1, "the question was lost: {:?}", s.human_questions);
    let qid = &s.human_questions[0].issue_id;
    assert!(qid.starts_with('Q'), "question id not assigned by our code: {qid}");
    assert!(s.issues.iter().all(|i| &i.issue.issue_id != qid), "the question id is also a finding id");
    assert!(s.issues.iter().any(|i| i.issue.issue_id == "R05-002"), "the colliding finding disappeared");
    assert!(run.join("output/reviews/author_response_02.json").exists(), "round 2 was not written");
}

// ---------------------------------------------------------------- VAL-R19

const ROLE: &str = "CT_ASTRA_ROLE";
const PIDFILE: &str = "CT_ASTRA_PIDFILE";

/// Helper "process roles": the test binary re-executes itself. Without the env var these two are no-ops.
#[test]
fn r19_helper_root() {
    if std::env::var(ROLE).as_deref() != Ok("root") {
        return;
    }
    let mut child = Command::new(std::env::current_exe().unwrap());
    child
        .args(["--exact", "r19_helper_holder", "--nocapture", "--test-threads=1"])
        .env(ROLE, "holder")
        .stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        child.creation_flags(0x08000000);
    }
    // Never waited for: the descendant keeps the inherited pipes after this root exits.
    #[allow(clippy::zombie_processes)]
    let _ = child.spawn().expect("spawn descendant");
}
#[test]
fn r19_helper_holder() {
    if std::env::var(ROLE).as_deref() != Ok("holder") {
        return;
    }
    let file = std::env::var(PIDFILE).expect("pid file");
    fs::write(&file, std::process::id().to_string()).unwrap();
    thread::sleep(Duration::from_secs(90));
}

fn alive(pid: u32) -> bool {
    #[cfg(windows)]
    {
        let mut cmd = Command::new("tasklist");
        cmd.args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"]);
        let out = cmd.output().expect("tasklist");
        String::from_utf8_lossy(&out.stdout).contains(&format!("\"{pid}\""))
    }
    #[cfg(unix)]
    {
        if unsafe { libc::kill(pid as libc::pid_t, 0) } != 0 {
            return false;
        }
        // A killed process that is not reaped yet is a zombie: gone for our purpose.
        match fs::read_to_string(format!("/proc/{pid}/stat")) {
            Ok(stat) => !stat.rsplit(')').next().unwrap_or("").trim_start().starts_with('Z'),
            Err(_) => true,
        }
    }
}
fn force_kill(pid: u32) {
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill").args(["/F", "/T", "/PID", &pid.to_string()]).output();
    }
    #[cfg(unix)]
    {
        unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
    }
}
fn root_command(pidfile: &Path) -> Command {
    let mut cmd = Command::new(std::env::current_exe().unwrap());
    cmd.args(["--exact", "r19_helper_root", "--nocapture", "--test-threads=1"])
        .env(ROLE, "root")
        .env(PIDFILE, pidfile);
    cmd
}
fn wait_pid(pidfile: &Path) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Ok(text) = fs::read_to_string(pidfile) {
            if let Ok(pid) = text.trim().parse() {
                return pid;
            }
        }
        assert!(Instant::now() < deadline, "descendant never reported its pid");
        thread::sleep(Duration::from_millis(20));
    }
}
fn wait_dead(pid: u32, within: Duration) -> bool {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        if !alive(pid) {
            return true;
        }
        thread::sleep(Duration::from_millis(100));
    }
    !alive(pid)
}

/// Control: the helper chain works and `alive` sees a live descendant, so the R19 checks below cannot pass vacuously.
#[test]
fn test_dod1_val_r19_control_descendant_is_detectable() {
    let dir = tempfile::tempdir().unwrap();
    let pidfile = dir.path().join("pid");
    let status = root_command(&pidfile)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    let pid = wait_pid(&pidfile);
    let seen = alive(pid);
    force_kill(pid);
    assert!(seen, "the descendant must be alive after its root exited");
    assert!(wait_dead(pid, Duration::from_secs(5)), "force_kill must remove it");
}

fn run_descendant_case(cancel_it: bool) {
    let dir = tempfile::tempdir().unwrap();
    let pidfile = dir.path().join("pid");
    let cancel: CancelToken = Arc::new(AtomicBool::new(false));
    let timeout = if cancel_it { Duration::from_secs(60) } else { Duration::from_secs(3) };
    let stopper = cancel_it.then(|| {
        let (cancel, pidfile) = (cancel.clone(), pidfile.clone());
        thread::spawn(move || {
            wait_pid(&pidfile);
            thread::sleep(Duration::from_millis(500));
            cancel.store(true, Ordering::SeqCst);
            Instant::now()
        })
    });
    let started = Instant::now();
    let result = process::run(root_command(&pidfile), String::new(), timeout, &cancel, &mut |_| {});
    let returned = Instant::now();
    let pid = wait_pid(&pidfile);
    let _reaper = Reaper(pid);
    assert_eq!(matches!(result, Err(CallError::Cancelled)), cancel_it, "unexpected result for cancel={cancel_it}");
    assert_eq!(matches!(result, Err(CallError::Timeout)), !cancel_it, "unexpected result for cancel={cancel_it}");
    // Bound (declared): the call returns within 4 s of the deadline or of the cancel (kill plus the 1.5 s pipe drain).
    let late = if cancel_it {
        returned.saturating_duration_since(stopper.unwrap().join().unwrap())
    } else {
        (returned - started).saturating_sub(timeout)
    };
    assert!(late < Duration::from_secs(4), "call returned {late:?} after the deadline");
    // SUB2: latency alone proves nothing; the descendant must be gone.
    assert!(
        wait_dead(pid, Duration::from_secs(5)),
        "descendant pid {pid} still exists 5 s after the call returned"
    );
}
struct Reaper(u32);
impl Drop for Reaper {
    fn drop(&mut self) {
        force_kill(self.0);
    }
}

#[test]
fn test_dod1_val_r19_cancel_kills_descendant_that_outlives_the_root() {
    run_descendant_case(true);
}
#[test]
fn test_dod1_val_r19_timeout_kills_descendant_that_outlives_the_root() {
    run_descendant_case(false);
}

// Platform names the card asks for (`cargo test ... r19_windows`) and their Linux counterpart (process group).
#[cfg(windows)]
#[test]
fn r19_windows_job_object_cancel_and_timeout_leave_no_descendant() {
    run_descendant_case(true);
    run_descendant_case(false);
}
#[cfg(unix)]
#[test]
fn r19_unix_process_group_cancel_and_timeout_leave_no_descendant() {
    run_descendant_case(true);
    run_descendant_case(false);
}

// ---------------------------------------------------------------- VAL-R31

struct Docx {
    document: String,
    numbering: String,
}
fn docx_of(markdown: &str) -> Docx {
    let dir = tempfile::tempdir().unwrap();
    let primary = dir.path().join("p.md");
    fs::write(&primary, "# P").unwrap();
    let run = dir.path().join("run");
    let pkg = OutputPackage::create(&run, &primary, &[]).unwrap();
    pkg.write_revision(1, markdown).unwrap();
    let bytes = fs::read(run.join("output/revisions/revision_01.docx")).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut read = |name: &str| {
        let mut s = String::new();
        if let Ok(mut f) = zip.by_name(name) {
            f.read_to_string(&mut s).unwrap();
        }
        s
    };
    Docx { document: read("word/document.xml"), numbering: read("word/numbering.xml") }
}
struct ListPara {
    text: String,
    num_id: Option<String>,
    level: Option<u32>,
}
fn paragraphs(xml: &str) -> Vec<ListPara> {
    let p = regex::Regex::new(r"(?s)<w:p[ >].*?</w:p>").unwrap();
    let t = regex::Regex::new(r"(?s)<w:t(?: [^>]*)?>(.*?)</w:t>").unwrap();
    let id = regex::Regex::new(r#"<w:numId w:val="(\d+)""#).unwrap();
    let lvl = regex::Regex::new(r#"<w:ilvl w:val="(\d+)""#).unwrap();
    p.find_iter(xml)
        .map(|m| {
            let s = m.as_str();
            ListPara {
                text: t.captures_iter(s).map(|c| c[1].to_owned()).collect(),
                num_id: id.captures(s).map(|c| c[1].to_owned()),
                level: lvl.captures(s).map(|c| c[1].parse().unwrap()),
            }
        })
        .collect()
}
/// (numFmt, effective start) of a numbering instance at a level: lvlOverride/startOverride wins over the definition.
fn level_def(numbering: &str, num_id: &str, level: u32) -> Option<(String, u32)> {
    let num = regex::Regex::new(&format!(r#"(?s)<w:num w:numId="{num_id}"[^>]*>(.*?)</w:num>"#)).unwrap();
    let body = num.captures(numbering)?.get(1)?.as_str().to_owned();
    let abs_id = regex::Regex::new(r#"<w:abstractNumId w:val="(\d+)""#).unwrap().captures(&body)?[1].to_owned();
    let abs = regex::Regex::new(&format!(r#"(?s)<w:abstractNum w:abstractNumId="{abs_id}"[^>]*>(.*?)</w:abstractNum>"#)).unwrap();
    let abs_body = abs.captures(numbering)?.get(1)?.as_str().to_owned();
    let lv = regex::Regex::new(&format!(r#"(?s)<w:lvl w:ilvl="{level}"[^>]*>(.*?)</w:lvl>"#)).unwrap();
    let lv_body = lv.captures(&abs_body)?.get(1)?.as_str().to_owned();
    let fmt = regex::Regex::new(r#"<w:numFmt w:val="([^"]+)""#).unwrap().captures(&lv_body)?[1].to_owned();
    let mut start: u32 = regex::Regex::new(r#"<w:start w:val="(\d+)""#)
        .unwrap()
        .captures(&lv_body)
        .map(|c| c[1].parse().unwrap())
        .unwrap_or(1);
    let ov = regex::Regex::new(&format!(
        r#"(?s)<w:lvlOverride w:ilvl="{level}"[^>]*>.*?<w:startOverride w:val="(\d+)""#
    ))
    .unwrap();
    if let Some(c) = ov.captures(&body) {
        start = c[1].parse().unwrap();
    }
    Some((fmt, start))
}
fn item<'a>(ps: &'a [ListPara], text: &str) -> &'a ListPara {
    ps.iter().find(|p| p.text == text).unwrap_or_else(|| panic!("no paragraph {text:?}"))
}

#[test]
fn test_dod1_val_r31_ordered_list_keeps_visible_decimal_numbering() {
    let d = docx_of("1. First\n2. Second\n\n3. Third");
    assert!(d.document.contains("<w:numPr>"), "document.xml has no <w:numPr>");
    let ps = paragraphs(&d.document);
    let items: Vec<&ListPara> = ["First", "Second", "Third"].iter().map(|t| item(&ps, t)).collect();
    let ids: std::collections::BTreeSet<_> = items.iter().map(|p| p.num_id.clone()).collect();
    assert!(items.iter().all(|p| p.num_id.is_some()), "an ordered item lost its numPr");
    assert_eq!(ids.len(), 1, "the three items must continue ONE list (visible 1, 2, 3): {ids:?}");
    let (fmt, start) = level_def(&d.numbering, items[0].num_id.as_ref().unwrap(), 0)
        .expect("numbering.xml has no definition for the list");
    assert_eq!(fmt, "decimal");
    assert_eq!(start, 1);
}

#[test]
fn test_dod1_val_r31_ordered_list_start_value_is_preserved() {
    let d = docx_of("3. Third\n4. Fourth");
    let ps = paragraphs(&d.document);
    let first = item(&ps, "Third");
    let id = first.num_id.as_ref().expect("ordered item has no numPr");
    let (fmt, start) = level_def(&d.numbering, id, 0).expect("no numbering definition");
    assert_eq!(fmt, "decimal");
    assert_eq!(start, 3, "list start value 3 lost");
    assert_eq!(item(&ps, "Fourth").num_id.as_ref(), Some(id));
}

#[test]
fn test_dod1_val_r31_nested_ordered_list_keeps_its_level() {
    let d = docx_of("1. A\n   1. Aa\n   2. Ab\n2. B");
    let ps = paragraphs(&d.document);
    for (text, level) in [("A", 0), ("Aa", 1), ("Ab", 1), ("B", 0)] {
        let p = item(&ps, text);
        assert_eq!(p.level, Some(level), "level of {text}");
        let id = p.num_id.as_ref().unwrap_or_else(|| panic!("{text} has no numPr"));
        let (fmt, start) = level_def(&d.numbering, id, level).unwrap_or_else(|| panic!("{text}: no level definition"));
        assert_eq!((fmt.as_str(), start), ("decimal", 1), "{text}");
    }
}

#[test]
fn test_dod1_val_r31_bullet_items_keep_their_numpr_control() {
    let d = docx_of("- x\n- y");
    let ps = paragraphs(&d.document);
    assert!(item(&ps, "x").num_id.is_some() && item(&ps, "y").num_id.is_some(), "bullet lost its numPr");
}

/// A numbering instance must resolve to exactly one definition: OOXML ids are unique. With a duplicate `abstractNumId`
/// or `numId` the «decimal definition» of a list is ambiguous and a reader may pick the bullet one.
#[test]
fn test_dod1_val_r31_numbering_xml_ids_are_unique() {
    for md in ["1. a\n2. b", "- a\n- b", "1. a\n   - b\n2. c"] {
        let n = docx_of(md).numbering;
        for (what, re) in [
            ("abstractNumId", r#"<w:abstractNum w:abstractNumId="(\d+)""#),
            ("numId", r#"<w:num w:numId="(\d+)""#),
        ] {
            let ids: Vec<String> =
                regex::Regex::new(re).unwrap().captures_iter(&n).map(|c| c[1].to_owned()).collect();
            let unique: std::collections::BTreeSet<_> = ids.iter().collect();
            assert_eq!(ids.len(), unique.len(), "duplicate {what} {ids:?} for {md:?}");
        }
    }
}

// ---------------------------------------------------------------- DoD3

/// DoD3: this file carries no skip/ignore; platform gating is by cfg only and every cfg(windows) has a cfg(unix) twin.
#[test]
fn test_dod3_no_skip_ignore_and_platform_twins() {
    let src = include_str!("ct_astra_core.rs");
    let code: String = src
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("//")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let banned = [format!("#[{}", "ignore"), format!("x{}", "fail"), format!("should_{}", "panic")];
    for b in &banned {
        assert!(!code.contains(b.as_str()), "found {b}");
    }
    assert!(code.matches("#[cfg(windows)]").count() >= 1 && code.matches("#[cfg(unix)]").count() >= 1);
}
