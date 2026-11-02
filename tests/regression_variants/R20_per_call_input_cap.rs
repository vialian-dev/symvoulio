// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! Row R20 (the per-call input cap is per file). Pre-fix commit d3db9a0, fix commit 5cd46e9. Old-API red variant.
//! (1) two files that are each under the per-file cap but together over the 8 MB per-call cap are rejected, naming both
//!     files (the pre-fix engine accepts them);
//! (2) a complete prompt over the cap pauses with kind `input_too_large` naming the primary file before any provider
//!     is spawned. FAKE_CLI_SCENARIO=auth_fail makes a provider that IS spawned fail fast with kind `auth`, so the
//!     pre-fix engine fails the assertion quickly instead of pushing 9 MB through three rounds.
//! Uses only `start`, `RunConfig` and `RunSnapshot.{state,last_error}`.
use std::{
    fs,
    sync::{Arc, Mutex},
    time::Duration,
};
use symvoulio_core::{engine::*, provider::*, *};

static ENV: Mutex<()> = Mutex::new(());

fn setup(dir: &std::path::Path) -> RunConfig {
    std::env::set_var("SYMVOULIO_FAKE_CLI", env!("CARGO_BIN_EXE_fake-cli"));
    std::env::remove_var("FAKE_CLI_LOGGED_OUT");
    std::env::remove_var("FAKE_CLI_LOG");
    std::env::set_var("FAKE_CLI_SCENARIO", "auth_fail");
    let primary = dir.join("business.md");
    fs::write(&primary, "# Northwind\nCapital cost $4.2M.").unwrap();
    RunConfig {
        run_id: "r20-run".into(),
        name: "Northwind".into(),
        run_dir: dir.join("run"),
        author: Provider::Claude,
        objective: "Improve for executive review".into(),
        criteria: vec!["All totals must reconcile.".into()],
        policy: Policy::default(),
        primary,
        supporting: vec![],
        call_timeout: Duration::from_secs(10),
        author_model: None,
        reviewer_model: None,
    }
}
fn clients() -> (Arc<dyn ProviderClient>, Arc<dyn ProviderClient>) {
    (Arc::new(ClaudeCli), Arc::new(CodexCli))
}
fn teardown() {
    for key in ["SYMVOULIO_FAKE_CLI", "FAKE_CLI_SCENARIO"] {
        std::env::remove_var(key);
    }
}

#[test]
fn r20_two_files_under_the_per_file_cap_but_over_the_aggregate_cap_are_rejected() {
    let _guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = setup(dir.path());
    fs::write(&cfg.primary, vec![b'x'; 4 * 1024 * 1024]).unwrap();
    let supporting = dir.path().join("support.txt");
    fs::write(&supporting, vec![b'x'; 4 * 1024 * 1024 + 1]).unwrap();
    cfg.supporting.push(supporting);
    let result = start(cfg, clients(), Arc::default(), |_| {});
    teardown();
    let snapshot = match result {
        Err(error) => {
            let message = error.to_string();
            assert!(message.contains("business.md") && message.contains("support.txt"), "rejected without naming both files: {message}");
            return;
        }
        Ok(s) => s,
    };
    panic!(
        "aggregate inputs over 8 MB were accepted: run started (state {:?}, last_error {:?})",
        snapshot.state,
        snapshot.last_error.map(|e| e.kind)
    );
}

#[test]
fn r20_oversized_complete_prompt_pauses_before_a_provider_is_spawned() {
    let _guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = setup(dir.path());
    cfg.objective = "x".repeat(9 * 1024 * 1024);
    let result = start(cfg, clients(), Arc::default(), |_| {});
    teardown();
    let s = result.expect("an oversized prompt pauses the run, it does not fail the call");
    assert_eq!(s.state, RunState::Paused);
    let error = s.last_error.expect("a paused run reports why");
    assert_eq!(error.kind, "input_too_large", "oversized prompt reached the provider (kind {})", error.kind);
    assert!(error.message.contains("business.md"), "{}", error.message);
}
