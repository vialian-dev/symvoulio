// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
    #[test]
    fn interrupted_decisions_keep_partial_export_available() {
        let _environment = PROVIDER_ENV.lock().unwrap();
        std::env::set_var("SYMVOULIO_FAKE_CLI", fake_cli());
        for cancelled in [false, true] {
            for decision in [DecisionKind::Approved, DecisionKind::Rejected] {
                let w = Workspace::new();
                let primary = w.0.join("primary.md");
                fs::write(&primary, "# Original").unwrap();
                let dir = w.0.join("runs/interrupted");
                let config = engine::RunConfig {
                    run_id: "interrupted".into(), name: "Interrupted review".into(),
                    run_dir: dir.clone(), primary, supporting: vec![],
                    author: Provider::Claude, objective: "Review".into(),
                    criteria: vec!["Accuracy".into()], policy: Policy::default(),
                    call_timeout: Duration::from_secs(10), author_model: None, reviewer_model: None,
                };
                let control = Arc::new(engine::RunControl::default());
                let stop = control.clone();
                let result = engine::start(config, (Arc::new(ClaudeCli), Arc::new(CodexCli)), control, move |event| {
                    if matches!(event, engine::RunEvent::Phase { role: symvoulio_core::Role::Reviewer, .. }) {
                        if cancelled { stop.cancel.store(true, Ordering::SeqCst); }
                        else { stop.stop(); }
                    }
                }).unwrap();
                assert_eq!(result.state, RunState::Stopped);
                assert_eq!(result.rounds_done, 0);
                // Cancelling an in-flight provider call is persisted as UserStop too.
                assert_eq!(result.stop_reason, Some(StopReason::UserStop));
                assert!(export(&dir, true).is_ok());
                engine::decide(&dir, decision, "QA", "Reviewed interrupted package").unwrap();
                let exported = export(&dir, true).unwrap();
                assert!(exported.path.join("final/final_review.html").is_file());
                assert!(exported.path.join("revisions/revision_01.md").is_file());
                let saved: serde_json::Value = serde_json::from_slice(&fs::read(exported.path.join("final/human_decision.json")).unwrap()).unwrap();
                assert_eq!(saved["kind"], serde_json::to_value(decision).unwrap());
            }
        }
        std::env::remove_var("SYMVOULIO_FAKE_CLI");
    }

    #[cfg(windows)]
    #[test]
    fn approved_run_with_unreadable_final_document_is_listed() {
        use std::os::windows::fs::OpenOptionsExt;
        let _environment = PROVIDER_ENV.lock().unwrap();
        std::env::set_var("SYMVOULIO_FAKE_CLI", fake_cli());
        let w = Workspace::new();
        let state = test_state(&w);
        let primary = w.0.join("primary.md");
        fs::write(&primary, "# Original").unwrap();
        let dir = w.0.join("runs/listed");
        let config = engine::RunConfig {
            run_id: "listed".into(), name: "Listed approval".into(),
            run_dir: dir.clone(), primary, supporting: vec![],
            author: Provider::Claude, objective: "Review".into(),
            criteria: vec!["Accuracy".into()], policy: Policy::default(),
            call_timeout: Duration::from_secs(10), author_model: None, reviewer_model: None,
        };
        engine::start(config, (Arc::new(ClaudeCli), Arc::new(CodexCli)), Arc::default(), |_| {}).unwrap();
        engine::decide(&dir, DecisionKind::Approved, "Owner", "approved").unwrap();
        let target = dir.join("output/final/final_document.md");
        let held = fs::OpenOptions::new().read(true).share_mode(0).open(&target).unwrap();
        assert!(fs::read(&target).is_err());
        let listed = state.list_runs().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "listed");
        assert_eq!(listed[0].state, RunState::Approved);
        let snapshot = state.get("listed").unwrap();
        assert_eq!(snapshot.decision.unwrap().name, "Owner");
        assert_eq!(snapshot.last_error.unwrap().kind, "publication");
        drop(held);
        assert!(state.get("listed").unwrap().last_error.is_none());
        std::env::remove_var("SYMVOULIO_FAKE_CLI");
    }
