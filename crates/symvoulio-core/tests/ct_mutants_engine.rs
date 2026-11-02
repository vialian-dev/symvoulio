// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! Mutation survivors in engine.rs (`event()` previous-started lookup, `save()` returned-run guard). Expectations come
//! from the intent of the code and the timeline semantics, not from its text:
//! * a «started» entry (AuthorStarted / ReviewStarted) of a role and provider is closed by the next event of that same
//!   role and provider: that event carries the round and attempt of THAT started entry, not of another role's start and
//!   not of whatever same-role event happened to come last (a legacy entry may lack both);
//! * `save()` lets a run continue after PublicationIncomplete only when the run is Running AND its committed decision
//!   is Returned. In every other state the incomplete publication is an error the caller sees.
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use symvoulio_core::{
    engine::{decide, load, resume, start, stop_idle, PublicationIncomplete, RunConfig, RunControl, RunEvent},
    provider::{CallError, CallOutcome, CallRequest, CancelToken, ProviderClient, StreamLine},
    DecisionKind, Policy, Provider, Role, RunState,
};

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
fn author_doc(round: u32) -> serde_json::Value {
    serde_json::json!({
        "revised_document": format!("# Doc\n\nrevision {round}"),
        "changes_made": ["c"], "responses": [], "new_claims": [], "human_decisions_required": []
    })
}
fn clean_review() -> serde_json::Value {
    serde_json::json!({"summary": "s", "issues": [], "criteria_results": [{"criterion": "c", "met": true, "note": "n"}]})
}
fn config(root: &Path, max_rounds: u32) -> RunConfig {
    let primary = root.join("doc.md");
    fs::write(&primary, "# Doc\n\noriginal").unwrap();
    RunConfig {
        run_id: "ctm-run".into(),
        name: "ctm".into(),
        run_dir: root.join("run"),
        author: Provider::Claude,
        objective: "Improve".into(),
        criteria: vec!["c".into()],
        policy: Policy { max_rounds, min_rounds: 1, new_content_limit_pct: 100.0, ..Policy::default() },
        primary,
        supporting: vec![],
        call_timeout: Duration::from_secs(5),
        author_model: None,
        reviewer_model: None,
    }
}
fn state_json(run: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(run.join("run_state.json")).unwrap()).unwrap()
}
fn write_state(run: &Path, v: &serde_json::Value) {
    fs::write(run.join("run_state.json"), serde_json::to_vec_pretty(v).unwrap()).unwrap();
}

// ---------------------------------------------------------------- event(): the started entry a later event closes

/// Timeline after a failed reviewer attempt 1 and a storage failure during reviewer attempt 2 of round 1:
/// author: AuthorStarted(r1,a1), AuthorCompleted; reviewer: ReviewStarted(r1,a1), ProviderError, ReviewStarted(r1,a2);
/// the recovery event is an Author-role ProviderError (round None). It closes the AUTHOR's started entry: attempt 1, round 1.
/// Not the reviewer's latest start (attempt 2), not nothing (role/provider of the lookup flipped), and not the author's
/// legacy AuthorCompleted entry, whose attempt and round the crafted state leaves empty.
#[test]
fn test_mutants_event_closes_the_previous_started_entry_of_the_same_role_and_provider() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path(), 1);
    let run = cfg.run_dir.clone();
    let reviewer_calls = Arc::new(AtomicUsize::new(0));
    let h: Handler = {
        let reviewer_calls = reviewer_calls.clone();
        Arc::new(move |req, _| match req.role {
            Role::Author => Ok(outcome(author_doc(req.round))),
            Role::Reviewer => {
                if reviewer_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    Err(CallError::Malformed("first reviewer attempt fails".into()))
                } else {
                    Ok(outcome(clean_review()))
                }
            }
        })
    };
    let s = start(cfg, clients(h.clone()), Arc::default(), |_| {}).unwrap();
    assert_eq!(s.state, RunState::Paused, "setup: the first reviewer attempt must pause the run: {:?}", s.last_error);

    // A legacy state: the author's completion entry carries neither attempt nor round.
    let mut st = state_json(&run);
    let mut crafted = 0;
    for e in st["timeline"].as_array_mut().unwrap() {
        if e["kind"] == "author_completed" {
            e["attempt"] = serde_json::Value::Null;
            e["round"] = serde_json::Value::Null;
            crafted += 1;
        }
    }
    assert_eq!(crafted, 1, "harness: exactly one author_completed entry to craft");
    write_state(&run, &st);

    // Reviewer attempt 2 starts; its result cannot be stored (storage failure), which the engine records as an
    // Author-role ProviderError without a round.
    let obstruction: PathBuf = run.join("output/reviews/review_01.json.tmp");
    let injected = Arc::new(AtomicUsize::new(0));
    let s = resume(&run, clients(h), Arc::default(), {
        let (obstruction, injected) = (obstruction.clone(), injected.clone());
        move |e| {
            if let RunEvent::Phase { role: Role::Reviewer, round: 1, .. } = e {
                fs::create_dir(&obstruction).unwrap();
                injected.fetch_add(1, Ordering::SeqCst);
            }
        }
    })
    .unwrap();
    assert!(injected.load(Ordering::SeqCst) > 0, "L55: the storage fault was never injected");
    assert_eq!(s.state, RunState::Paused, "{:?}", s.last_error);
    assert_eq!(s.last_error.as_ref().map(|e| e.kind.as_str()), Some("storage"), "the injected fault must surface as a storage failure");

    let timeline = state_json(&run)["timeline"].as_array().unwrap().clone();
    let describe = |e: &serde_json::Value| format!("{} role={} provider={} round={} attempt={}", e["kind"], e["role"], e["provider"], e["round"], e["attempt"]);
    let all: Vec<String> = timeline.iter().map(describe).collect();
    assert!(
        timeline.iter().any(|e| e["kind"] == "review_started" && e["attempt"] == 2 && e["round"] == 1),
        "harness: reviewer attempt 2 of round 1 must exist:\n{all:#?}"
    );
    let last = timeline.last().unwrap();
    assert_eq!((last["kind"].as_str(), last["role"].as_str()), (Some("provider_error"), Some("author")), "the recovery event:\n{all:#?}");
    assert_eq!(last["attempt"], 1, "the author-role error closes the AUTHOR's started entry (attempt 1):\n{all:#?}");
    assert_eq!(last["round"], 1, "the author-role error closes the author's started entry (round 1):\n{all:#?}");
    // The reviewer-role error of attempt 1 closes the reviewer's first start in the same way.
    let first_error = timeline.iter().find(|e| e["kind"] == "provider_error" && e["role"] == "reviewer").expect("reviewer error");
    assert_eq!((first_error["attempt"].clone(), first_error["round"].clone()), (serde_json::json!(1), serde_json::json!(1)), "{all:#?}");
}

// ---------------------------------------------------------------- save(): continue only a Running + Returned run

/// A completed three-round-maximum run that stops after round 1 and is then sent back (Return): its decision is Returned.
fn returned_run(root: &Path) -> (PathBuf, Handler) {
    let cfg = config(root, 3);
    let run = cfg.run_dir.clone();
    let author_calls = Arc::new(AtomicUsize::new(0));
    let h: Handler = {
        let author_calls = author_calls.clone();
        Arc::new(move |req, _| {
            Ok(outcome(match req.role {
                Role::Author => {
                    author_calls.fetch_add(1, Ordering::SeqCst);
                    author_doc(req.round)
                }
                Role::Reviewer => clean_review(),
            }))
        })
    };
    let control = Arc::new(RunControl::default());
    let c = control.clone();
    let s = start(cfg, clients(h.clone()), control, move |e| {
        if matches!(e, RunEvent::RoundCompleted { .. }) {
            c.pause.store(true, Ordering::SeqCst);
        }
    })
    .unwrap();
    assert_eq!(s.state, RunState::Paused, "setup: {:?}", s.last_error);
    stop_idle(&run).unwrap();
    let s = decide(&run, DecisionKind::Returned, "Rita-Returner-3310", "once more").unwrap();
    assert_eq!(s.state, RunState::Returned);
    assert_eq!(s.decision.as_ref().map(|d| d.kind), Some(DecisionKind::Returned));
    (run, h)
}
/// The published decision file becomes a directory: every later publication of output/final fails AFTER the state commit.
fn obstruct_final(run: &Path) {
    let dest = run.join("output/final/human_decision.json");
    fs::remove_file(&dest).unwrap();
    fs::create_dir(&dest).unwrap();
}

/// Positive: the run is Running and its committed decision is Returned; the incomplete publication must not stop it.
#[test]
fn test_mutants_save_lets_a_running_returned_run_continue_after_incomplete_publication() {
    let dir = tempfile::tempdir().unwrap();
    let (run, h) = returned_run(dir.path());
    let author_rounds = Arc::new(std::sync::Mutex::new(Vec::<u32>::new()));
    let h2: Handler = {
        let author_rounds = author_rounds.clone();
        Arc::new(move |req, sink| {
            if req.role == Role::Author {
                author_rounds.lock().unwrap().push(req.round);
            }
            h(req, sink)
        })
    };
    obstruct_final(&run);
    let s = resume(&run, clients(h2), Arc::default(), |_| {});
    let rounds = author_rounds.lock().unwrap().clone();
    assert!(
        rounds.contains(&2),
        "the returned run must go on to round 2 although the package cannot be published; author rounds called: {rounds:?}, resume: {:?}",
        s.as_ref().map(|s| (s.state, s.last_error.clone())).map_err(|e| e.to_string())
    );
    // The decision stays committed.
    assert_eq!(state_json(&run)["snapshot"]["decision"]["kind"], "returned");
}

/// Negative: the same obstruction when the run is NOT Running (load turns an interrupted run into Paused) must surface
/// as PublicationIncomplete, even though the committed decision is Returned.
#[test]
fn test_mutants_save_reports_incomplete_publication_of_a_returned_decision_unless_the_run_is_running() {
    let dir = tempfile::tempdir().unwrap();
    let (run, _h) = returned_run(dir.path());
    let mut st = state_json(&run);
    st["snapshot"]["state"] = serde_json::json!("running"); // an interrupted run: load pauses it and saves
    write_state(&run, &st);
    obstruct_final(&run);
    let r = load(&run);
    let err = r.expect_err("load of an interrupted run with an obstructed package must report the incomplete publication");
    assert!(err.downcast_ref::<PublicationIncomplete>().is_some(), "wrong error: {err}");
    // The paused state is durable.
    assert_eq!(state_json(&run)["snapshot"]["state"], "paused");
    assert_eq!(state_json(&run)["snapshot"]["decision"]["kind"], "returned");
}

/// Negative: a Running run whose committed decision is NOT Returned (an approval) is not continued either.
#[test]
fn test_mutants_save_does_not_continue_a_running_run_whose_decision_is_not_returned() {
    let dir = tempfile::tempdir().unwrap();
    let (run, h) = returned_run(dir.path());
    // Re-label the committed decision as an approval and keep the state Running (a crafted legacy state).
    let mut st = state_json(&run);
    st["snapshot"]["state"] = serde_json::json!("running");
    st["snapshot"]["decision"]["kind"] = serde_json::json!("approved");
    write_state(&run, &st);
    obstruct_final(&run);
    // resume saves with state Running and an Approved decision: the incomplete publication is an error, no provider call.
    let calls = Arc::new(AtomicUsize::new(0));
    let h2: Handler = {
        let calls = calls.clone();
        Arc::new(move |req, sink| {
            calls.fetch_add(1, Ordering::SeqCst);
            h(req, sink)
        })
    };
    let r = resume(&run, clients(h2), Arc::default(), |_| {});
    assert_eq!(calls.load(Ordering::SeqCst), 0, "a provider was called although the decision is not Returned");
    let err = r.expect_err("resume must report the incomplete publication");
    assert!(err.downcast_ref::<PublicationIncomplete>().is_some(), "wrong error: {err}");
}

#[test]
fn test_mutants_no_skip_or_ignore_in_this_file() {
    let src = include_str!("ct_mutants_engine.rs");
    let code: String = src.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
    for banned in [format!("#[{}", "ignore"), format!("x{}", "fail"), format!("should_{}", "panic"), format!("catch_{}", "unwind")] {
        assert!(!code.contains(banned.as_str()), "found {banned}");
    }
}
