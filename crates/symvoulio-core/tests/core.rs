// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
use symvoulio_core::*;
fn issue(id: &str, severity: Severity) -> Issue {
    Issue {
        issue_id: id.into(),
        severity,
        criterion: "Criterion".into(),
        finding: "Finding".into(),
        evidence: "Source".into(),
        proposed_action: "Fix".into(),
        human_decision_required: false,
        status: Status::Open,
    }
}
fn review(issues: Vec<Issue>) -> Review {
    Review {
        summary: "Review".into(),
        issues,
        criteria_results: vec![],
    }
}
fn ledger(severity: Severity) -> IssueLedger {
    let mut l = IssueLedger::default();
    l.apply_review(1, &review(vec![issue("R01-001", severity)]));
    l
}
#[test]
fn continue_open_precedes_minimum() {
    assert_eq!(
        gate_after(1, &Policy::default(), &ledger(Severity::Critical)).reason,
        GateReason::ContinueOpen
    );
}
#[test]
fn continue_minimum() {
    assert_eq!(
        gate_after(1, &Policy::default(), &IssueLedger::default()).reason,
        GateReason::ContinueMin
    );
}
#[test]
fn continue_new() {
    assert_eq!(
        gate_after(2, &Policy::default(), &ledger(Severity::Medium)).reason,
        GateReason::ContinueNew
    );
}
#[test]
fn stop_no_new() {
    assert_eq!(
        gate_after(3, &Policy::default(), &ledger(Severity::Medium)).reason,
        GateReason::StopNoNew
    );
}
#[test]
fn stop_clean() {
    let mut p = Policy::default();
    p.stop.no_new = false;
    assert_eq!(
        gate_after(2, &p, &IssueLedger::default()).reason,
        GateReason::StopClean
    );
}
#[test]
fn stop_max_precedes_other_checks() {
    let mut p = Policy::default();
    p.stop.max_rounds = false;
    assert_eq!(
        gate_after(5, &p, &IssueLedger::default()).reason,
        GateReason::StopMax
    );
}
#[test]
fn stopped_human_even_if_blocking_checks_disabled() {
    let mut p = Policy::default();
    p.stop.no_high = false;
    assert_eq!(
        gate_after(5, &p, &ledger(Severity::High)).reason,
        GateReason::StoppedHuman
    );
}
#[test]
fn stopped_human_with_blocker() {
    assert_eq!(
        gate_after(5, &Policy::default(), &ledger(Severity::Critical)).kind,
        GateKind::Stopped
    );
}
#[test]
fn terminal_manifest_reasons() {
    for (reason, expected) in [
        (GateReason::StopNoNew, StopReason::GateNoNewIssues),
        (GateReason::StopClean, StopReason::GateClean),
        (GateReason::StopMax, StopReason::GateMaxRounds),
        (
            GateReason::StoppedHuman,
            StopReason::GateHumanReviewRequired,
        ),
    ] {
        assert_eq!(reason.stop_reason(), Some(expected));
    }
    assert_eq!(GateReason::ContinueNew.stop_reason(), None);
}
#[test]
fn canned_three_round_run() {
    let mut l = IssueLedger::default();
    let mut issues: Vec<Issue> = [
        Severity::Critical,
        Severity::High,
        Severity::High,
        Severity::Medium,
        Severity::Medium,
        Severity::Medium,
    ]
    .iter()
    .enumerate()
    .map(|(n, s)| issue(&format!("R01-{:03}", n + 1), *s))
    .collect();
    l.apply_review(1, &review(issues.clone()));
    assert_eq!(
        l.open_counts(),
        Counts {
            critical: 1,
            high: 2,
            medium: 3,
            low: 0
        }
    );
    assert_eq!(
        gate_after(1, &Policy::default(), &l).reason,
        GateReason::ContinueOpen
    );
    issues[0].status = Status::Resolved;
    issues[3].status = Status::Resolved;
    l.apply_review(2, &review(issues.clone()));
    assert_eq!(
        gate_after(2, &Policy::default(), &l).reason,
        GateReason::ContinueOpen
    );
    issues[1].status = Status::Resolved;
    issues[2].status = Status::Resolved;
    l.apply_review(3, &review(issues));
    let g = gate_after(3, &Policy::default(), &l);
    assert_eq!(
        g.counts,
        Counts {
            critical: 0,
            high: 0,
            medium: 2,
            low: 0
        }
    );
    assert_eq!((g.kind, g.reason), (GateKind::Stop, GateReason::StopNoNew));
}
#[test]
fn omissions_history_and_author_replies() {
    let mut l = ledger(Severity::High);
    l.apply_review(2, &review(vec![]));
    assert_eq!(l.open_counts().high, 1);
    let mut a: AuthorResponse = serde_json::from_str(include_str!(
        "../../../samples/fixtures/author_response_01.json"
    ))
    .unwrap();
    a.responses.truncate(1);
    l.apply_author(2, &a);
    assert_eq!(l.history("R01-001").unwrap().responses[&2], a.responses[0]);
    let mut resolved = issue("R01-001", Severity::High);
    resolved.status = Status::Resolved;
    l.apply_review(3, &review(vec![resolved]));
    assert!(l.open().is_empty());
    assert_eq!(l.history("R01-001").unwrap().reviews.len(), 2);
    assert_eq!(l.new_material_in(3), 0);
}
#[test]
fn human_choices_preserve_history_and_only_keep_waives() {
    for choice in [HumanChoice::Accept, HumanChoice::Keep] {
        let mut l = ledger(Severity::Critical);
        assert!(l.apply_human("R01-001", choice, "Rationale", 2));
        l.apply_review(3, &review(vec![issue("R01-001", Severity::Critical)]));
        assert_eq!(l.open().is_empty(), choice == HumanChoice::Keep);
        assert_eq!(l.human_decisions()[0].note, "Rationale");
        assert!(!l.apply_human("unknown", choice, "", 2));
    }
}
#[test]
fn low_findings_are_not_new_material() {
    let mut l = ledger(Severity::Low);
    l.apply_review(2, &review(vec![issue("R02-001", Severity::Low)]));
    assert_eq!(l.new_material_in(1), 0);
    assert_eq!(l.new_material_in(2), 0);
    assert_eq!(
        gate_after(2, &Policy::default(), &l).reason,
        GateReason::StopNoNew
    );
}
#[test]
fn policy_validation_and_defaults() {
    let p = Policy::default();
    assert!(p.validate().is_ok());
    assert_eq!(
        (
            p.max_rounds,
            p.min_rounds,
            p.contribution,
            p.new_content_limit_pct
        ),
        (5, 2, Contribution::Correct, 10.0)
    );
    for (min, max) in [(0, 5), (1, 6), (3, 2)] {
        let mut p = p.clone();
        p.min_rounds = min;
        p.max_rounds = max;
        assert!(p.validate().is_err());
    }
    for pct in [-1.0, 101.0, f64::NAN, f64::INFINITY] {
        let mut p = p.clone();
        p.new_content_limit_pct = pct;
        assert!(p.validate().is_err());
    }
    assert_eq!(Provider::Claude.other(), Provider::Codex);
    assert_eq!(Provider::Codex.other(), Provider::Claude);
}
#[test]
fn word_diff_insertions_replacements_and_deletions() {
    assert_eq!(new_text_pct("one two three", "one new two three"), 33.3);
    assert_eq!(new_text_pct("one two three", "one other three"), 33.3);
    assert_eq!(new_text_pct("one two", "one"), 0.0);
    assert_eq!(new_text_pct("one  two", "one\ntwo"), 0.0);
    assert_eq!(new_text_pct("", ""), 0.0);
    assert_eq!(new_text_pct("", "new"), 100.0);
}
#[test]
fn sections_ignore_code_and_duplicate_headings() {
    assert_eq!(
        new_sections("# Old", "# Old\n## New\n## New\n```\n# Code\n```"),
        1
    );
    assert_eq!(new_sections("# **Old**", "## Old"), 0);
    assert_eq!(new_sections("", "Setext\n======"), 1);
}
#[test]
fn accept_keeps_high_open_until_reviewer_resolves() {
    let mut l = ledger(Severity::High);
    assert!(l.apply_human("R01-001", HumanChoice::Accept, "Correct the amount", 1));
    assert_eq!(l.open_counts().high, 1, "accepted High must stay open");
    let open: Vec<Issue> = l.open().into_iter().cloned().collect();
    let prompt = symvoulio_core::prompts::author_prompt(
        2,
        "Fix",
        &[],
        &Policy::default(),
        "Original",
        "Revision",
        &[],
        &open,
    );
    assert!(prompt.contains("R01-001"));
    let instructions = symvoulio_core::prompts::human_instructions(&l);
    assert!(instructions.contains("Correct the amount"));
    assert!(instructions.contains("implement; remains open"));
    let mut fixed = issue("R01-001", Severity::High);
    fixed.status = Status::Resolved;
    l.apply_review(2, &review(vec![fixed]));
    assert!(l.open().is_empty());
}

#[test]
fn material_escalation_and_reopening_reset_quiet_rounds() {
    let mut l = ledger(Severity::Low);
    l.apply_review(2, &review(vec![issue("R01-001", Severity::Medium)]));
    assert_eq!(l.new_material_in(2), 1);
    assert_eq!(
        gate_after(2, &Policy::default(), &l).reason,
        GateReason::ContinueNew
    );
    let mut resolved = issue("R01-001", Severity::Medium);
    resolved.status = Status::Resolved;
    l.apply_review(3, &review(vec![resolved]));
    l.apply_review(4, &review(vec![issue("R01-001", Severity::Medium)]));
    assert_eq!(l.new_material_in(4), 1);
}
