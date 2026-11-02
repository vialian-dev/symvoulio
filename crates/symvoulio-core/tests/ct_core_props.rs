// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! CRUEL TESTER property tests for card M1-CORE (DoD2): the gate against an independent truth-table oracle written
//! from VAL3..VAL5, and the schema validator against arbitrary JSON.
use proptest::prelude::*;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use symvoulio_core::{provider::*, *};

// ---------------------------------------------------------------------------------------------------------------
// gate oracle
// ---------------------------------------------------------------------------------------------------------------
/// One reviewer snapshot: severity index 0 critical, 1 high, 2 medium, 3 low.
#[derive(Clone, Debug)]
struct Snap {
    id: u8,
    sev: u8,
    resolved: bool,
}
fn severity(i: u8) -> Severity {
    [Severity::Critical, Severity::High, Severity::Medium, Severity::Low][i as usize]
}
#[derive(Debug, PartialEq, Clone, Copy)]
enum Want {
    Continue,
    Stop,
    Stopped,
}
/// Independent oracle. `None` = the card leaves the case open (see report), so it is not compared.
fn oracle(rounds: &[Vec<Snap>], r: usize, min: u32, max: u32) -> (Option<Want>, [usize; 4]) {
    // history per id: (round, sev, resolved)
    let mut hist: BTreeMap<u8, Vec<(usize, u8, bool)>> = BTreeMap::new();
    for (i, round) in rounds[..r].iter().enumerate() {
        for s in round {
            hist.entry(s.id).or_default().push((i + 1, s.sev, s.resolved));
        }
    }
    let mut counts = [0usize; 4];
    for h in hist.values() {
        let last = h.last().unwrap();
        if !last.2 {
            counts[last.1 as usize] += 1;
        }
    }
    let new_material = |k: usize| -> usize {
        let mut n = 0;
        for h in hist.values() {
            for (idx, (round, sev, resolved)) in h.iter().enumerate() {
                if *round != k || *resolved || *sev == 3 {
                    continue;
                }
                let new = idx == 0 || h[idx - 1].2 || h[idx - 1].1 == 3;
                if new {
                    n += 1;
                }
            }
        }
        n
    };
    let blocking = counts[0] > 0 || counts[1] > 0;
    let open_any = counts.iter().sum::<usize>() > 0;
    let want = if r as u32 >= max {
        if blocking {
            Some(Want::Stopped)
        } else if !open_any {
            Some(Want::Stop)
        } else {
            None // cap reached with only medium/low left: the card text does not pin it
        }
    } else if blocking || (r as u32) < min {
        Some(Want::Continue)
    } else if r == 1 {
        if new_material(1) > 0 {
            Some(Want::Continue)
        } else {
            None // a single review cannot be "two consecutive": pinned by a dedicated test in ct_core.rs
        }
    } else if new_material(r) == 0 && new_material(r - 1) == 0 {
        Some(Want::Stop)
    } else {
        Some(Want::Continue)
    };
    (want, counts)
}
fn arb_round() -> impl Strategy<Value = Vec<Snap>> {
    prop::collection::vec((0u8..4, 0u8..4, any::<bool>()), 0..5).prop_map(|v| {
        let mut seen = BTreeSet::new();
        v.into_iter()
            .filter(|(id, _, _)| seen.insert(*id))
            .map(|(id, sev, resolved)| Snap { id, sev, resolved })
            .collect()
    })
}
fn build(rounds: &[Vec<Snap>], upto: usize) -> IssueLedger {
    let mut l = IssueLedger::default();
    for (i, round) in rounds[..upto].iter().enumerate() {
        let issues = round
            .iter()
            .map(|s| Issue {
                issue_id: format!("R01-{:03}", s.id + 1),
                severity: severity(s.sev),
                criterion: "c".into(),
                finding: "f".into(),
                evidence: "e".into(),
                proposed_action: "a".into(),
                human_decision_required: false,
                status: if s.resolved { Status::Resolved } else { Status::Open },
            })
            .collect();
        l.apply_review(
            i as u32 + 1,
            &Review {
                summary: "s".into(),
                issues,
                criteria_results: vec![],
            },
        );
    }
    l
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(600))]
    #[test]
    fn test_dod2_gate_matches_truth_table_oracle(
        max in 1u32..=5,
        min_seed in 0u32..5,
        pct in 0.0f64..=100.0,
        contribution in 0u8..3,
        rounds in prop::collection::vec(arb_round(), 5),
    ) {
        let min = 1 + min_seed % max;
        let policy = Policy {
            max_rounds: max,
            min_rounds: min,
            contribution: [Contribution::Verify, Contribution::Correct, Contribution::Improve][contribution as usize],
            new_content_limit_pct: pct,
            ..Policy::default()
        };
        prop_assert!(policy.validate().is_ok());
        for r in 1..=max as usize {
            let ledger = build(&rounds, r);
            let g = gate_after(r as u32, &policy, &ledger);
            let (want, counts) = oracle(&rounds, r, min, max);
            prop_assert_eq!(
                (g.counts.critical, g.counts.high, g.counts.medium, g.counts.low),
                (counts[0], counts[1], counts[2], counts[3]),
                "open counts, round {}", r
            );
            // gate is a pure function of its inputs
            prop_assert_eq!(gate_after(r as u32, &policy, &ledger), g.clone());
            if let Some(w) = want {
                let got = match g.kind {
                    GateKind::Continue => Want::Continue,
                    GateKind::Stop => Want::Stop,
                    GateKind::Stopped => Want::Stopped,
                };
                prop_assert_eq!(got, w, "round {} min {} max {} rounds {:?}", r, min, max, &rounds[..r]);
            }
            match g.kind {
                GateKind::Continue => prop_assert_eq!(g.reason.stop_reason(), None),
                GateKind::Stop => {
                    let sr = g.reason.stop_reason();
                    prop_assert!(sr.is_some());
                    prop_assert_ne!(sr, Some(StopReason::GateHumanReviewRequired));
                }
                GateKind::Stopped => prop_assert_eq!(g.reason.stop_reason(), Some(StopReason::GateHumanReviewRequired)),
            }
            // a Stopped verdict only ever happens at the cap
            if g.kind == GateKind::Stopped { prop_assert!(r as u32 >= max); }
            // a Stop never leaves a critical or high behind
            if g.kind == GateKind::Stop { prop_assert!(counts[0] == 0 && counts[1] == 0); }
        }
    }

    #[test]
    fn test_dod2_new_material_matches_oracle(rounds in prop::collection::vec(arb_round(), 1..=5)) {
        // independent recount of VAL4: severity critical/high/medium, first sight / escalation from low / reopening
        for r in 1..=rounds.len() {
            let ledger = build(&rounds, r);
            let mut expect = 0usize;
            for s in &rounds[r - 1] {
                if s.resolved || s.sev == 3 { continue; }
                let prior = rounds[..r - 1].iter().rev().find_map(|rd| rd.iter().find(|x| x.id == s.id));
                let new = match prior { None => true, Some(p) => p.resolved || p.sev == 3 };
                if new { expect += 1; }
            }
            prop_assert_eq!(ledger.new_material_in(r as u32), expect, "round {}", r);
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// validator oracle
// ---------------------------------------------------------------------------------------------------------------
fn valid_id(v: &Value) -> bool {
    let Some(s) = v.as_str() else { return false };
    let b = s.as_bytes();
    b.len() == 7
        && b[0] == b'R'
        && b[1].is_ascii_digit()
        && b[2].is_ascii_digit()
        && b[3] == b'-'
        && b[4..].iter().all(|c| c.is_ascii_digit())
}
fn keys_are(o: &Map<String, Value>, keys: &[&str]) -> bool {
    o.len() == keys.len() && keys.iter().all(|k| o.contains_key(*k))
}
fn is_str(v: &Value) -> bool {
    v.is_string()
}
fn str_array(v: &Value) -> bool {
    v.as_array().is_some_and(|a| a.iter().all(is_str))
}
fn oracle_review(v: &Value) -> bool {
    let Some(o) = v.as_object() else { return false };
    if !keys_are(o, &["summary", "issues", "criteria_results"]) || !is_str(&o["summary"]) {
        return false;
    }
    let Some(issues) = o["issues"].as_array() else { return false };
    let mut ids = BTreeSet::new();
    for i in issues {
        let Some(i) = i.as_object() else { return false };
        if !keys_are(
            i,
            &["issue_id", "severity", "criterion", "finding", "evidence", "proposed_action", "human_decision_required", "status"],
        ) {
            return false;
        }
        if !valid_id(&i["issue_id"])
            || !["critical", "high", "medium", "low"].contains(&i["severity"].as_str().unwrap_or("?"))
            || !["open", "resolved"].contains(&i["status"].as_str().unwrap_or("?"))
            || !["criterion", "finding", "evidence", "proposed_action"].iter().all(|k| is_str(&i[*k]))
            || !i["human_decision_required"].is_boolean()
        {
            return false;
        }
        if !ids.insert(i["issue_id"].as_str().unwrap().to_string()) {
            return false;
        }
    }
    let Some(cr) = o["criteria_results"].as_array() else { return false };
    cr.iter().all(|c| {
        c.as_object()
            .is_some_and(|c| keys_are(c, &["criterion", "met", "note"]) && is_str(&c["criterion"]) && c["met"].is_boolean() && is_str(&c["note"]))
    })
}
fn oracle_author(v: &Value) -> bool {
    let Some(o) = v.as_object() else { return false };
    if !keys_are(o, &["revised_document", "changes_made", "responses", "new_claims", "human_decisions_required"])
        || !is_str(&o["revised_document"])
        || !str_array(&o["changes_made"])
        || !str_array(&o["new_claims"])
    {
        return false;
    }
    let Some(rs) = o["responses"].as_array() else { return false };
    for r in rs {
        let Some(r) = r.as_object() else { return false };
        if !keys_are(r, &["issue_id", "response", "explanation", "evidence"]) || !valid_id(&r["issue_id"]) {
            return false;
        }
        let Some(kind) = r["response"].as_str() else { return false };
        if !["accepted", "rejected_with_evidence", "partially_accepted", "human_decision_required"].contains(&kind) {
            return false;
        }
        if !is_str(&r["explanation"]) || !is_str(&r["evidence"]) {
            return false;
        }
        if kind == "rejected_with_evidence" && r["evidence"].as_str().unwrap().trim().is_empty() {
            return false;
        }
    }
    let Some(hs) = o["human_decisions_required"].as_array() else { return false };
    hs.iter().all(|h| {
        h.as_object()
            .is_some_and(|h| keys_are(h, &["issue_id", "question"]) && valid_id(&h["issue_id"]) && is_str(&h["question"]))
    })
}

fn arb_review() -> impl Strategy<Value = Value> {
    (
        prop::collection::vec((0u8..4, 0u8..2, any::<bool>(), "[a-z ]{0,6}"), 0..4),
        prop::collection::vec(("[a-z]{0,4}", any::<bool>()), 0..3),
        "[a-z ]{0,10}",
    )
        .prop_map(|(issues, crs, summary)| {
            let issues: Vec<Value> = issues
                .into_iter()
                .enumerate()
                .map(|(n, (sev, st, hd, text))| {
                    json!({
                        "issue_id": format!("R01-{:03}", n + 1),
                        "severity": (["critical", "high", "medium", "low"][sev as usize]),
                        "criterion": text, "finding": "f", "evidence": "e", "proposed_action": "a",
                        "human_decision_required": hd,
                        "status": (["open", "resolved"][st as usize]),
                    })
                })
                .collect();
            let crs: Vec<Value> = crs.into_iter().map(|(c, m)| json!({"criterion": c, "met": m, "note": "n"})).collect();
            json!({"summary": summary, "issues": issues, "criteria_results": crs})
        })
}
const BAD_IDS: [&str; 10] = [
    "R1-001", "R01-0001", "r01-001", "R01-001\n", "R01-00\u{661}", "", "R01_001", " R01-001", "R01-001 ", "R01-00",
];
fn first_issue(v: &mut Value) -> Option<&mut Map<String, Value>> {
    v["issues"].as_array_mut().and_then(|a| a.first_mut()).and_then(|x| x.as_object_mut())
}
fn first(v: &mut Value) -> Option<&mut Map<String, Value>> {
    v["responses"].as_array_mut().and_then(|a| a.first_mut()).and_then(|x| x.as_object_mut())
}
fn mutate_review(v: &mut Value, op: u8, pick: usize) {
    match op {
        0 => {
            let k = ["summary", "issues", "criteria_results"][pick % 3];
            v.as_object_mut().unwrap().remove(k);
        }
        1 => {
            v.as_object_mut().unwrap().insert("extra".into(), json!(1));
        }
        2 => v["summary"] = json!(5),
        3 => {
            if let Some(i) = first_issue(v) {
                i.insert("issue_id".into(), json!(BAD_IDS[pick % BAD_IDS.len()]));
            }
        }
        4 => {
            if let Some(i) = first_issue(v) {
                i.insert("severity".into(), json!(["urgent", "Critical", "", "CRITICAL"][pick % 4]));
            }
        }
        5 => {
            if let Some(i) = first_issue(v) {
                i.insert("x".into(), json!(true));
            }
        }
        6 => {
            if let Some(i) = first_issue(v) {
                let k = ["issue_id", "severity", "criterion", "finding", "evidence", "proposed_action", "human_decision_required", "status"][pick % 8];
                i.remove(k);
            }
        }
        7 => {
            if let Some(a) = v["issues"].as_array_mut() {
                if let Some(f) = a.first().cloned() {
                    a.push(f);
                }
            }
        }
        8 => {
            if let Some(i) = first_issue(v) {
                i.insert("status".into(), json!("closed"));
            }
        }
        9 => {
            if let Some(i) = first_issue(v) {
                i.insert("human_decision_required".into(), json!("true"));
            }
        }
        10 => match v["criteria_results"].as_array_mut() {
            Some(a) if !a.is_empty() => {
                a[0].as_object_mut().unwrap().remove("met");
            }
            Some(a) => a.push(json!({"criterion": "c"})),
            None => {}
        },
        11 => v["issues"] = json!({}),
        _ => {}
    }
}
fn arb_author() -> impl Strategy<Value = Value> {
    (
        "[a-z ]{0,8}",
        prop::collection::vec("[a-z]{0,4}", 0..3),
        prop::collection::vec((0u8..4, 0u8..4, "[a-z ]{0,5}", "[ a-z]{0,3}"), 0..4),
        prop::collection::vec("[a-z]{0,4}", 0..3),
        prop::collection::vec((0u8..4, "[a-z ?]{0,6}"), 0..3),
    )
        .prop_map(|(doc, changes, rs, claims, hs)| {
            let rs: Vec<Value> = rs
                .into_iter()
                .map(|(id, kind, ex, ev)| {
                    json!({
                        "issue_id": format!("R01-{:03}", id + 1),
                        "response": (["accepted", "rejected_with_evidence", "partially_accepted", "human_decision_required"][kind as usize]),
                        "explanation": ex, "evidence": ev,
                    })
                })
                .collect();
            let hs: Vec<Value> = hs.into_iter().map(|(id, q)| json!({"issue_id": format!("R02-{:03}", id + 1), "question": q})).collect();
            json!({"revised_document": doc, "changes_made": changes, "responses": rs, "new_claims": claims, "human_decisions_required": hs})
        })
}
fn mutate_author(v: &mut Value, op: u8, pick: usize) {
    match op {
        0 => {
            let k = ["revised_document", "changes_made", "responses", "new_claims", "human_decisions_required"][pick % 5];
            v.as_object_mut().unwrap().remove(k);
        }
        1 => {
            v.as_object_mut().unwrap().insert("extra".into(), json!(null));
        }
        2 => v["revised_document"] = json!(1),
        3 => {
            if let Some(r) = first(v) {
                r.insert("response".into(), json!(["Accepted", "accept", "", "rejected"][pick % 4]));
            }
        }
        4 => {
            if let Some(r) = first(v) {
                r.insert("issue_id".into(), json!(BAD_IDS[pick % BAD_IDS.len()]));
            }
        }
        5 => {
            if let Some(r) = first(v) {
                r.remove("evidence");
            }
        }
        6 => {
            if let Some(h) = v["human_decisions_required"].as_array_mut().and_then(|a| a.first_mut()).and_then(|x| x.as_object_mut()) {
                h.remove("question");
            }
        }
        7 => {
            if let Some(a) = v["new_claims"].as_array_mut() {
                a.push(json!(3));
            }
        }
        8 => v["changes_made"] = json!("one change"),
        9 => {
            if let Some(r) = first(v) {
                r.insert("response".into(), json!("rejected_with_evidence"));
                r.insert("evidence".into(), json!(["   ", "", "\t\n", "x"][pick % 4]));
            }
        }
        10 => {
            if let Some(r) = first(v) {
                r.insert("extra".into(), json!(1));
            }
        }
        _ => {}
    }
}
fn arb_json() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(|n| json!(n)),
        "[ -~]{0,8}".prop_map(Value::String),
    ];
    let key = prop_oneof![
        Just("summary".to_string()),
        Just("issues".to_string()),
        Just("criteria_results".to_string()),
        Just("revised_document".to_string()),
        Just("responses".to_string()),
        "[a-z_]{1,6}",
    ];
    leaf.prop_recursive(4, 40, 4, move |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4).prop_map(Value::Array),
            prop::collection::btree_map(key.clone(), inner, 0..5).prop_map(|m| Value::Object(m.into_iter().collect())),
        ]
    })
}
fn accepts(kind: SchemaKind, v: &Value) -> bool {
    validate_contract(kind, v).is_ok()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(800))]
    #[test]
    fn test_dod2_validator_review_agrees_with_oracle(mut v in arb_review(), op in 0u8..13, pick in 0usize..64, second in 0u8..13) {
        // untouched documents are valid
        prop_assert!(oracle_review(&v));
        prop_assert!(accepts(SchemaKind::Review, &v), "{v}");
        mutate_review(&mut v, op, pick);
        mutate_review(&mut v, second, pick / 3);
        prop_assert_eq!(accepts(SchemaKind::Review, &v), oracle_review(&v), "{}", v);
        // a review is never an author response and vice versa
        prop_assert!(!accepts(SchemaKind::AuthorResponse, &v));
    }

    #[test]
    fn test_dod2_validator_author_agrees_with_oracle(mut v in arb_author(), op in 0u8..12, pick in 0usize..64, second in 0u8..12) {
        mutate_author(&mut v, op, pick);
        mutate_author(&mut v, second, pick / 3);
        prop_assert_eq!(accepts(SchemaKind::AuthorResponse, &v), oracle_author(&v), "{}", v);
        prop_assert!(!accepts(SchemaKind::Review, &v));
    }

    #[test]
    fn test_dod2_validator_never_panics_and_never_accepts_invalid(v in arb_json()) {
        let r = accepts(SchemaKind::Review, &v);
        let a = accepts(SchemaKind::AuthorResponse, &v);
        prop_assert!(!r || oracle_review(&v), "accepted invalid review: {}", v);
        prop_assert!(!a || oracle_author(&v), "accepted invalid author response: {}", v);
        if !v.is_object() {
            prop_assert!(!r && !a);
        }
    }

    #[test]
    fn test_dod2_validator_non_json_shapes_are_rejected(s in ".{0,40}", n in any::<i64>(), b in any::<bool>()) {
        for v in [Value::Null, json!(s), json!(n), json!(b), json!([]), json!([1, 2]), json!({})] {
            prop_assert!(!accepts(SchemaKind::Review, &v), "{}", v);
            prop_assert!(!accepts(SchemaKind::AuthorResponse, &v), "{}", v);
        }
    }

    // VAL15 as a property: redact never panics, is idempotent, and removes secret-shaped values
    #[test]
    fn test_dod2_redact_total_idempotent_and_masks(
        noise in "[ -~\u{0400}-\u{04FF}\r\n]{0,60}",
        key in "[A-Za-z0-9]{16,40}",
        bearer in "[A-Za-z0-9._-]{8,40}",
    ) {
        let once = redact(&noise);
        prop_assert_eq!(redact(&once), once.clone());
        let text = format!("{noise} sk-{key} and Bearer {bearer} {{\"password\":\"p{key}\"}}");
        let r = redact(&text);
        prop_assert!(!r.contains(&format!("sk-{key}")), "{}", r);
        prop_assert!(!r.contains(&bearer), "{}", r);
        prop_assert!(!r.contains(&format!("p{key}")), "{}", r);
        prop_assert_eq!(redact(&r), r.clone());
    }
}
