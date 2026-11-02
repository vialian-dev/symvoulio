// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! R53: in real canary #3 the round-1 author raised two human questions (R01-001, R01-002) before any review; the
//! round-1 reviewer, which does not see them, reported a NEW issue R01-001 (`tests/fixtures/r53_real_review.json`, the
//! reviewer's answer byte for byte as in the evidence). The run paused as malformed («finding conflicts with question id»).
//! Rule (the mechanism is not fixed): the round completes, the question and the finding are two distinct entries (the
//! question in the human questions, the finding in the issues) with different ids, and final_review.html shows both texts.
//! A rejection that still happens must carry a controlled reason, not the generic «invalid structured output».
//! Decision (21:56Z): human questions get their own ids Q<round>-<nnn>, assigned by our code (the model's
//! issue_id for a question is never trusted); old runs with R-style question ids are mapped at read time.
//! The author's questions are served by the fake CLI's opt-in knob FAKE_CLI_AUTHOR_QUESTIONS, the review by
//! FAKE_CLI_REVIEWER_FILE.
use std::{fs, path::Path, sync::Arc, sync::Mutex, time::Duration};
use symvoulio_core::{
    engine::{decide_issue_with_answerer, read_snapshot, start, RunConfig, RunSnapshot},
    provider::{ClaudeCli, CodexCli, ProviderClient},
    HumanChoice, Policy, Provider, RunState,
};

const REAL_REVIEW: &str = include_str!("fixtures/r53_real_review.json");
const FINDING: &str = "Both decision-relevant figures lack supporting evidence";
const Q1: &str = "Who approves the capital budget?";
const Q2: &str = "Which currency do the totals use?";

static ENV: Mutex<()> = Mutex::new(());
struct Env;
impl Env {
    fn new(reviewer_file: &Path, author_questions: &str) -> Self {
        std::env::set_var("SYMVOULIO_FAKE_CLI", env!("CARGO_BIN_EXE_fake-cli"));
        std::env::set_var("FAKE_CLI_REVIEWER_FILE", reviewer_file);
        std::env::set_var("FAKE_CLI_AUTHOR_QUESTIONS", author_questions);
        std::env::remove_var("FAKE_CLI_SCENARIO");
        Self
    }
}
impl Drop for Env {
    fn drop(&mut self) {
        for k in ["SYMVOULIO_FAKE_CLI", "FAKE_CLI_REVIEWER_FILE", "FAKE_CLI_AUTHOR_QUESTIONS"] {
            std::env::remove_var(k);
        }
    }
}

/// `rounds` rounds (Claude author, fake Codex reviewer serving the real review in round 1).
/// `author_questions` is the knob value: a JSON array of per-round question arrays.
fn run(root: &Path, rounds: u32, author_questions: &str) -> (RunSnapshot, std::path::PathBuf) {
    run_with(root, rounds, author_questions, REAL_REVIEW)
}

/// The real review with its issue renamed so that it cannot collide with any author question id.
fn review_without_collision() -> String {
    REAL_REVIEW.replace("\"R01-001\"", "\"R01-005\"")
}

/// `review` None: the fake reviewer's own default answers (issues R01-001..R01-006 in round 1, consistent with its
/// round-2 answer), used where a second round must complete.
fn run_with(root: &Path, rounds: u32, author_questions: &str, review: &str) -> (RunSnapshot, std::path::PathBuf) {
    let served = root.join("reviewer_answer.json");
    let _env = if review.is_empty() {
        let env = Env::new(&served, author_questions);
        std::env::remove_var("FAKE_CLI_REVIEWER_FILE");
        env
    } else {
        fs::write(&served, review).unwrap();
        Env::new(&served, author_questions)
    };
    let primary = root.join("business.md");
    fs::write(&primary, "# Northwind\nCapital cost $4.6M. Growth 18 %.").unwrap();
    let cfg = RunConfig {
        run_id: "r53-run".into(),
        name: "r53".into(),
        run_dir: root.join("run"),
        author: Provider::Claude,
        objective: "Improve for executive review".into(),
        criteria: vec!["All factual claims must be supported.".into(), "All totals must reconcile.".into()],
        policy: Policy { max_rounds: rounds, min_rounds: rounds, new_content_limit_pct: 100.0, ..Policy::default() },
        primary,
        supporting: vec![],
        call_timeout: Duration::from_secs(10),
        author_model: None,
        reviewer_model: None,
    };
    let dir = cfg.run_dir.clone();
    let clients: (Arc<dyn ProviderClient>, Arc<dyn ProviderClient>) = (Arc::new(ClaudeCli), Arc::new(CodexCli));
    (start(cfg, clients, Arc::default(), |_| {}).unwrap(), dir)
}

fn two_questions() -> String {
    serde_json::json!([[
        {"issue_id": "R01-001", "question": Q1},
        {"issue_id": "R01-002", "question": Q2}
    ]])
    .to_string()
}

#[test]
fn test_dod1_val_r53_reviewer_issue_with_the_id_of_an_author_question_does_not_pause_and_both_stay() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let (s, run) = run(dir.path(), 1, &two_questions());

    assert_ne!(s.state, RunState::Paused, "the round paused: {:?}", s.last_error);
    assert!(s.last_error.is_none(), "the review was rejected: {:?}", s.last_error);
    assert_eq!(s.rounds_done, 1, "round 1 did not complete");

    let st: serde_json::Value = serde_json::from_slice(&fs::read(run.join("run_state.json")).unwrap()).unwrap();
    let snap = &st["snapshot"];
    let issues = snap["issues"].as_array().expect("issue list");
    let questions = snap["humanQuestions"].as_array().expect("human question list");

    // Both are present as separate entries; neither overwrote the other.
    let finding = issues
        .iter()
        .find(|i| i["issue"]["finding"].as_str().is_some_and(|f| f.contains(FINDING)))
        .unwrap_or_else(|| panic!("the reviewer's finding is not in the issues: {issues:#?}"));
    for (text, what) in [(Q1, "first"), (Q2, "second")] {
        assert!(
            questions.iter().any(|q| q["question"].as_str() == Some(text)),
            "the author's {what} question is not in the human questions: {questions:#?}"
        );
    }
    assert!(
        !issues.iter().any(|i| i.to_string().contains(Q1)),
        "the question text sits in the issue list (overwrote or merged with the finding)"
    );
    assert!(
        !questions.iter().any(|q| q.to_string().contains(FINDING)),
        "the finding text sits in the question list"
    );

    // Different ids in the final snapshot, whatever the scheme.
    let finding_id = finding["issue"]["issue_id"].as_str().expect("finding id");
    let question_ids: Vec<&str> = questions.iter().filter_map(|q| q["issue_id"].as_str()).collect();
    assert!(
        !question_ids.contains(&finding_id),
        "the finding {finding_id} shares its id with a question: {question_ids:?}"
    );
    let mut sorted = question_ids.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), question_ids.len(), "two questions share an id: {question_ids:?}");
    // Decision (21:56Z): questions live in their own namespace Q<round>-<nnn>; the issue keeps R01-001.
    assert_eq!(finding_id, "R01-001", "the reviewer's issue lost its id");
    assert_eq!(question_ids.len(), 2, "question ids: {question_ids:?}");
    for id in &question_ids {
        assert!(is_q_id(id), "question id {id} is not Q<round>-<nnn>");
    }

    // The export shows both.
    let html = fs::read_to_string(run.join("output/final/final_review.html")).unwrap();
    assert!(html.contains(FINDING), "final_review.html lacks the finding text");
    assert!(html.contains(Q1), "final_review.html lacks the first question text");
    assert!(html.contains(Q2), "final_review.html lacks the second question text");
}

/// Under the Q-id scheme the model's question id is ignored, so the only remaining collision rejection is the defensive
/// one in the public validation, which must keep its controlled reason (not the generic «invalid structured output»).
#[test]
fn test_dod1_val_r53_control_defensive_question_id_conflict_has_a_controlled_reason() {
    assert_eq!(
        symvoulio_core::provider::validation_reason("finding conflicts with question id"),
        Some("finding conflicts with question id".to_string())
    );
}

fn is_q_id(id: &str) -> bool {
    let b = id.as_bytes();
    b.len() == 7 && b[0] == b'Q' && b[3] == b'-' && b[1..3].iter().chain(&b[4..]).all(u8::is_ascii_digit)
}

/// The ids in the model's `human_decisions_required` are never trusted: identical or garbage ids still give one
/// recorded question each, with distinct Q ids, and nothing pauses.
fn model_ids_are_ignored(model_ids: [&str; 2]) {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let knob = serde_json::json!([[
        {"issue_id": model_ids[0], "question": Q1},
        {"issue_id": model_ids[1], "question": Q2}
    ]])
    .to_string();
    let (s, _run) = run_with(dir.path(), 1, &knob, &review_without_collision());
    assert_ne!(s.state, RunState::Paused, "the round paused: {:?}", s.last_error);
    assert!(s.last_error.is_none(), "{:?}", s.last_error);
    let ids: Vec<&str> = s.human_questions.iter().map(|q| q.issue_id.as_str()).collect();
    for text in [Q1, Q2] {
        assert_eq!(
            s.human_questions.iter().filter(|q| q.question == text).count(),
            1,
            "question «{text}» not recorded exactly once: {:?}",
            s.human_questions
        );
    }
    assert_eq!(ids.len(), 2, "question ids: {ids:?}");
    assert_ne!(ids[0], ids[1], "the two questions share an id: {ids:?}");
    for id in &ids {
        assert!(is_q_id(id), "question id {id} is not Q<round>-<nnn> (the model's id was trusted)");
    }
}

#[test]
fn test_dod1_val_r53_two_questions_with_the_same_model_id_are_both_recorded_with_distinct_q_ids() {
    model_ids_are_ignored(["R01-001", "R01-001"]);
}

#[test]
fn test_dod1_val_r53_two_questions_with_garbage_model_ids_are_both_recorded_with_distinct_q_ids() {
    model_ids_are_ignored(["x", ""]);
}

/// The round-2 author repeats a round-1 question verbatim (any model id): still one entry.
#[test]
fn test_dod1_val_r53_a_repeated_question_in_round_2_is_not_recorded_twice() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let knob = serde_json::json!([
        [{"issue_id": "R01-101", "question": Q1}, {"issue_id": "R01-102", "question": Q2}],
        [{"issue_id": "R01-101", "question": Q1}]
    ])
    .to_string();
    // The fake reviewer's default answers (R01-001..R01-006), so that round 2 completes; the model ids do not collide.
    let (s, _run) = run_with(dir.path(), 2, &knob, "");
    assert_ne!(s.state, RunState::Paused, "the run paused: {:?}", s.last_error);
    assert_eq!(s.rounds_done, 2, "round 2 did not complete");
    assert_eq!(
        s.human_questions.iter().filter(|q| q.question == Q1).count(),
        1,
        "the repeated question is recorded more than once: {:?}",
        s.human_questions
    );
    assert_eq!(s.human_questions.len(), 2, "{:?}", s.human_questions);
}

/// The real answer path the app uses (`commands.rs` -> `engine::decide_issue_with_answerer` -> `Ledger::apply_human`).
#[test]
fn test_dod1_val_r53_a_q_id_question_can_be_answered() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let (s, run) = run_with(dir.path(), 1, &two_questions(), &review_without_collision());
    assert!(s.last_error.is_none(), "{:?}", s.last_error);
    let q = s.human_questions.iter().find(|q| q.question == Q1).expect("first question");
    assert!(is_q_id(&q.issue_id), "question id {} is not Q<round>-<nnn>", q.issue_id);
    let qid = q.issue_id.clone();
    let other = s.human_questions.iter().find(|q| q.question == Q2).expect("second question").issue_id.clone();
    let after = decide_issue_with_answerer(&run, &qid, HumanChoice::Accept, "The board.", Some("Ann")).unwrap();
    let answered = after.human_questions.iter().find(|q| q.issue_id == qid).expect("answered question");
    assert!(answered.resolved, "{answered:?}");
    assert_eq!(answered.answer.as_deref(), Some("The board."));
    assert_eq!(answered.answered_by.as_deref(), Some("Ann"));
    assert!(!after.human_questions.iter().find(|q| q.issue_id == other).unwrap().resolved, "the other question was answered too");
    let again = read_snapshot(&run).unwrap();
    assert!(again.human_questions.iter().any(|q| q.issue_id == qid && q.resolved), "the answer did not persist");
}

/// Every file under `dir` with its bytes, to prove a read changes nothing.
fn tree(dir: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    fn walk(base: &Path, dir: &Path, out: &mut std::collections::BTreeMap<String, Vec<u8>>) {
        for e in fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(base, &p, out);
            } else {
                out.insert(p.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/"), fs::read(&p).unwrap());
            }
        }
    }
    let mut out = Default::default();
    walk(dir, dir, &mut out);
    out
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for e in fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &to.join(e.file_name()));
        } else {
            fs::copy(e.path(), to.join(e.file_name())).unwrap();
        }
    }
}

/// `fixtures/r53_old_run` was written by the code at ead3e95 (a real fake-CLI round, author questions R01-001 and R01-002,
/// a non-colliding reviewer issue R01-005), run_state.json and the inputs/output folders copied unchanged.
#[test]
fn test_dod1_val_r53_old_run_with_r_style_question_ids_is_read_shown_and_answerable() {
    let dir = tempfile::tempdir().unwrap();
    let run = dir.path().join("run");
    copy_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/r53_old_run"), &run);
    let before = tree(&run);

    let s = read_snapshot(&run).expect("the old run is readable");
    let texts: Vec<&str> = s.human_questions.iter().map(|q| q.question.as_str()).collect();
    assert_eq!(texts.len(), 2, "{:?}", s.human_questions);
    assert!(texts.contains(&Q1) && texts.contains(&Q2), "question texts: {texts:?}");
    assert!(s.human_questions.iter().all(|q| !q.resolved), "{:?}", s.human_questions);
    let ids: Vec<String> = s.human_questions.iter().map(|q| q.issue_id.clone()).collect();
    assert_ne!(ids[0], ids[1], "the two questions share an id: {ids:?}");
    for id in &ids {
        assert!(is_q_id(id), "question id {id} of the old run is not Q<round>-<nnn>");
    }
    assert_eq!(read_snapshot(&run).unwrap().human_questions.iter().map(|q| q.issue_id.clone()).collect::<Vec<_>>(), ids, "the mapping is not stable between reads");

    // R48: nothing disagrees between run_state.json and the exports, so a plain read writes nothing.
    assert_eq!(tree(&run).keys().collect::<Vec<_>>(), before.keys().collect::<Vec<_>>(), "the read added or removed files");
    for (name, bytes) in &before {
        assert!(tree(&run)[name] == *bytes, "the read rewrote {name}");
    }

    // The human can still answer one, by the id the read shows.
    let q = s.human_questions.iter().find(|q| q.question == Q2).unwrap();
    let after = decide_issue_with_answerer(&run, &q.issue_id, HumanChoice::Accept, "USD.", Some("Ann")).unwrap();
    let answered = after.human_questions.iter().find(|x| x.question == Q2).unwrap();
    assert!(answered.resolved && answered.answer.as_deref() == Some("USD."), "{answered:?}");
    assert!(!after.human_questions.iter().find(|x| x.question == Q1).unwrap().resolved);
    let again = read_snapshot(&run).unwrap();
    assert!(again.human_questions.iter().any(|x| x.question == Q2 && x.resolved), "the answer did not persist");
}

/// P7-INT control: a genuinely STALE derived file of an old run is not silently accepted. The read either repairs it (the
/// bytes become what the untouched fixture holds, i.e. what the consistency check renders from the stored, unmapped state)
/// or reports incomplete publication. The returned question ids are Q-style either way.
fn stale_old_package_is_repaired(rel: &str, stale: impl Fn(&[u8]) -> Vec<u8>) {
    let dir = tempfile::tempdir().unwrap();
    let run = dir.path().join("run");
    copy_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/r53_old_run"), &run);
    let pristine = tree(&run);
    let file = run.join(rel);
    let old = pristine[rel].clone();
    let staled = stale(&old);
    assert_ne!(staled, old, "harness: {rel} was not made stale");
    fs::write(&file, &staled).unwrap();

    let s = read_snapshot(&run).expect("the old run with a stale file is readable");
    assert_eq!(s.human_questions.len(), 2, "{:?}", s.human_questions);
    for q in &s.human_questions {
        assert!(is_q_id(&q.issue_id), "question id {} of the old run is not Q<round>-<nnn>", q.issue_id);
    }
    let now = fs::read(&file).unwrap();
    if now == staled {
        let e = s.last_error.as_ref().unwrap_or_else(|| panic!("the stale {rel} was silently accepted"));
        assert_eq!(e.kind, "publication", "the stale {rel} was left in place with the wrong error: {e:?}");
    } else {
        assert!(now == old, "{rel} was rewritten, but not to what the stored state renders");
        assert_eq!(tree(&run).keys().collect::<Vec<_>>(), pristine.keys().collect::<Vec<_>>(), "the repair added or removed files");
        let after = tree(&run);
        for (name, bytes) in &pristine {
            // Same rule as the product's consistency check: a .docx is a zip whose bytes may differ (timestamps,
            // compression) while its parts are identical, so it is compared part by part.
            let same = if name.ends_with(".docx") { docx_parts(&after[name]) == docx_parts(bytes) } else { after[name] == *bytes };
            assert!(same, "the repair of {rel} also changed {name}");
        }
    }
}

/// Every part of a zip with its uncompressed bytes.
fn docx_parts(bytes: &[u8]) -> std::collections::BTreeMap<String, Vec<u8>> {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("a readable docx");
    let mut parts = std::collections::BTreeMap::new();
    for i in 0..zip.len() {
        let mut part = zip.by_index(i).unwrap();
        let mut data = Vec::new();
        part.read_to_end(&mut data).unwrap();
        parts.insert(part.name().to_owned(), data);
    }
    parts
}

#[test]
fn test_dod1_val_r53_old_run_with_a_stale_final_review_is_repaired_not_accepted() {
    stale_old_package_is_repaired("output/final/final_review.html", |old| {
        let text = String::from_utf8_lossy(old).into_owned();
        let kept: Vec<&str> = text.lines().collect();
        kept[..kept.len() - 1].join("\n").into_bytes()
    });
}

#[test]
fn test_dod1_val_r53_old_run_with_a_stale_timeline_is_repaired_not_accepted() {
    stale_old_package_is_repaired("output/audit/timeline.json", |_| b"[]\n".to_vec());
}

#[test]
fn test_dod3_no_skip_or_ignore_in_this_file() {
    let src = include_str!("ct_r53_question_collision.rs");
    let code: String = src.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
    for banned in [format!("#[{}", "ignore"), format!("x{}", "fail"), format!("should_{}", "panic"), format!("catch_{}", "unwind")] {
        assert!(!code.contains(banned.as_str()), "found {banned}");
    }
}
