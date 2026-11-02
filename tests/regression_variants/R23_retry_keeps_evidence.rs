// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! Row R23 (a retry overwrites its failure evidence). Pre-fix commit d3db9a0, fix commit 5cd46e9. Old-API red variant.
//! A reviewer auth failure leaves provider-output evidence for the failed attempt; after `resume` succeeds, every file
//! that existed under audit/provider_output before the retry must still exist with the same bytes. The check is
//! independent of file naming (the fix adds an `_aNN` attempt suffix; the pre-fix engine rewrites the same name).
//! Uses only `start`, `resume`, `RunEvent::Phase` and the output folder.
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use symvoulio_core::{engine::*, provider::*, *};

static ENV: Mutex<()> = Mutex::new(());

fn evidence(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| (e.file_name().to_string_lossy().into_owned(), fs::read(e.path()).unwrap()))
        .collect()
}

#[test]
fn r23_a_retry_never_overwrites_the_failed_attempts_evidence() {
    let _guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("SYMVOULIO_FAKE_CLI", env!("CARGO_BIN_EXE_fake-cli"));
    for key in ["FAKE_CLI_SCENARIO", "FAKE_CLI_LOGGED_OUT", "FAKE_CLI_LOG"] {
        std::env::remove_var(key);
    }
    let dir = tempfile::tempdir().unwrap();
    let primary = dir.path().join("business.md");
    fs::write(&primary, "# Northwind\nCapital cost $4.2M. Growth 18 %. owner to be confirmed by Operations").unwrap();
    let cfg = RunConfig {
        run_id: "r23-run".into(),
        name: "Northwind".into(),
        run_dir: dir.path().join("run"),
        author: Provider::Claude,
        objective: "Improve for executive review".into(),
        criteria: vec!["All totals must reconcile.".into()],
        policy: Policy::default(),
        primary,
        supporting: vec![],
        call_timeout: Duration::from_secs(10),
        author_model: None,
        reviewer_model: None,
    };
    let run = cfg.run_dir.clone();
    let clients = || -> (Arc<dyn ProviderClient>, Arc<dyn ProviderClient>) { (Arc::new(ClaudeCli), Arc::new(CodexCli)) };
    let switched = AtomicBool::new(false);
    let s = start(cfg, clients(), Arc::default(), |e| {
        if let RunEvent::Phase { role: Role::Reviewer, .. } = e {
            if !switched.swap(true, Ordering::SeqCst) {
                std::env::set_var("FAKE_CLI_SCENARIO", "auth_fail");
            }
        }
    })
    .unwrap();
    assert_eq!(s.last_error.as_ref().map(|e| e.kind.as_str()), Some("auth"), "setup: the reviewer must fail with auth");
    let audit = run.join("output/audit/provider_output");
    let before = evidence(&audit);
    assert!(!before.is_empty(), "setup: the failed attempt left no evidence");
    std::env::remove_var("FAKE_CLI_SCENARIO");
    let s = resume(&run, clients(), Arc::default(), |_| {}).unwrap();
    std::env::remove_var("SYMVOULIO_FAKE_CLI");
    assert!(s.last_error.is_none() && s.rounds_done >= 1, "setup: the resumed run proceeds: {:?}", s.last_error);
    let after = evidence(&audit);
    let lost: Vec<_> = before
        .iter()
        .filter(|(name, bytes)| after.get(*name) != Some(*bytes))
        .map(|(name, _)| name.clone())
        .collect();
    assert!(lost.is_empty(), "the retry overwrote or removed the failed attempt's evidence: {lost:?}");
}
