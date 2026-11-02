// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! Controlled validation and real structured-answer channel regressions.
use serde_json::json;
#[cfg(unix)]
use serde_json::Value;
use symvoulio_core::provider::*;

#[test]
fn structured_output_is_an_answer_channel_only_by_exact_name() {
    let event = json!({"type":"assistant","message":{"content":[{"type":"tool_use","name":"StructuredOutput","input":{"revised_document":"# Revised"}}]}});
    assert!(!is_tool_execution(&event.to_string()));
    for name in ["Read", "structuredoutput", "StructuredOutput ", "Bash"] {
        let mut control = event.clone();
        control["message"]["content"][0]["name"] = json!(name);
        assert!(is_tool_execution(&control.to_string()));
    }
    assert!(!is_tool_execution(r#"{"type":"content_block_start","content_block":{"type":"tool_use","name":"StructuredOutput"}}"#));
    assert!(is_tool_execution(r#"{"type":"content_block_start","content_block":{"type":"tool_use","name":"Read"}}"#));
}

#[test]
fn normalize_only_unambiguous_requested_round_ids() {
    for id in ["R1-001", "R01-1", " r01-001 ", " r 1 - 1 "] {
        let mut value = json!({"issues":[{"issue_id":id}]});
        normalize_issue_ids(SchemaKind::Review, &mut value, 1).unwrap();
        assert_eq!(value["issues"][0]["issue_id"], "R01-001");
    }
    for id in ["R2-001", "R01-0001", "R01_001", "R01-x", "provider secret"] {
        let mut value = json!({"issues":[{"issue_id":id}]});
        let error = normalize_issue_ids(SchemaKind::Review, &mut value, 1).unwrap_err();
        assert_eq!(error.to_string(), "malformed structured output: issue_id does not match R01-<nnn>");
    }
    for mut value in [json!("text"), json!([]), json!(null), json!(17), json!({"issues":["text",null]})] {
        normalize_issue_ids(SchemaKind::Review, &mut value, 1).unwrap();
        assert!(validate_contract(SchemaKind::Review, &value).is_err());
    }
    let mut previous = json!({"issues":[{"issue_id":"R01-001"}]});
    normalize_issue_ids(SchemaKind::Review, &mut previous, 2).unwrap();
    assert_eq!(previous["issues"][0]["issue_id"], "R01-001");
}

#[cfg(unix)]
#[test]
fn fake_cli_outputs_keep_validation_categories_body_free_and_normalize_r46() {
    use std::{fs, os::unix::fs::PermissionsExt, sync::{Arc, atomic::AtomicBool}, time::Duration};
    let dir = tempfile::tempdir().unwrap();
    let fake = dir.path().join("fake-cli");
    let original = std::env::var_os("SYMVOULIO_FAKE_CLI");
    struct Restore(Option<std::ffi::OsString>);
    impl Drop for Restore { fn drop(&mut self) { if let Some(v) = &self.0 { std::env::set_var("SYMVOULIO_FAKE_CLI", v); } else { std::env::remove_var("SYMVOULIO_FAKE_CLI"); } } }
    let _restore = Restore(original);
    std::env::set_var("SYMVOULIO_FAKE_CLI", &fake);
    let review: Value = serde_json::from_str(include_str!("../fixtures/fake/review-1.json")).unwrap();
    let mut missing = review.clone(); missing.as_object_mut().unwrap().remove("summary");
    let mut enumeration = review.clone(); enumeration["issues"][0]["severity"] = json!("private-secret-enum");
    let mut pattern = review.clone(); pattern["issues"][0]["issue_id"] = json!("private-secret-id");
    let mut r46 = review.clone(); r46["issues"][0]["issue_id"] = json!("R1-001");
    let cases = [("private not JSON".into(), Some("not JSON")), (missing.to_string(), Some("missing field summary")), (enumeration.to_string(), Some("value not allowed for severity")), (pattern.to_string(), Some("issue_id does not match R01-<nnn>")), (r46.to_string(), None)];
    for (body, expected) in cases {
        let event = json!({"type":"item.completed","item":{"type":"agent_message","text":body}}).to_string();
        fs::write(&fake, format!("#!/bin/sh\ncat >/dev/null\ncat <<'FAKE_OUTPUT'\n{event}\nFAKE_OUTPUT\n")).unwrap();
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o700)).unwrap();
        let req = CallRequest { role: symvoulio_core::Role::Reviewer, round: 1, prompt: "ROUND: 1".into(), schema: SchemaKind::Review, workdir: dir.path().join("work"), timeout: Duration::from_secs(5), model: None };
        let result = CodexCli.call(&req, &mut |_| {}, &Arc::new(AtomicBool::new(false)));
        if let Some(expected) = expected {
            let CallError::Malformed(detail) = result.unwrap_err() else { panic!("expected malformed answer") };
            assert_eq!(validation_reason(&detail).as_deref(), Some(expected));
            assert!(!detail.contains("private"));
        } else {
            assert_eq!(result.unwrap().structured["issues"][0]["issue_id"], "R01-001");
        }
    }
    let mut pattern = review;
    pattern["issues"][0]["issue_id"] = json!("private-secret");
    let CallError::Malformed(detail) = validate_contract(SchemaKind::Review, &pattern).unwrap_err() else { panic!("pattern") };
    assert_eq!(validation_reason(&detail).as_deref(), Some("field issue_id does not match its pattern"));
}

#[cfg(windows)]
#[test]
fn windows_sharing_faults_after_commit_carry_the_decision_and_repair() {
    use std::{fs, os::windows::fs::OpenOptionsExt, sync::{mpsc, Arc}, time::{Duration, Instant}};
    use symvoulio_core::{engine::{self, RunConfig, RunControl}, Policy, Provider, RunState};
    struct Restore(Vec<(&'static str, Option<std::ffi::OsString>)>);
    impl Drop for Restore {
        fn drop(&mut self) { for (key, value) in &self.0 { if let Some(value) = value { std::env::set_var(key, value); } else { std::env::remove_var(key); } } }
    }
    let keys = ["SYMVOULIO_FAKE_CLI", "FAKE_CLI_SCENARIO", "FAKE_CLI_SCENARIO_AUTHOR", "FAKE_CLI_SCENARIO_REVIEWER"];
    let _restore = Restore(keys.iter().map(|key| (*key, std::env::var_os(key))).collect());
    for key in keys { std::env::remove_var(key); }
    std::env::set_var("SYMVOULIO_FAKE_CLI", env!("CARGO_BIN_EXE_fake-cli"));
    let dir = tempfile::tempdir().unwrap();
    let primary = dir.path().join("source.md");
    fs::write(&primary, "# Source\nA short test document.").unwrap();
    for fault in ["final", "audit", "cleanup"] {
        let mut fired = false;
        for attempt in 1..=25 {
            let run = dir.path().join(format!("run-{fault}-{attempt}"));
            let config = RunConfig { run_id: format!("sharing-{fault}-{attempt}"), name: "Sharing fault".into(), run_dir: run.clone(), author: Provider::Claude, objective: "Review".into(), criteria: vec!["Reconcile".into()], policy: Policy { max_rounds: 1, min_rounds: 1, new_content_limit_pct: 100.0, ..Policy::default() }, primary: primary.clone(), supporting: vec![], call_timeout: Duration::from_secs(10), author_model: None, reviewer_model: None };
            let completed = engine::start(config, (Arc::new(ClaudeCli), Arc::new(CodexCli)), Arc::new(RunControl::default()), |_| {}).unwrap();
            assert!(matches!(completed.state, RunState::Stopped | RunState::Completed));
            let target = match fault {
                "final" => run.join("output/final/human_decision.json"),
                "audit" => run.join("output/audit/run_manifest.json"),
                _ => run.join(".publication/output/revisions/revision_01.md"),
            };
            let watch_run = run.clone();
            let watch_target = target.clone();
            let (release_tx, release_rx) = mpsc::channel();
            let watcher = std::thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_secs(3);
                while Instant::now() < deadline {
                    let staged = watch_run.join(".publication/output/final/human_decision.json");
                    if fs::read_to_string(staged).is_ok_and(|text| text.contains("Win-Sharing-Decider")) {
                        if let Ok(handle) = fs::OpenOptions::new().read(true).share_mode(0).open(&watch_target) {
                            let _ = release_rx.recv_timeout(Duration::from_secs(10));
                            drop(handle);
                            return true;
                        }
                    }
                    std::thread::yield_now();
                }
                false
            });
            let result = engine::decide(&run, symvoulio_core::DecisionKind::Approved, "Win-Sharing-Decider", "private saved note");
            let committed_failure = result.as_ref().err().and_then(|e| e.downcast_ref::<engine::PublicationIncomplete>());
            if let Some(error) = committed_failure {
                assert_eq!(error.snapshot.decision.as_ref().unwrap().name, "Win-Sharing-Decider");
                assert_eq!(error.snapshot.state, RunState::Approved);
                assert!(error.to_string().contains("decision committed, publication incomplete"));
                // Verify that an actual Windows sharing violation, not merely an
                // injected return value, remains active until the watcher is released.
                let denied = fs::OpenOptions::new().write(true).open(&target).unwrap_err();
                assert_eq!(denied.raw_os_error(), Some(32));
            }
            let _ = release_tx.send(());
            let held = watcher.join().unwrap();
            if committed_failure.is_none() {
                assert!(result.is_ok(), "unexpected pre-commit failure: {result:?}");
                continue;
            }
            assert!(held, "the sharing lock was not acquired");
            engine::export_report(&run).unwrap();
            let manifest: serde_json::Value = serde_json::from_slice(&fs::read(run.join("output/audit/run_manifest.json")).unwrap()).unwrap();
            let decision: serde_json::Value = serde_json::from_slice(&fs::read(run.join("output/final/human_decision.json")).unwrap()).unwrap();
            assert_eq!(manifest["human_decision"], decision);
            assert_eq!(engine::load(&run).unwrap().decision.unwrap().name, "Win-Sharing-Decider");
            eprintln!("Windows sharing fault {fault} fired in {attempt} attempt(s)");
            fired = true;
            break;
        }
        assert!(fired, "the {fault} sharing fault never fired");
    }
}
