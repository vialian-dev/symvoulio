// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! CRUEL TESTER checks for card M1-CORE. Expectations come from the card (VAL1..VAL15, DoD, SUB) and the docs,
//! never from the code under test.
use regex::Regex;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::Ordering,
        Arc, Mutex, MutexGuard,
    },
    time::{Duration, Instant},
};
use symvoulio_core::{engine::*, provider::*, *};

// ---------------------------------------------------------------------------------------------------------------
// harness
// ---------------------------------------------------------------------------------------------------------------
static ENV: Mutex<()> = Mutex::new(());
const ENV_KEYS: [&str; 4] = [
    "SYMVOULIO_FAKE_CLI",
    "FAKE_CLI_SCENARIO",
    "FAKE_CLI_LOGGED_OUT",
    "FAKE_CLI_LOG",
];
struct EnvGuard(#[allow(dead_code)] MutexGuard<'static, ()>);
fn env() -> EnvGuard {
    let g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    for key in ENV_KEYS {
        std::env::remove_var(key);
    }
    std::env::set_var("SYMVOULIO_FAKE_CLI", env!("CARGO_BIN_EXE_fake-cli"));
    EnvGuard(g)
}
impl Drop for EnvGuard {
    fn drop(&mut self) {
        for key in ENV_KEYS {
            std::env::remove_var(key);
        }
    }
}

const DOC: &str = "# Northwind\nCapital cost $4.2M. Growth 18 %. owner to be confirmed by Operations";

type Hook = Arc<dyn Fn(&CallRequest) + Send + Sync>;
type Seen = Arc<Mutex<Vec<(Role, u32, String, PathBuf)>>>;
/// Recording wrapper around the real adapters: sees every prompt and working folder, can inject per-call env.
struct Rec {
    inner: Arc<dyn ProviderClient>,
    seen: Seen,
    hook: Option<Hook>,
}
impl ProviderClient for Rec {
    fn provider(&self) -> Provider {
        self.inner.provider()
    }
    fn call(
        &self,
        req: &CallRequest,
        sink: &mut dyn FnMut(StreamLine),
        cancel: &CancelToken,
    ) -> std::result::Result<CallOutcome, CallError> {
        if let Some(h) = &self.hook {
            h(req);
        }
        self.seen.lock().unwrap().push((
            req.role,
            req.round,
            req.prompt.clone(),
            req.workdir.clone(),
        ));
        self.inner.call(req, sink, cancel)
    }
}
type Clients = (Arc<dyn ProviderClient>, Arc<dyn ProviderClient>);
fn clients() -> Clients {
    (Arc::new(ClaudeCli), Arc::new(CodexCli))
}
fn clients_codex_author() -> Clients {
    (Arc::new(CodexCli), Arc::new(ClaudeCli))
}
fn rec_clients(author: Provider, seen: &Seen, hook: Option<Hook>) -> Clients {
    let mk = |p: Arc<dyn ProviderClient>| -> Arc<dyn ProviderClient> {
        Arc::new(Rec {
            inner: p,
            seen: seen.clone(),
            hook: hook.clone(),
        })
    };
    if author == Provider::Claude {
        (mk(Arc::new(ClaudeCli)), mk(Arc::new(CodexCli)))
    } else {
        (mk(Arc::new(CodexCli)), mk(Arc::new(ClaudeCli)))
    }
}
fn cfg_in(dir: &Path, min: u32, max: u32, pct: f64) -> RunConfig {
    let primary = dir.join("business.md");
    fs::write(&primary, DOC).unwrap();
    RunConfig {
        run_id: "test-run".into(),
        name: "Northwind".into(),
        run_dir: dir.join("run"),
        author: Provider::Claude,
        objective: "Improve for executive review".into(),
        criteria: vec!["All totals must reconcile.".into()],
        policy: Policy {
            max_rounds: max,
            min_rounds: min,
            new_content_limit_pct: pct,
            ..Policy::default()
        },
        primary,
        supporting: vec![],
        call_timeout: Duration::from_secs(5),
        author_model: None,
        reviewer_model: None,
    }
}
fn cfg(dir: &Path) -> RunConfig {
    cfg_in(dir, 2, 5, 100.0)
}
fn bytes(p: impl AsRef<Path>) -> Vec<u8> {
    fs::read(p.as_ref()).unwrap_or_else(|e| panic!("{}: {e}", p.as_ref().display()))
}
fn text(p: impl AsRef<Path>) -> String {
    String::from_utf8(bytes(p)).unwrap()
}
fn jsonf(p: impl AsRef<Path>) -> Value {
    serde_json::from_slice(&bytes(p)).unwrap()
}
fn sha(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn walk(root: &Path) -> Vec<String> {
    fn go(base: &Path, dir: &Path, out: &mut Vec<String>) {
        for e in fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                go(base, &p, out);
            } else {
                out.push(
                    p.strip_prefix(base)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    let mut out = vec![];
    go(root, root, &mut out);
    out.sort();
    out
}
fn provider_calls(log: &Path) -> Vec<Vec<String>> {
    if !log.exists() {
        return vec![];
    }
    text(log)
        .lines()
        .map(|l| serde_json::from_str::<Vec<String>>(l).unwrap())
        .filter(|a| !a.iter().any(|s| s == "--version"))
        .collect()
}
fn mark_running_on_disk(run: &Path) {
    let path = run.join("run_state.json");
    let mut raw = jsonf(&path);
    raw["snapshot"]["state"] = json!("running");
    fs::write(path, serde_json::to_vec(&raw).unwrap()).unwrap();
}
fn state_str(s: RunState) -> String {
    serde_json::to_value(s).unwrap().as_str().unwrap().to_string()
}
fn live_fake_clis() -> usize {
    let exe = env!("CARGO_BIN_EXE_fake-cli");
    let mut n = 0;
    if let Ok(rd) = fs::read_dir("/proc") {
        for e in rd.flatten() {
            if let Ok(c) = fs::read(e.path().join("cmdline")) {
                if c.starts_with(exe.as_bytes()) {
                    n += 1;
                }
            }
        }
    }
    n
}
/// Stop the run while the reviewer of round `n` works (revision `n` is already written).
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
fn stopped_in_review(n: u32, min: u32, max: u32) -> (tempfile::TempDir, PathBuf, RunSnapshot) {
    std::env::remove_var("FAKE_CLI_SCENARIO");
    let dir = tempfile::tempdir().unwrap();
    let c = cfg_in(dir.path(), min, max, 100.0);
    let run = c.run_dir.clone();
    let control = Arc::new(RunControl::default());
    let s = start(c, clients(), control.clone(), stop_in_review_of(n, control)).unwrap();
    std::env::remove_var("FAKE_CLI_SCENARIO");
    (dir, run, s)
}

// gate helpers -------------------------------------------------------------------------------------------------
fn issue(id: &str, severity: Severity, status: Status) -> Issue {
    Issue {
        issue_id: id.into(),
        severity,
        criterion: "c".into(),
        finding: format!("finding {id}"),
        evidence: "e".into(),
        proposed_action: "a".into(),
        human_decision_required: false,
        status,
    }
}
use Severity::{Critical as C, High as H, Low as L, Medium as M};
use Status::{Open as O, Resolved as R};
fn ledger_of(rounds: &[Vec<Issue>]) -> IssueLedger {
    let mut l = IssueLedger::default();
    for (i, r) in rounds.iter().enumerate() {
        l.apply_review(
            i as u32 + 1,
            &Review {
                summary: "s".into(),
                issues: r.clone(),
                criteria_results: vec![],
            },
        );
    }
    l
}
fn gate_at(rounds: &[Vec<Issue>], r: u32, min: u32, max: u32) -> Gate {
    let l = ledger_of(&rounds[..r as usize]);
    gate_after(
        r,
        &Policy {
            max_rounds: max,
            min_rounds: min,
            ..Policy::default()
        },
        &l,
    )
}

// ---------------------------------------------------------------------------------------------------------------
// VAL1 — policy bounds
// ---------------------------------------------------------------------------------------------------------------
#[test]
fn test_dod1_val1_policy_bounds_exhaustive() {
    for max in 0..=7u32 {
        for min in 0..=7u32 {
            let p = Policy {
                max_rounds: max,
                min_rounds: min,
                ..Policy::default()
            };
            let ok = (1..=5).contains(&max) && (1..=5).contains(&min) && min <= max;
            assert_eq!(p.validate().is_ok(), ok, "max={max} min={min}");
        }
    }
}
#[test]
fn test_dod1_val1_invalid_policy_is_refused_at_start_without_side_effects() {
    let _g = env();
    for (min, max) in [(1, 6), (0, 3), (3, 2), (6, 6), (2, 0), (5, 4), (0, 0)] {
        let dir = tempfile::tempdir().unwrap();
        let c = cfg_in(dir.path(), min, max, 100.0);
        let run = c.run_dir.clone();
        let log = dir.path().join("calls.jsonl");
        std::env::set_var("FAKE_CLI_LOG", &log);
        assert!(
            start(c, clients(), Arc::default(), |_| {}).is_err(),
            "min={min} max={max} must be refused"
        );
        assert!(!run.join("run_state.json").exists(), "min={min} max={max}");
        assert!(provider_calls(&log).is_empty(), "no provider call on refusal");
    }
    // boundary values are accepted
    for (min, max) in [(1, 1), (5, 5), (1, 5)] {
        let dir = tempfile::tempdir().unwrap();
        let c = cfg_in(dir.path(), min, max, 100.0);
        assert!(
            start(c, clients(), Arc::default(), |_| {}).is_ok(),
            "min={min} max={max}"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------
// VAL2 — different providers
// ---------------------------------------------------------------------------------------------------------------
#[test]
fn test_dod1_val2_same_provider_is_refused_with_exact_message() {
    let _g = env();
    for (a, b) in [
        (
            Arc::new(ClaudeCli) as Arc<dyn ProviderClient>,
            Arc::new(ClaudeCli) as Arc<dyn ProviderClient>,
        ),
        (Arc::new(CodexCli), Arc::new(CodexCli)),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let c = cfg(dir.path());
        let run = c.run_dir.clone();
        let err = start(c, (a, b), Arc::default(), |_| {}).unwrap_err();
        assert_eq!(err.to_string(), "No model grades its own work.");
        assert!(!run.join("run_state.json").exists());
    }
    // both provider pairings work
    for p in [Provider::Claude, Provider::Codex] {
        let dir = tempfile::tempdir().unwrap();
        let mut c = cfg(dir.path());
        c.author = p;
        let pair = if p == Provider::Claude {
            clients()
        } else {
            clients_codex_author()
        };
        let s = start(c, pair, Arc::default(), |_| {}).unwrap();
        assert_eq!(s.author, p);
        assert_eq!(s.reviewer, p.other());
        assert_eq!(s.state, RunState::Completed);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// VAL3 — gate
// ---------------------------------------------------------------------------------------------------------------
#[test]
fn test_dod1_val3_critical_or_high_open_continues_below_max() {
    let rounds = vec![vec![issue("R01-001", C, O)], vec![issue("R01-001", C, O)]];
    let g = gate_at(&rounds, 2, 1, 5);
    assert_eq!(g.kind, GateKind::Continue);
    assert_eq!(g.reason.stop_reason(), None);
    assert_eq!(g.counts.critical, 1);
    let rounds = vec![vec![issue("R01-001", H, O)], vec![issue("R01-001", H, O)]];
    let g = gate_at(&rounds, 2, 1, 5);
    assert_eq!(g.kind, GateKind::Continue);
    assert_eq!(g.counts.high, 1);
    // a high that is resolved no longer blocks
    let rounds = vec![
        vec![issue("R01-001", H, O)],
        vec![issue("R01-001", H, R)],
        vec![issue("R01-001", H, R)],
    ];
    assert_eq!(gate_at(&rounds, 3, 1, 5).kind, GateKind::Stop);
}
#[test]
fn test_dod1_val3_minimum_rounds_not_done_continues() {
    let rounds = vec![vec![], vec![], vec![]];
    assert_eq!(gate_at(&rounds, 2, 3, 5).kind, GateKind::Continue);
    assert_eq!(gate_at(&rounds, 3, 3, 5).kind, GateKind::Stop);
    assert_eq!(gate_at(&rounds, 2, 2, 5).kind, GateKind::Stop);
    assert_eq!(gate_at(&rounds, 2, 5, 5).kind, GateKind::Continue);
}
#[test]
fn test_dod1_val3_single_review_cannot_satisfy_two_consecutive() {
    // spec: "two consecutive reviews found no new material issues" — one review is not two.
    let rounds = vec![vec![]];
    assert_eq!(gate_at(&rounds, 1, 1, 5).kind, GateKind::Continue);
    let rounds = vec![vec![issue("R01-001", L, O)]];
    assert_eq!(gate_at(&rounds, 1, 1, 5).kind, GateKind::Continue);
}
#[test]
fn test_dod1_val3_two_quiet_reviews_stop_and_new_material_blocks() {
    let a = || issue("R01-001", M, O);
    let rounds = vec![vec![a()], vec![a()], vec![a()]];
    assert_eq!(gate_at(&rounds, 1, 1, 5).kind, GateKind::Continue);
    assert_eq!(gate_at(&rounds, 2, 1, 5).kind, GateKind::Continue, "round 1 was new");
    let g = gate_at(&rounds, 3, 1, 5);
    assert_eq!(g.kind, GateKind::Stop);
    assert_eq!(g.reason.stop_reason(), Some(StopReason::GateNoNewIssues));
    // new material in the latest round blocks; the following quiet round is still only one quiet review
    let b = || issue("R03-001", M, O);
    let rounds = vec![vec![a()], vec![a()], vec![a(), b()], vec![a(), b()]];
    assert_eq!(gate_at(&rounds, 3, 1, 5).kind, GateKind::Continue);
    assert_eq!(gate_at(&rounds, 4, 1, 5).kind, GateKind::Continue);
}
#[test]
fn test_dod1_val3_max_rounds_with_blocking_issues_is_stopped_for_human_review() {
    for sev in [C, H] {
        for max in 1..=5u32 {
            let rounds: Vec<Vec<Issue>> = (0..max).map(|_| vec![issue("R01-001", sev, O)]).collect();
            let g = gate_at(&rounds, max, 1, max);
            assert_eq!(g.kind, GateKind::Stopped, "{sev:?} max {max}");
            assert_eq!(
                g.reason.stop_reason(),
                Some(StopReason::GateHumanReviewRequired)
            );
            if max > 1 {
                assert_eq!(gate_at(&rounds, max - 1, 1, max).kind, GateKind::Continue);
            }
        }
    }
    assert_eq!(
        serde_json::to_value(StopReason::GateHumanReviewRequired).unwrap(),
        json!("gate_human_review_required")
    );
    // nothing open at the cap: a plain Stop, not a human-review stop
    let rounds = vec![vec![], vec![]];
    let g = gate_at(&rounds, 2, 1, 2);
    assert_eq!(g.kind, GateKind::Stop);
    assert_ne!(g.reason.stop_reason(), Some(StopReason::GateHumanReviewRequired));
    assert!(g.reason.stop_reason().is_some());
}
#[test]
fn test_dod1_val3_omitted_issue_stays_open() {
    let rounds = vec![vec![issue("R01-001", H, O)], vec![]];
    let l = ledger_of(&rounds);
    assert_eq!(l.open_counts().high, 1, "a reviewer omitting an issue does not close it");
    assert_eq!(gate_at(&rounds, 2, 1, 5).kind, GateKind::Continue);
}
#[test]
fn test_dod1_val3_engine_stops_at_own_max_with_human_review_reason() {
    let _g = env();
    // fake reviews: round 1 has a critical, round 2 still two highs.
    for (min, max) in [(1u32, 1u32), (1, 2), (2, 2)] {
        let dir = tempfile::tempdir().unwrap();
        let c = cfg_in(dir.path(), min, max, 100.0);
        let run = c.run_dir.clone();
        let s = start(c, clients(), Arc::default(), |_| {}).unwrap();
        assert_eq!(s.rounds_done, max, "min {min} max {max}");
        assert_eq!(s.state, RunState::Stopped, "min {min} max {max}");
        assert_eq!(s.stop_reason, Some(StopReason::GateHumanReviewRequired));
        let m = jsonf(run.join("output/audit/run_manifest.json"));
        assert_eq!(m["stop_reason"], "gate_human_review_required");
        assert_eq!(m["state"], "stopped");
        assert_eq!(m["rounds_completed"], max);
    }
}
#[test]
fn test_dod1_val3_engine_default_policy_gate_sequence() {
    let _g = env();
    let dir = tempfile::tempdir().unwrap();
    let c = cfg(dir.path());
    let gates = Arc::new(Mutex::new(vec![]));
    let g2 = gates.clone();
    let s = start(c, clients(), Arc::default(), move |e| {
        if let RunEvent::RoundCompleted { round, gate, .. } = e {
            g2.lock().unwrap().push((round, gate.kind));
        }
    })
    .unwrap();
    assert_eq!(
        *gates.lock().unwrap(),
        vec![
            (1, GateKind::Continue),
            (2, GateKind::Continue),
            (3, GateKind::Stop)
        ]
    );
    assert_eq!((s.state, s.rounds_done), (RunState::Completed, 3));
    assert_eq!(s.stop_reason, Some(StopReason::GateNoNewIssues));
    assert_eq!(s.rounds.len(), 3);
    assert_eq!(s.rounds[2].gate.kind, GateKind::Stop);
}
#[test]
fn test_dod1_val3_engine_min_rounds_delay_the_stop() {
    let _g = env();
    for (min, expect) in [(1u32, 3u32), (2, 3), (3, 3), (4, 4), (5, 5)] {
        let dir = tempfile::tempdir().unwrap();
        let c = cfg_in(dir.path(), min, 5, 100.0);
        let s = start(c, clients(), Arc::default(), |_| {}).unwrap();
        assert_eq!(s.rounds_done, expect, "min {min}");
    }
}

// ---------------------------------------------------------------------------------------------------------------
// VAL4 — material, escalation, reopening (SUB1)
// ---------------------------------------------------------------------------------------------------------------
#[test]
fn test_dod1_val4_new_material_by_severity() {
    for (sev, material) in [(C, 1), (H, 1), (M, 1), (L, 0)] {
        let l = ledger_of(&[vec![issue("R01-001", sev, O)]]);
        assert_eq!(l.new_material_in(1), material, "{sev:?}");
    }
    // a first report that is already resolved is history only
    let l = ledger_of(&[vec![issue("R01-001", H, R)]]);
    assert_eq!(l.new_material_in(1), 0);
    // unchanged open issue is not new
    let l = ledger_of(&[vec![issue("R01-001", H, O)], vec![issue("R01-001", H, O)]]);
    assert_eq!(l.new_material_in(2), 0);
    // low stays low: not material, not escalated
    let l = ledger_of(&[vec![issue("R01-001", L, O)], vec![issue("R01-001", L, O)]]);
    assert_eq!(l.new_material_in(2), 0);
}
#[test]
fn test_dod1_val4_escalation_from_low_counts_as_new() {
    for sev in [M, H, C] {
        let l = ledger_of(&[vec![issue("R01-001", L, O)], vec![issue("R01-001", sev, O)]]);
        assert_eq!(l.new_material_in(1), 0);
        assert_eq!(l.new_material_in(2), 1, "low -> {sev:?}");
    }
}
#[test]
fn test_dod1_val4_reopening_counts_as_new() {
    for sev in [M, H, C] {
        let l = ledger_of(&[
            vec![issue("R01-001", sev, O)],
            vec![issue("R01-001", sev, R)],
            vec![issue("R01-001", sev, O)],
        ]);
        assert_eq!(l.new_material_in(2), 0, "resolution is not new material");
        assert_eq!(l.new_material_in(3), 1, "reopened {sev:?}");
    }
    // a low issue that is reopened is still not material
    let l = ledger_of(&[
        vec![issue("R01-001", L, O)],
        vec![issue("R01-001", L, R)],
        vec![issue("R01-001", L, O)],
    ]);
    assert_eq!(l.new_material_in(3), 0);
}
#[test]
fn test_dod1_val4_sub1_gate_sees_escalation_that_leaves_counts_unchanged() {
    // round 1: A low, B medium; round 2: A medium (escalated), B resolved. Open counts are medium=1 in both rounds.
    let rounds = vec![
        vec![issue("R01-001", L, O), issue("R01-002", M, O)],
        vec![issue("R01-001", M, O), issue("R01-002", M, R)],
        vec![issue("R01-001", M, O), issue("R01-002", M, R)],
        vec![issue("R01-001", M, O), issue("R01-002", M, R)],
    ];
    let l = ledger_of(&rounds[..2]);
    assert_eq!(l.new_material_in(2), 1);
    assert_eq!(gate_at(&rounds, 2, 1, 5).kind, GateKind::Continue);
    let g3 = gate_at(&rounds, 3, 1, 5);
    assert_eq!(g3.counts.medium, 1);
    assert_eq!(
        g3.kind,
        GateKind::Continue,
        "round 2 escalated low->medium: round 3 is only one quiet review"
    );
    assert_eq!(gate_at(&rounds, 4, 1, 5).kind, GateKind::Stop);
}
#[test]
fn test_dod1_val4_sub1_gate_sees_reopening_that_leaves_counts_unchanged() {
    // round 3: A resolved, B open; round 4: A reopened, B resolved. medium count is 1 in both rounds.
    let rounds = vec![
        vec![issue("R01-001", M, O), issue("R01-002", M, O)],
        vec![issue("R01-001", M, O), issue("R01-002", M, O)],
        vec![issue("R01-001", M, R), issue("R01-002", M, O)],
        vec![issue("R01-001", M, O), issue("R01-002", M, R)],
    ];
    assert_eq!(gate_at(&rounds, 3, 1, 5).kind, GateKind::Stop, "rounds 2 and 3 are quiet");
    let g4 = gate_at(&rounds, 4, 1, 5);
    assert_eq!(g4.counts.medium, 1);
    assert_eq!(g4.kind, GateKind::Continue, "A reopened in round 4");
}
#[test]
fn test_dod1_val4_low_findings_never_block_the_stop() {
    let rounds = vec![
        vec![issue("R01-001", L, O)],
        vec![issue("R01-001", L, O), issue("R02-001", L, O)],
    ];
    let g = gate_at(&rounds, 2, 1, 5);
    assert_eq!(g.kind, GateKind::Stop);
    assert_eq!(g.counts.low, 2);
}

// ---------------------------------------------------------------------------------------------------------------
// VAL5 — new-text limit
// ---------------------------------------------------------------------------------------------------------------
fn pcts(limit: f64, min: u32, max: u32) -> (RunSnapshot, Vec<f64>) {
    let dir = tempfile::tempdir().unwrap();
    let c = cfg_in(dir.path(), min, max, limit);
    let s = start(c, clients(), Arc::default(), |_| {}).unwrap();
    let p = s.rounds.iter().map(|r| r.new_text_pct).collect();
    // keep the temp dir alive only for the call
    (s, p)
}
#[test]
fn test_dod1_val5_limit_zero_never_ends_with_stop() {
    let _g = env();
    let (_, p) = pcts(100.0, 2, 5);
    assert!(p.iter().all(|x| *x > 0.0), "fake revisions add words: {p:?}");
    for (min, max) in [(2u32, 5u32), (1, 3), (1, 2)] {
        let (s, _) = pcts(0.0, min, max);
        assert_eq!(s.rounds_done, max, "min {min} max {max}: the run must go to its cap");
        assert_eq!(s.state, RunState::Stopped, "min {min} max {max}");
        assert_eq!(s.stop_reason, Some(StopReason::GateHumanReviewRequired));
        for r in &s.rounds {
            assert!(r.contribution_exceeded, "round {}", r.round);
            assert_ne!(r.gate.kind, GateKind::Stop, "round {}", r.round);
            if r.round < max {
                assert_eq!(r.gate.kind, GateKind::Continue, "round {}", r.round);
            }
        }
    }
}
#[test]
fn test_dod1_val5_limit_is_strict_greater_than() {
    let _g = env();
    let (base, p) = pcts(100.0, 2, 5);
    assert_eq!(base.state, RunState::Completed);
    assert!(base.rounds.iter().all(|r| !r.contribution_exceeded));
    let m = p.iter().cloned().fold(0.0, f64::max);
    assert!(m > 0.0 && m <= 100.0, "{p:?}");
    // limit exactly equal to the largest added share: not exceeded, same outcome as without a limit
    let (s, _) = pcts(m, 2, 5);
    assert!(s.rounds.iter().all(|r| !r.contribution_exceeded), "{p:?} limit {m}");
    assert_eq!((s.state, s.rounds_done), (RunState::Completed, 3));
    // just below the largest share: that round is exceeded and cannot end the run
    let (s, _) = pcts(m - 0.05, 2, 5);
    assert!(s.rounds.iter().any(|r| r.contribution_exceeded));
    for r in s.rounds.iter().filter(|r| r.contribution_exceeded) {
        assert_ne!(r.gate.kind, GateKind::Stop, "round {}", r.round);
    }
    if s.state == RunState::Completed {
        assert!(!s.rounds.last().unwrap().contribution_exceeded);
    }
    // just below the share of round 3 (the round that would Stop): the run must go on past round 3
    let p3 = p[2];
    assert!(p3 >= 0.05);
    let (s, _) = pcts(p3 - 0.05, 2, 5);
    assert!(s.rounds[2].contribution_exceeded, "p3 {p3}");
    assert_eq!(s.rounds[2].gate.kind, GateKind::Continue);
    assert!(s.rounds_done >= 4, "rounds {}", s.rounds_done);
    // exactly the share of round 3: not exceeded, the run stops at round 3 as before
    let (s, _) = pcts(p3, 2, 5);
    assert!(!s.rounds[2].contribution_exceeded);
    assert_eq!(s.rounds[2].gate.kind, GateKind::Stop);
}

// ---------------------------------------------------------------------------------------------------------------
// VAL6 — human approval is only by `decide`
// ---------------------------------------------------------------------------------------------------------------
#[test]
fn test_dod1_val6_engine_never_sets_a_human_state() {
    let _g = env();
    let humans = [RunState::Approved, RunState::Rejected, RunState::Returned];
    for (min, max) in [(2u32, 5u32), (1, 1), (5, 5)] {
        let dir = tempfile::tempdir().unwrap();
        let c = cfg_in(dir.path(), min, max, 100.0);
        let states = Arc::new(Mutex::new(vec![]));
        let s2 = states.clone();
        let s = start(c, clients(), Arc::default(), move |e| {
            if let RunEvent::State { state, .. } = e {
                s2.lock().unwrap().push(state);
            }
        })
        .unwrap();
        assert!(!humans.contains(&s.state), "{:?}", s.state);
        assert!(s.decision.is_none() && s.decision_log.is_empty());
        for st in states.lock().unwrap().iter() {
            assert!(!humans.contains(st), "{st:?}");
        }
        let run = dir.path().join("run");
        assert_eq!(
            jsonf(run.join("output/final/human_decision.json")),
            Value::Null
        );
        assert!(jsonf(run.join("output/audit/run_manifest.json"))["human_decision"].is_null());
    }
}
#[test]
fn test_dod1_val6_decide_only_on_stopped_or_completed() {
    let _g = env();
    // Paused (by the pre-set pause flag): refused, nothing changes
    let dir = tempfile::tempdir().unwrap();
    let c = cfg(dir.path());
    let run = c.run_dir.clone();
    let control = Arc::new(RunControl::default());
    control.pause.store(true, Ordering::SeqCst);
    let s = start(c, clients(), control, |_| {}).unwrap();
    assert_eq!(s.state, RunState::Paused);
    for kind in [DecisionKind::Approved, DecisionKind::Rejected, DecisionKind::Returned] {
        assert!(decide(&run, kind, "Owner", "").is_err(), "{kind:?} on a Paused run");
    }
    let after = load(&run).unwrap();
    assert_eq!(after.state, RunState::Paused);
    assert!(after.decision.is_none() && after.decision_log.is_empty());
    assert_eq!(jsonf(run.join("output/final/human_decision.json")), Value::Null);
    // Running: refused
    mark_running_on_disk(&run);
    assert!(decide(&run, DecisionKind::Approved, "Owner", "").is_err());
    // Paused by an auth failure: refused too
    let dir = tempfile::tempdir().unwrap();
    let c = cfg(dir.path());
    let run2 = c.run_dir.clone();
    std::env::set_var("FAKE_CLI_SCENARIO", "auth_fail");
    let s = start(c, clients(), Arc::default(), |_| {}).unwrap();
    std::env::remove_var("FAKE_CLI_SCENARIO");
    assert_eq!(s.state, RunState::Paused);
    assert!(decide(&run2, DecisionKind::Approved, "Owner", "").is_err());
    // Completed and Stopped: accepted, and the state follows the kind
    for kind in [DecisionKind::Approved, DecisionKind::Rejected] {
        let dir = tempfile::tempdir().unwrap();
        let c = cfg(dir.path());
        let run = c.run_dir.clone();
        start(c, clients(), Arc::default(), |_| {}).unwrap();
        let s = decide(&run, kind, "Owner", "note").unwrap();
        assert_eq!(
            s.state,
            if kind == DecisionKind::Approved {
                RunState::Approved
            } else {
                RunState::Rejected
            }
        );
        // already decided: a later decision is accepted and replaces the state
        let s = decide(&run, DecisionKind::Rejected, "Owner", "changed mind").unwrap();
        assert_eq!(s.state, RunState::Rejected);
        assert_eq!(s.decision_log.len(), 2);
        assert_eq!(s.decision.unwrap().note, "changed mind");
    }
    let (_d, run, s) = stopped_in_review(2, 2, 5);
    assert_eq!(s.state, RunState::Stopped);
    assert_eq!(
        decide(&run, DecisionKind::Rejected, "Owner", "").unwrap().state,
        RunState::Rejected
    );
}

// ---------------------------------------------------------------------------------------------------------------
// VAL7 — Return (SUB3)
// ---------------------------------------------------------------------------------------------------------------
#[test]
fn test_dod1_val7_sub3_return_refused_at_the_runs_own_max_not_at_five() {
    let _g = env();
    for max in [1u32, 2, 4] {
        // min = max keeps the run going to its cap; for 1 and 2 the cap is a human-review stop, for 4 a completion
        let dir = tempfile::tempdir().unwrap();
        let c = cfg_in(dir.path(), max, max, 100.0);
        let run = c.run_dir.clone();
        let s = start(c, clients(), Arc::default(), |_| {}).unwrap();
        assert_eq!(s.rounds_done, max);
        let err = decide(&run, DecisionKind::Returned, "Owner", "").unwrap_err();
        assert!(
            err.to_string().contains("Maximum rounds reached"),
            "max {max}: {err}"
        );
        let after = load(&run).unwrap();
        assert_eq!(after.decision_log.len(), 0, "a refused Return leaves no trace");
        assert_eq!(after.state, s.state);
        // the other decisions stay open
        assert_eq!(
            decide(&run, DecisionKind::Rejected, "Owner", "").unwrap().state,
            RunState::Rejected
        );
    }
}
#[test]
fn test_dod1_val7_return_allowed_below_own_max_and_runs_exactly_the_granted_round() {
    let _g = env();
    // max 5, gate-completed after round 3: Return is allowed and leads to exactly round 4
    let dir = tempfile::tempdir().unwrap();
    let c = cfg(dir.path());
    let run = c.run_dir.clone();
    let s = start(c, clients(), Arc::default(), |_| {}).unwrap();
    assert_eq!(s.rounds_done, 3);
    let r = decide(&run, DecisionKind::Returned, "Owner", "more").unwrap();
    assert_eq!(r.state, RunState::Returned);
    assert_eq!(r.decision.as_ref().unwrap().revision, 3);
    let s = resume(&run, clients(), Arc::default(), |_| {}).unwrap();
    assert_eq!(s.rounds_done, 4);
    assert_eq!(s.state, RunState::Completed);
    // after round 4 (< 5) Return is still allowed; after round 5 it is not
    decide(&run, DecisionKind::Returned, "Owner", "more").unwrap();
    let s = resume(&run, clients(), Arc::default(), |_| {}).unwrap();
    assert_eq!(s.rounds_done, 5);
    assert!(decide(&run, DecisionKind::Returned, "Owner", "").is_err());
}
#[test]
fn test_dod1_val7_return_refused_when_stopped_mid_round_of_the_last_round() {
    let _g = env();
    for max in [1u32, 2] {
        let (_d, run, s) = stopped_in_review(max, max, max);
        assert_eq!((s.state, s.rounds_done), (RunState::Stopped, max - 1), "max {max}");
        assert!(run
            .join(format!("output/revisions/revision_{max:02}.md"))
            .is_file());
        let err = decide(&run, DecisionKind::Returned, "Owner", "").unwrap_err();
        assert!(err.to_string().contains("Maximum rounds reached"), "max {max}: {err}");
    }
}
#[test]
fn test_dod1_val7_return_after_mid_round_stop_keeps_revision_and_skips_the_author() {
    let _g = env();
    let (dir, run, s) = stopped_in_review(2, 2, 5);
    assert_eq!((s.state, s.rounds_done), (RunState::Stopped, 1));
    let rev2 = bytes(run.join("output/revisions/revision_02.md"));
    let rev2_docx = bytes(run.join("output/revisions/revision_02.docx"));
    decide(&run, DecisionKind::Returned, "Owner", "finish").unwrap();
    let log = dir.path().join("after.jsonl");
    std::env::set_var("FAKE_CLI_LOG", &log);
    let s = resume(&run, clients(), Arc::default(), |_| {}).unwrap();
    let calls = provider_calls(&log);
    assert_eq!(calls.len(), 1, "only the reviewer runs: {calls:?}");
    assert!(calls[0].iter().any(|a| a == "codex"), "{:?}", calls[0]);
    assert!(!calls[0].iter().any(|a| a == "claude"), "{:?}", calls[0]);
    assert_eq!(s.rounds_done, 2);
    assert_eq!(bytes(run.join("output/revisions/revision_02.md")), rev2);
    assert_eq!(bytes(run.join("output/revisions/revision_02.docx")), rev2_docx);
    assert!(run.join("output/reviews/review_02.json").is_file());
}

// ---------------------------------------------------------------------------------------------------------------
// VAL8 — final document is the last committed revision (SUB2, SUB5)
// ---------------------------------------------------------------------------------------------------------------
#[test]
fn test_dod1_val8_sub2_final_is_committed_revision_not_pending_nor_orphan() {
    let _g = env();
    let (_d, run, s) = stopped_in_review(2, 2, 5);
    assert_eq!(s.rounds_done, 1);
    let rev1 = bytes(run.join("output/revisions/revision_01.md"));
    let rev2 = bytes(run.join("output/revisions/revision_02.md"));
    assert_ne!(rev1, rev2, "fixture sanity");
    assert_eq!(bytes(run.join("output/final/final_document.md")), rev1);
    // an orphan, higher revision on disk never becomes final
    OutputPackage::reopen(&run, s.documents[0].clone())
        .write_revision(9, "Orphan revision nine")
        .unwrap();
    let a = decide(&run, DecisionKind::Approved, "Owner", "ok").unwrap();
    assert_eq!(a.rounds_done, 1);
    assert_eq!(bytes(run.join("output/final/final_document.md")), rev1);
    assert!(!text(run.join("output/final/final_document.md")).contains("Orphan"));
}
#[test]
fn test_dod1_val8_zero_rounds_final_is_the_original() {
    let _g = env();
    let (_d, run, s) = stopped_in_review(1, 1, 5);
    assert_eq!((s.state, s.rounds_done), (RunState::Stopped, 0));
    assert!(run.join("output/revisions/revision_01.md").is_file(), "pending revision exists");
    let fin = text(run.join("output/final/final_document.md"));
    assert_eq!(fin, s.documents[0]);
    assert_eq!(fin.trim(), DOC.trim());
    assert_ne!(fin, text(run.join("output/revisions/revision_01.md")));
    let a = decide(&run, DecisionKind::Approved, "Owner", "").unwrap();
    let d = a.decision.unwrap();
    assert_eq!((d.revision, d.after_round), (0, 0));
    assert_eq!(d.document_sha256, sha(fin.as_bytes()));
}
#[test]
fn test_dod1_val8_sub5_decision_is_bound_to_revision_and_hash() {
    let _g = env();
    let (_d, run1, _) = stopped_in_review(2, 2, 5);
    let a1 = decide(&run1, DecisionKind::Approved, "Owner", "n").unwrap().decision.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let c = cfg(dir.path());
    let run3 = c.run_dir.clone();
    let s3 = start(c, clients(), Arc::default(), |_| {}).unwrap();
    assert_eq!(s3.rounds_done, 3);
    let a3 = decide(&run3, DecisionKind::Approved, "Owner", "n").unwrap().decision.unwrap();
    assert_eq!((a1.revision, a3.revision), (1, 3));
    assert_eq!((a1.after_round, a3.after_round), (1, 3));
    let hex64 = Regex::new("^[0-9a-f]{64}$").unwrap();
    for (run, d) in [(&run1, &a1), (&run3, &a3)] {
        let fin = bytes(run.join("output/final/final_document.md"));
        assert_eq!(d.document_sha256, sha(&fin));
        assert!(hex64.is_match(&d.document_sha256));
        let m = jsonf(run.join("output/audit/run_manifest.json"));
        assert_eq!(m["human_decision"]["revision"], d.revision);
        assert_eq!(m["human_decision"]["document_sha256"], d.document_sha256.as_str());
        let hd = jsonf(run.join("output/final/human_decision.json"));
        assert_eq!(hd, m["human_decision"]);
        assert_eq!(hd["kind"], "approved");
        assert_eq!(hd["name"], "Owner");
    }
    assert_eq!(
        a3.document_sha256,
        sha(&bytes(run3.join("output/revisions/revision_03.md")))
    );
    assert_ne!(a1.document_sha256, a3.document_sha256);
}

// ---------------------------------------------------------------------------------------------------------------
// VAL9 — output tree
// ---------------------------------------------------------------------------------------------------------------
#[test]
fn test_dod1_val9_output_tree_exact() {
    let _g = env();
    let dir = tempfile::tempdir().unwrap();
    let c = cfg(dir.path());
    let run = c.run_dir.clone();
    let s = start(c, clients(), Arc::default(), |_| {}).unwrap();
    assert_eq!(s.rounds_done, 3);
    let out = run.join("output");
    let mut expected: Vec<String> = vec![
        "original/business.md".into(),
        "final/final_document.docx".into(),
        "final/final_document.md".into(),
        "final/final_review.html".into(),
        "final/unresolved_issues.json".into(),
        "final/human_decision.json".into(),
        "audit/run_manifest.json".into(),
        "audit/timeline.json".into(),
    ];
    for n in 1..=3 {
        for f in [
            format!("revisions/revision_{n:02}.docx"),
            format!("revisions/revision_{n:02}.md"),
            format!("reviews/review_{n:02}.json"),
            format!("reviews/author_response_{n:02}.json"),
            format!("audit/provider_output/round_{n:02}_author_claude_a01.jsonl"),
            format!("audit/provider_output/round_{n:02}_reviewer_codex_a01.jsonl"),
        ] {
            expected.push(f);
        }
    }
    let files = walk(&out);
    for e in &expected {
        assert!(files.contains(e), "missing {e}; have {files:?}");
    }
    let allowed = Regex::new(r"^audit/provider_output/round_0[1-5]_(author|reviewer)_(claude|codex)_a[0-9]{2}\.(jsonl|stderr\.txt)$").unwrap();
    for f in &files {
        assert!(
            expected.contains(f) || allowed.is_match(f),
            "unexpected file {f}"
        );
        assert!(!f.ends_with(".tmp") && !f.contains(".tmp"), "leftover temp file {f}");
    }
    // snapshot's own listing agrees
    for e in &expected {
        assert!(s.output_tree.contains(e), "output_tree lacks {e}");
    }
    // no revision beyond the rounds done
    assert!(!files.iter().any(|f| f.contains("revision_04") || f.contains("review_04")));
    // content
    assert_eq!(bytes(out.join("original/business.md")), DOC.as_bytes());
    assert_eq!(&bytes(out.join("revisions/revision_01.docx"))[..2], b"PK");
    assert_eq!(&bytes(out.join("final/final_document.docx"))[..2], b"PK");
    for n in 1..=3 {
        let md = text(out.join(format!("revisions/revision_{n:02}.md")));
        assert!(md.contains(&format!("<!-- revision {n} -->")), "NN mapping, revision {n}");
        let rv: Value = jsonf(out.join(format!("reviews/review_{n:02}.json")));
        let schema = serde_json::from_str(include_str!("../../../schemas/review.schema.json")).unwrap();
        jsonschema::validator_for(&schema).unwrap().validate(&rv).unwrap();
        assert!(jsonf(out.join(format!("reviews/author_response_{n:02}.json")))["responses"].is_array());
    }
    assert_eq!(bytes(out.join("final/final_document.md")), bytes(out.join("revisions/revision_03.md")));
    let tl = jsonf(out.join("audit/timeline.json"));
    assert!(tl.as_array().unwrap().len() >= 6);
    // human_decision.json is null until the human decides
    assert_eq!(jsonf(out.join("final/human_decision.json")), Value::Null);
    // unresolved issues: the two mediums still open after round 3
    let un = jsonf(out.join("final/unresolved_issues.json")).to_string();
    assert!(un.contains("R01-005") && un.contains("R01-006"), "{un}");
    for gone in ["R01-001", "R01-002", "R01-003", "R01-004"] {
        assert!(!un.contains(gone), "{gone} was resolved: {un}");
    }
    decide(&run, DecisionKind::Approved, "Owner", "ok").unwrap();
    assert_eq!(jsonf(out.join("final/human_decision.json"))["kind"], "approved");
}

// ---------------------------------------------------------------------------------------------------------------
// VAL10 — manifest
// ---------------------------------------------------------------------------------------------------------------
#[test]
fn test_dod1_val10_manifest_contents_and_schema() {
    let _g = env();
    let dir = tempfile::tempdir().unwrap();
    let mut c = cfg(dir.path());
    // Cyrillic name + content (bytes != chars), CRLF supporting file
    let primary = dir.path().join("бизнес.md");
    let body = "# Отчёт\r\nСтоимость 4,2 млн. Рост 18 %.";
    fs::write(&primary, body).unwrap();
    let support = dir.path().join("notes.txt");
    let sup_body = "line one\r\nline two\r\n";
    fs::write(&support, sup_body).unwrap();
    c.primary = primary;
    c.supporting = vec![support];
    c.policy.contribution = Contribution::Improve;
    c.policy.new_content_limit_pct = 77.5;
    c.criteria = vec!["Первый критерий".into(), "Second".into()];
    let run = c.run_dir.clone();
    let s = start(c, clients(), Arc::default(), |_| {}).unwrap();
    let m = jsonf(run.join("output/audit/run_manifest.json"));
    let schema = serde_json::from_str(include_str!("../../../schemas/run-manifest.schema.json")).unwrap();
    jsonschema::validator_for(&schema).unwrap().validate(&m).unwrap();
    let ts = Regex::new(r"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ$").unwrap();
    assert_eq!(m["schema_version"], 1);
    assert_eq!(m["run_id"], "test-run");
    assert_eq!(m["name"], "Northwind");
    assert!(ts.is_match(m["created_at"].as_str().unwrap()));
    assert!(ts.is_match(m["updated_at"].as_str().unwrap()));
    assert!(m["updated_at"].as_str().unwrap() >= m["created_at"].as_str().unwrap());
    assert!(!m["os"].as_str().unwrap().is_empty());
    assert!(!m["app_version"].as_str().unwrap().is_empty());
    assert_eq!(m["author"]["provider"], "claude");
    assert_eq!(m["reviewer"]["provider"], "codex");
    assert!(m["author"]["cli_version"].is_string() && m["reviewer"]["cli_version"].is_string(), "{m}");
    assert_eq!(m["author"]["model"], "claude-fake-1");
    assert_eq!(m["objective"], "Improve for executive review");
    assert_eq!(m["criteria"], json!(["Первый критерий", "Second"]));
    assert_eq!(m["policy"]["max_rounds"], 5);
    assert_eq!(m["policy"]["min_rounds"], 2);
    assert_eq!(m["policy"]["contribution"], "improve");
    assert_eq!(m["policy"]["new_content_limit_pct"], 77.5);
    assert_eq!(m["policy"]["stop"], json!({"no_critical":true,"no_high":true,"no_new":true,"max_rounds":true}));
    assert_eq!(m["rounds_completed"], s.rounds_done);
    assert_eq!(m["state"], state_str(s.state));
    assert_eq!(m["stop_reason"], serde_json::to_value(s.stop_reason).unwrap());
    assert!(m["human_decision"].is_null());
    let inputs = m["inputs"].as_array().unwrap();
    assert_eq!(inputs.len(), 2);
    assert_eq!(inputs[0]["file_name"], "бизнес.md");
    assert_eq!(inputs[0]["role"], "primary");
    assert_eq!(inputs[0]["bytes"], body.len());
    assert_eq!(inputs[0]["sha256"], sha(body.as_bytes()));
    assert!(inputs[0]["extracted_chars"].as_u64().unwrap() > 0);
    assert!(inputs[0]["extracted_chars"].as_u64().unwrap() <= body.chars().count() as u64 + 2);
    assert_eq!(inputs[1]["file_name"], "notes.txt");
    assert_eq!(inputs[1]["role"], "supporting");
    assert_eq!(inputs[1]["bytes"], sup_body.len());
    assert_eq!(inputs[1]["sha256"], sha(sup_body.as_bytes()));
    // the primary is copied byte for byte
    assert_eq!(bytes(run.join("output/original/бизнес.md")), body.as_bytes());
    // after the decision the manifest is rewritten with it
    decide(&run, DecisionKind::Rejected, "Имя", "заметка").unwrap();
    let m = jsonf(run.join("output/audit/run_manifest.json"));
    jsonschema::validator_for(&schema).unwrap().validate(&m).unwrap();
    assert_eq!(m["state"], "rejected");
    assert_eq!(m["human_decision"]["kind"], "rejected");
    assert_eq!(m["human_decision"]["name"], "Имя");
    assert_eq!(m["human_decision"]["note"], "заметка");
}

#[test]
fn test_dod1_val10_manifest_keeps_the_real_cli_version() {
    // the fake CLIs print "2.1.309 (Claude Code)" and "codex-cli 0.165.0"; the manifest must record the version
    let _g = env();
    let dir = tempfile::tempdir().unwrap();
    let c = cfg(dir.path());
    let run = c.run_dir.clone();
    start(c, clients(), Arc::default(), |_| {}).unwrap();
    let m = jsonf(run.join("output/audit/run_manifest.json"));
    let a = m["author"]["cli_version"].as_str().unwrap_or("");
    let r = m["reviewer"]["cli_version"].as_str().unwrap_or("");
    assert!(a.contains("2.1.309"), "author cli_version: {a:?}");
    assert!(r.contains("0.165.0"), "reviewer cli_version: {r:?}");
    assert!(!a.contains("REDACTED") && !r.contains("REDACTED"));
}

// ---------------------------------------------------------------------------------------------------------------
// VAL11 — pause / stop / crash
// ---------------------------------------------------------------------------------------------------------------
#[test]
fn test_dod1_val11_pause_takes_effect_before_the_next_provider_call() {
    let _g = env();
    let dir = tempfile::tempdir().unwrap();
    let c = cfg(dir.path());
    let run = c.run_dir.clone();
    let log = dir.path().join("calls.jsonl");
    std::env::set_var("FAKE_CLI_LOG", &log);
    // pre-set: no call at all
    let control = Arc::new(RunControl::default());
    control.pause.store(true, Ordering::SeqCst);
    let s = start(c, clients(), control, |_| {}).unwrap();
    assert_eq!((s.state, s.rounds_done), (RunState::Paused, 0));
    assert!(provider_calls(&log).is_empty());
    // resume, pause again during the author's call: the author finishes, the reviewer is not called
    let control = Arc::new(RunControl::default());
    let c2 = control.clone();
    let s = resume(&run, clients(), control, move |e| {
        if let RunEvent::Line { role: Role::Author, line, .. } = e {
            if line.readable.as_deref() == Some("Completed") {
                c2.pause.store(true, Ordering::SeqCst);
            }
        }
    })
    .unwrap();
    assert_eq!(s.state, RunState::Paused);
    assert_eq!(provider_calls(&log).len(), 1, "author only");
    assert!(run.join("output/revisions/revision_01.docx").is_file());
    assert!(!run.join("output/reviews/review_01.json").exists());
    let s = resume(&run, clients(), Arc::default(), |_| {}).unwrap();
    assert_eq!((s.state, s.rounds_done), (RunState::Completed, 3));
    assert_eq!(provider_calls(&log).len(), 1 + 5, "no repeated author call: 1 + 5 more calls");
}
#[test]
fn test_dod1_val11_stop_kills_the_active_call_at_once() {
    let _g = env();
    let dir = tempfile::tempdir().unwrap();
    let mut c = cfg(dir.path());
    c.call_timeout = Duration::from_secs(60);
    let run = c.run_dir.clone();
    let baseline = live_fake_clis();
    std::env::set_var("FAKE_CLI_SCENARIO", "slow"); // 30 s sleep in the child
    let control = Arc::new(RunControl::default());
    let c2 = control.clone();
    let t0 = Instant::now();
    let s = start(c, clients(), control, move |e| {
        if let RunEvent::Phase { role: Role::Author, round: 1, .. } = e {
            let c3 = c2.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(400));
                c3.stop();
            });
        }
    })
    .unwrap();
    assert!(t0.elapsed() < Duration::from_secs(4), "{:?}", t0.elapsed());
    assert_eq!(s.state, RunState::Stopped);
    assert_eq!(s.stop_reason, Some(StopReason::UserStop));
    assert_eq!(s.rounds_done, 0);
    std::thread::sleep(Duration::from_millis(500));
    assert!(live_fake_clis() <= baseline, "the stopped child must be killed");
    let m = jsonf(run.join("output/audit/run_manifest.json"));
    assert_eq!(m["stop_reason"], "user_stop");
    assert_eq!(m["state"], "stopped");
    // stop before the first call
    let dir = tempfile::tempdir().unwrap();
    let c = cfg(dir.path());
    let log = dir.path().join("calls.jsonl");
    std::env::set_var("FAKE_CLI_LOG", &log);
    let control = Arc::new(RunControl::default());
    control.stop();
    let s = start(c, clients(), control, |_| {}).unwrap();
    assert_eq!((s.state, s.stop_reason), (RunState::Stopped, Some(StopReason::UserStop)));
    assert!(provider_calls(&log).is_empty());
}
#[test]
fn test_dod1_val11_stop_keeps_completed_rounds_byte_for_byte() {
    let _g = env();
    let dir = tempfile::tempdir().unwrap();
    let c = cfg(dir.path());
    let run = c.run_dir.clone();
    let control = Arc::new(RunControl::default());
    let c2 = control.clone();
    let before: Arc<Mutex<BTreeMap<String, Vec<u8>>>> = Arc::default();
    let b2 = before.clone();
    let run2 = run.clone();
    let s = start(c, clients(), control, move |e| match e {
        RunEvent::RoundCompleted { round: 1, .. } => {
            let mut m = b2.lock().unwrap();
            for f in ["revisions/revision_01.md", "revisions/revision_01.docx", "reviews/review_01.json", "reviews/author_response_01.json"] {
                m.insert(f.into(), fs::read(run2.join("output").join(f)).unwrap());
            }
            std::env::set_var("FAKE_CLI_SCENARIO", "slow");
        }
        RunEvent::Phase { round: 2, role: Role::Author, .. } => {
            let c3 = c2.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(250));
                c3.stop();
            });
        }
        _ => (),
    })
    .unwrap();
    assert_eq!((s.state, s.rounds_done), (RunState::Stopped, 1));
    for (f, b) in before.lock().unwrap().iter() {
        assert_eq!(&bytes(run.join("output").join(f)), b, "{f}");
    }
    let m = jsonf(run.join("output/audit/run_manifest.json"));
    assert_eq!((m["state"].as_str(), m["stop_reason"].as_str()), (Some("stopped"), Some("user_stop")));
    assert_eq!(m["rounds_completed"], 1);
}
#[test]
fn test_dod1_val11_stop_on_a_paused_run_without_worker() {
    let _g = env();
    let dir = tempfile::tempdir().unwrap();
    let c = cfg(dir.path());
    let run = c.run_dir.clone();
    let control = Arc::new(RunControl::default());
    let c2 = control.clone();
    let s = start(c, clients(), control, move |e| {
        if let RunEvent::RoundCompleted { round: 1, .. } = e {
            c2.pause.store(true, Ordering::SeqCst);
        }
    })
    .unwrap();
    assert_eq!((s.state, s.rounds_done), (RunState::Paused, 1));
    let st = stop_idle(&run).unwrap();
    assert_eq!(st.state, RunState::Stopped);
    assert_eq!(st.stop_reason, Some(StopReason::UserStop));
    assert_eq!(st.rounds_done, 1);
    assert_eq!(load(&run).unwrap().state, RunState::Stopped);
    assert_eq!(jsonf(run.join("output/audit/run_manifest.json"))["stop_reason"], "user_stop");
    assert!(run.join("output/reviews/review_01.json").is_file());
    // a stopped / completed run cannot be idle-stopped again
    assert!(stop_idle(&run).is_err());
}
#[test]
fn test_dod1_val11_crash_recovery_reports_paused_interrupted_and_keeps_rounds() {
    let _g = env();
    let dir = tempfile::tempdir().unwrap();
    let c = cfg(dir.path());
    let run = c.run_dir.clone();
    let control = Arc::new(RunControl::default());
    let c2 = control.clone();
    start(c, clients(), control, move |e| {
        if let RunEvent::RoundCompleted { round: 2, .. } = e {
            c2.pause.store(true, Ordering::SeqCst);
        }
    })
    .unwrap();
    let keep = bytes(run.join("output/reviews/review_02.json"));
    let keep_rev = bytes(run.join("output/revisions/revision_02.docx"));
    mark_running_on_disk(&run);
    let s = load(&run).unwrap();
    assert_eq!(s.state, RunState::Paused);
    assert_eq!(s.rounds_done, 2);
    let e = s.last_error.expect("last_error");
    assert_eq!(e.message, "interrupted");
    // persisted, not only reported
    assert_eq!(jsonf(run.join("run_state.json"))["snapshot"]["state"], "paused");
    assert_eq!(load(&run).unwrap().state, RunState::Paused);
    assert_eq!(bytes(run.join("output/reviews/review_02.json")), keep);
    assert_eq!(bytes(run.join("output/revisions/revision_02.docx")), keep_rev);
    let s = resume(&run, clients(), Arc::default(), |_| {}).unwrap();
    assert!(s.rounds_done >= 3);
    assert!(s.last_error.is_none());
    assert_eq!(bytes(run.join("output/reviews/review_02.json")), keep);
}

// ---------------------------------------------------------------------------------------------------------------
// VAL12 — provider failures
// ---------------------------------------------------------------------------------------------------------------
fn failing_run(scenario: &str, timeout_ms: u64) -> (tempfile::TempDir, PathBuf, RunSnapshot, Duration) {
    let dir = tempfile::tempdir().unwrap();
    let mut c = cfg(dir.path());
    c.call_timeout = Duration::from_millis(timeout_ms);
    let run = c.run_dir.clone();
    std::env::set_var("FAKE_CLI_SCENARIO", scenario);
    let t0 = Instant::now();
    let s = start(c, clients(), Arc::default(), |_| {}).unwrap();
    let el = t0.elapsed();
    std::env::remove_var("FAKE_CLI_SCENARIO");
    (dir, run, s, el)
}
fn assert_paused_with(run: &Path, s: &RunSnapshot, kind: &str) {
    assert_eq!(s.state, RunState::Paused, "{kind}");
    let e = s.last_error.as_ref().unwrap_or_else(|| panic!("{kind}: no last_error"));
    assert_eq!(e.kind, kind);
    assert!(!e.message.is_empty());
    let tl = jsonf(run.join("output/audit/timeline.json"));
    assert!(
        tl.as_array().unwrap().iter().any(|ev| ev["kind"] == "provider_error"
            && ev["detail"].as_str().is_some_and(|d| !d.is_empty())),
        "{kind}: timeline lacks provider_error: {tl}"
    );
    assert_eq!(load(run).unwrap().state, RunState::Paused);
    assert_eq!(jsonf(run.join("run_state.json"))["snapshot"]["state"], "paused");
}
#[test]
fn test_dod1_val12_auth_malformed_timeout_pause_with_kind() {
    let _g = env();
    let (_d, run, s, _) = failing_run("auth_fail", 5000);
    assert_paused_with(&run, &s, "auth");
    let (_d, run, s, _) = failing_run("malformed", 5000);
    assert_paused_with(&run, &s, "malformed");
    // test-input change (orchestrator 00:07Z): old budget 400 ms, bound 5 s; new budget 2000 ms, bound 10 s;
    // reason: Windows spawn ~229 ms measured; the assertions are unchanged
    let (_d, run, s, el) = failing_run("slow", 2000);
    assert_paused_with(&run, &s, "timeout");
    assert!(el < Duration::from_secs(10), "{el:?}");
    // retry after the cause is gone completes the run
    let s = resume(&run, clients(), Arc::default(), |_| {}).unwrap();
    assert_eq!((s.state, s.rounds_done), (RunState::Completed, 3));
    assert!(s.last_error.is_none());
}
#[test]
fn test_dod1_val12_failure_in_the_reviewer_keeps_the_written_revision() {
    let _g = env();
    for (scenario, kind) in [("auth_fail", "auth"), ("malformed", "malformed")] {
        let dir = tempfile::tempdir().unwrap();
        let c = cfg(dir.path());
        let run = c.run_dir.clone();
        let hook: Hook = Arc::new(move |req: &CallRequest| {
            if req.role == Role::Reviewer {
                std::env::set_var("FAKE_CLI_SCENARIO", scenario);
            } else {
                std::env::remove_var("FAKE_CLI_SCENARIO");
            }
        });
        let seen: Seen = Arc::default();
        let s = start(c, rec_clients(Provider::Claude, &seen, Some(hook)), Arc::default(), |_| {}).unwrap();
        std::env::remove_var("FAKE_CLI_SCENARIO");
        assert_paused_with(&run, &s, kind);
        assert_eq!(s.rounds_done, 0);
        assert!(run.join("output/revisions/revision_01.docx").is_file());
        assert!(!run.join("output/reviews/review_01.json").exists());
    }
}
#[test]
fn test_dod1_val12_oversized_input_is_input_too_large() {
    let _g = env();
    let dir = tempfile::tempdir().unwrap();
    let c = cfg(dir.path());
    let run = c.run_dir.clone();
    // two copies of the document go into every prompt: 2 x 5 MB is over the 9 MiB prompt limit
    fs::write(&c.primary, "word ".repeat(1_000_000)).unwrap();
    let s = start(c, clients(), Arc::default(), |_| {}).expect("an oversized input pauses the run, it does not abort start");
    assert_paused_with(&run, &s, "input_too_large");
    assert_eq!(s.rounds_done, 0);
}

// ---------------------------------------------------------------------------------------------------------------
// VAL13 — workspace isolation (SUB4)
// ---------------------------------------------------------------------------------------------------------------
#[test]
fn test_dod1_val13_sub4_workspaces_and_reviewer_prompt_hide_the_author() {
    let _g = env();
    std::env::set_var("FAKE_CLI_SCENARIO", "human_question");
    let dir = tempfile::tempdir().unwrap();
    let c = cfg(dir.path());
    let run = c.run_dir.clone();
    let seen: Seen = Arc::default();
    let s = start(c, rec_clients(Provider::Claude, &seen, None), Arc::default(), |_| {}).unwrap();
    std::env::remove_var("FAKE_CLI_SCENARIO");
    assert!(s.rounds_done >= 2, "{:?} {:?}", s.state, s.last_error);
    let aw =fs::canonicalize(run.join("workspaces/author")).unwrap();
    let rw = fs::canonicalize(run.join("workspaces/reviewer")).unwrap();
    assert_ne!(aw, rw);
    let mut author_secrets: Vec<String> = vec!["Who owns the capital budget?".into()];
    for n in 1..=4 {
        let a: AuthorResponse = serde_json::from_str(
            &fs::read_to_string(format!(
                "{}/fixtures/fake/author-{n}.json",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap(),
        )
        .unwrap();
        author_secrets.extend(a.changes_made);
        author_secrets.extend(a.responses.into_iter().map(|r| r.explanation));
    }
    author_secrets.retain(|s| !s.trim().is_empty());
    assert!(author_secrets.len() >= 5);
    let seen = seen.lock().unwrap();
    let (mut na, mut nr) = (0, 0);
    for (role, round, prompt, workdir) in seen.iter() {
        let wd = fs::canonicalize(workdir).unwrap();
        match role {
            Role::Author => {
                na += 1;
                assert_eq!(wd, aw, "author call of round {round}");
            }
            Role::Reviewer => {
                nr += 1;
                assert_eq!(wd, rw, "reviewer call of round {round}");
                assert!(prompt.contains(DOC.trim()), "reviewer must get the original, round {round}");
                let cur = text(run.join(format!("output/revisions/revision_{round:02}.md")));
                assert!(
                    prompt.contains(cur.trim()),
                    "reviewer must get the current revision {round}"
                );
                assert!(prompt.contains(&format!("ROUND: {round}")));
                for secret in &author_secrets {
                    // the fake author may copy its change note into the document itself; that is the document, not a leak
                    if cur.contains(secret.as_str()) {
                        continue;
                    }
                    assert!(!prompt.contains(secret.as_str()), "author text leaked to the reviewer in round {round}: {secret}");
                }
            }
        }
    }
    assert!(na >= 2 && nr >= 2, "author calls {na}, reviewer calls {nr}");
}

// ---------------------------------------------------------------------------------------------------------------
// VAL14 — relocatable run folder
// ---------------------------------------------------------------------------------------------------------------
#[test]
fn test_dod1_val14_run_folder_is_relocatable() {
    let _g = env();
    let dir = tempfile::tempdir().unwrap();
    let c = cfg(dir.path());
    let old = c.run_dir.clone();
    let source = c.primary.clone();
    let control = Arc::new(RunControl::default());
    let c2 = control.clone();
    let s = start(c, clients(), control, move |e| {
        if let RunEvent::RoundCompleted { round: 1, .. } = e {
            c2.pause.store(true, Ordering::SeqCst);
        }
    })
    .unwrap();
    assert_eq!((s.state, s.rounds_done), (RunState::Paused, 1));
    let moved = dir.path().join("elsewhere").join("moved-run");
    fs::create_dir_all(moved.parent().unwrap()).unwrap();
    fs::rename(&old, &moved).unwrap();
    fs::remove_file(&source).unwrap(); // nothing may reach back to the original source path
    assert!(!old.exists());
    let l = load(&moved).unwrap();
    assert_eq!((l.state, l.rounds_done), (RunState::Paused, 1));
    let s = resume(&moved, clients(), Arc::default(), |_| {}).unwrap();
    assert_eq!((s.state, s.rounds_done), (RunState::Completed, 3));
    assert!(!old.exists(), "nothing may be written to the old place");
    assert!(moved.join("output/revisions/revision_03.md").is_file());
    assert!(moved.join("output/reviews/review_03.json").is_file());
    let old_s = old.to_string_lossy().to_string();
    for f in ["run_state.json", "output/audit/run_manifest.json", "output/audit/timeline.json"] {
        assert!(!text(moved.join(f)).contains(&old_s), "{f} still names the old folder");
    }
    decide(&moved, DecisionKind::Approved, "Owner", "").unwrap();
    assert!(!old.exists());
}

// ---------------------------------------------------------------------------------------------------------------
// VAL15 — no secrets persisted or streamed
// ---------------------------------------------------------------------------------------------------------------
const SECRET_FRAGMENTS: [&str; 12] = [
    "sk-testSyntheticKey",
    "sk-ant-testSyntheticKey",
    "syntheticBearerToken",
    "eyJhbGciOiJIUzI1NiJ9",
    "eyJzdWIiOiIxMjM0In0",
    "syntheticSignature",
    "shortApiSecret",
    "shortAccessSecret",
    "shortRefreshSecret",
    "shortPassword",
    "ABCDEF0123456789ABCDEF0123456789",
    "QWxhZGRpbjpvcGVuIHNlc2FtZVN5bnRoZXRpYw==",
];
fn assert_no_secret(label: &str, s: &str) {
    for f in SECRET_FRAGMENTS {
        assert!(!s.contains(f), "{label} leaks {f}");
    }
}
#[test]
fn test_dod1_val15_secrets_do_not_reach_disk_or_events() {
    let _g = env();
    for (author, role_with_secrets) in [
        (Provider::Claude, Role::Author),
        (Provider::Claude, Role::Reviewer),
        (Provider::Codex, Role::Author),
        (Provider::Codex, Role::Reviewer),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut c = cfg(dir.path());
        c.author = author;
        let run = c.run_dir.clone();
        let hook: Hook = Arc::new(move |req: &CallRequest| {
            if req.role == role_with_secrets {
                std::env::set_var("FAKE_CLI_SCENARIO", "secrets");
            } else {
                std::env::remove_var("FAKE_CLI_SCENARIO");
            }
        });
        let seen: Seen = Arc::default();
        let events: Arc<Mutex<Vec<String>>> = Arc::default();
        let e2 = events.clone();
        let s = start(c, rec_clients(author, &seen, Some(hook)), Arc::default(), move |e| {
            e2.lock().unwrap().push(format!("{e:?}"));
        })
        .unwrap();
        std::env::remove_var("FAKE_CLI_SCENARIO");
        let label = format!("{author:?}/{role_with_secrets:?}");
        assert_eq!(s.state, RunState::Paused, "{label}");
        assert!(!events.lock().unwrap().is_empty());
        for ev in events.lock().unwrap().iter() {
            assert_no_secret(&format!("{label} event stream"), ev);
        }
        assert_no_secret(&format!("{label} snapshot"), &serde_json::to_string(&s).unwrap());
        assert_no_secret(&format!("{label} load()"), &serde_json::to_string(&load(&run).unwrap()).unwrap());
        let mut scanned = 0;
        for f in walk(&run) {
            if f.starts_with("inputs/") || f.starts_with("output/original/") {
                continue;
            }
            let b = bytes(run.join(&f));
            assert_no_secret(&format!("{label} {f}"), &String::from_utf8_lossy(&b));
            scanned += 1;
        }
        assert!(scanned >= 4);
        let role = if role_with_secrets == Role::Author { "author" } else { "reviewer" };
        let prefix = format!("output/audit/provider_output/round_01_{role}_");
        assert!(
            walk(&run).iter().any(|f| f.starts_with(&prefix)),
            "{label}: the failed call's evidence is kept"
        );
    }
}
#[test]
fn test_dod1_val15_redact_masks_every_shape_and_keeps_prose() {
    let cases = [
        "sk-testSyntheticKey",
        "sk-ant-testSyntheticKey",
        "Bearer syntheticBearerToken",
        "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0In0.syntheticSignature",
        r#"{"api_key":"shortApiSecret"}"#,
        r#"{"access_token": "shortAccessSecret"}"#,
        r#"{"refresh_token":"shortRefreshSecret"}"#,
        r#"{"password":"shortPassword"}"#,
        r#"{\"api_key\":\"shortApiSecret\",\"password\":\"shortPassword\"}"#,
        "ABCDEF0123456789ABCDEF0123456789",
        "token=QWxhZGRpbjpvcGVuIHNlc2FtZVN5bnRoZXRpYw==",
    ];
    for c in cases {
        let r = redact(&format!("before {c} after"));
        assert_no_secret(c, &r);
        assert!(r.starts_with("before "), "{r}");
        assert_eq!(redact(&r), r, "idempotent for {c}");
    }
    for len in [32usize, 33, 64] {
        let tok = "aB3".repeat(len).chars().take(len).collect::<String>();
        assert!(!redact(&format!("x {tok} y")).contains(&tok), "{len}-char token");
    }
    for prose in [
        "Capital cost $4.2M. Growth 18 %.",
        "Стоимость 4,2 млн. — «рост» 18 %.\r\nСтрока два.",
        "",
    ] {
        assert_eq!(redact(prose), prose);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// DoD3 — concurrency stress
// ---------------------------------------------------------------------------------------------------------------
struct Rng(Mutex<u64>);
impl Rng {
    fn new(i: u64) -> Self {
        Rng(Mutex::new(0x9E37_79B9_7F4A_7C15 ^ (i + 1).wrapping_mul(0x1000_0000_01B3)))
    }
    fn next(&self) -> u64 {
        let mut g = self.0.lock().unwrap();
        let mut x = *g;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *g = x;
        x
    }
}
fn check_invariants(
    label: &str,
    run: &Path,
    snap: &RunSnapshot,
    seen: &mut BTreeMap<String, Vec<u8>>,
) {
    assert!(
        matches!(snap.state, RunState::Paused | RunState::Stopped | RunState::Completed),
        "{label}: final state {:?}",
        snap.state
    );
    let loaded = load(run).unwrap_or_else(|e| panic!("{label}: load failed: {e}"));
    assert_eq!(loaded.state, snap.state, "{label}: load() vs returned state");
    assert_eq!(loaded.rounds_done, snap.rounds_done, "{label}");
    let disk = serde_json::from_slice::<Value>(&bytes(run.join("run_state.json")))
        .unwrap_or_else(|e| panic!("{label}: run_state.json is not valid JSON: {e}"));
    assert_eq!(disk["snapshot"]["state"], state_str(loaded.state), "{label}: disk vs load()");
    assert_eq!(disk["snapshot"]["roundsDone"], loaded.rounds_done, "{label}");
    for n in 1..=snap.rounds_done {
        for f in [
            format!("revisions/revision_{n:02}.docx"),
            format!("revisions/revision_{n:02}.md"),
            format!("reviews/review_{n:02}.json"),
            format!("reviews/author_response_{n:02}.json"),
        ] {
            let p = run.join("output").join(&f);
            assert!(p.is_file(), "{label}: completed round file lost: {f}");
            let b = bytes(&p);
            match seen.get(&f) {
                Some(old) => assert_eq!(old, &b, "{label}: completed round file changed: {f}"),
                None => {
                    seen.insert(f, b);
                }
            }
        }
    }
    for f in ["output/audit/timeline.json", "output/audit/run_manifest.json"] {
        let p = run.join(f);
        if p.exists() {
            serde_json::from_slice::<Value>(&bytes(&p))
                .unwrap_or_else(|e| panic!("{label}: {f} is torn: {e}"));
        }
    }
    if snap.state != RunState::Paused {
        let m = jsonf(run.join("output/audit/run_manifest.json"));
        assert_eq!(m["state"], state_str(snap.state), "{label}: manifest state");
        assert_eq!(m["rounds_completed"], snap.rounds_done, "{label}: manifest rounds");
        assert!(run.join("output/final/final_document.md").is_file(), "{label}");
    }
}
#[test]
fn test_dod3_concurrency_stress_pause_stop_resume_with_injected_failures() {
    let _g = env();
    let mut outcomes: BTreeMap<String, u32> = BTreeMap::new();
    for it in 0..60u64 {
        let dir = tempfile::tempdir().unwrap();
        let mut c = cfg_in(dir.path(), 1 + (it % 3) as u32, 3 + (it % 3) as u32, 100.0);
        c.call_timeout = Duration::from_millis(1500);
        let run = c.run_dir.clone();
        let rng = Arc::new(Rng::new(it));
        let r2 = rng.clone();
        let hook: Hook = Arc::new(move |_req: &CallRequest| {
            // only the worker thread touches the environment
            match r2.next() % 100 {
                0..=9 => std::env::set_var("FAKE_CLI_SCENARIO", "auth_fail"),
                10..=16 => std::env::set_var("FAKE_CLI_SCENARIO", "malformed"),
                17..=22 => std::env::set_var("FAKE_CLI_SCENARIO", "crash"),
                23..=27 => std::env::set_var("FAKE_CLI_SCENARIO", "slow"),
                _ => std::env::remove_var("FAKE_CLI_SCENARIO"),
            }
        });
        let seen: Seen = Arc::default();
        let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for cycle in 0..5u32 {
            let label = format!("iteration {it} cycle {cycle}");
            let control = Arc::new(RunControl::default());
            let (delay, action) = (rng.next() % 300, rng.next() % 5);
            let chaos = {
                let c = control.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(delay));
                    match action {
                        1 => c.pause.store(true, Ordering::SeqCst),
                        2 => c.stop(),
                        3 => {
                            c.pause.store(true, Ordering::SeqCst);
                            std::thread::sleep(Duration::from_millis(20));
                            c.stop();
                        }
                        4 => {
                            c.stop();
                            c.pause.store(true, Ordering::SeqCst);
                        }
                        _ => {}
                    }
                })
            };
            let pair = rec_clients(Provider::Claude, &seen, Some(hook.clone()));
            let (cfg2, run2, c3) = (c.clone(), run.clone(), control.clone());
            let worker = std::thread::spawn(move || -> std::result::Result<RunSnapshot, String> {
                let r = if cycle == 0 {
                    start(cfg2, pair, c3, |_| {})
                } else {
                    resume(&run2, pair, c3, |_| {})
                };
                r.map_err(|e| e.to_string())
            });
            let joined = worker.join();
            chaos.join().expect("chaos thread");
            let snap = match joined {
                Err(_) => panic!("{label}: the engine panicked"),
                Ok(Err(e)) => panic!("{label}: engine error: {e}"),
                Ok(Ok(s)) => s,
            };
            std::env::remove_var("FAKE_CLI_SCENARIO");
            check_invariants(&label, &run, &snap, &mut files);
            *outcomes.entry(format!("{:?}", snap.state)).or_default() += 1;
            if snap.state != RunState::Paused {
                break;
            }
            if rng.next().is_multiple_of(4) {
                let st = stop_idle(&run).unwrap_or_else(|e| panic!("{label}: stop_idle: {e}"));
                assert_eq!(st.state, RunState::Stopped, "{label}");
                assert_eq!(st.stop_reason, Some(StopReason::UserStop), "{label}");
                check_invariants(&format!("{label} idle"), &run, &st, &mut files);
                break;
            }
        }
    }
    eprintln!("stress outcomes: {outcomes:?}");
    assert!(outcomes.contains_key("Paused") && outcomes.len() >= 2, "{outcomes:?}");
}
