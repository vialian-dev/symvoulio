// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! Row R01 (provider calls not isolated). Pre-fix commit 0f4453c, fix commit 52f6d4d. Old-API red variant.
//! Runs a full fake-CLI run, reads the argv log of every model call and asserts the isolation flags the fix added:
//! Claude `--strict-mcp-config`, `--disable-slash-commands`, an empty `--mcp-config`, `--settings`
//! with disableAllHooks; Codex `--ignore-user-config`, `--ignore-rules`, `--ephemeral`. Uses only `start`,
//! `RunConfig`, the CLI adapters and the FAKE_CLI_LOG argv log, all available at 0f4453c.
use std::{
    fs,
    sync::{Arc, Mutex},
    time::Duration,
};
use symvoulio_core::{engine::*, provider::*, *};

static ENV: Mutex<()> = Mutex::new(());

#[test]
fn r01_every_model_call_carries_the_provider_isolation_flags() {
    let _guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("SYMVOULIO_FAKE_CLI", env!("CARGO_BIN_EXE_fake-cli"));
    for key in ["FAKE_CLI_SCENARIO", "FAKE_CLI_LOGGED_OUT"] {
        std::env::remove_var(key);
    }
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("argv.jsonl");
    std::env::set_var("FAKE_CLI_LOG", &log);
    let primary = dir.path().join("business.md");
    fs::write(&primary, "# Northwind\nCapital cost $4.2M. Growth 18 %. owner to be confirmed by Operations").unwrap();
    let cfg = RunConfig {
        run_id: "r01-run".into(),
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
    let clients: (Arc<dyn ProviderClient>, Arc<dyn ProviderClient>) = (Arc::new(ClaudeCli), Arc::new(CodexCli));
    let outcome = start(cfg, clients, Arc::default(), |_| {});
    std::env::remove_var("FAKE_CLI_LOG");
    std::env::remove_var("SYMVOULIO_FAKE_CLI");
    // The pre-fix fake CLI accepts unisolated calls; at the fix it rejects them, so a missing flag also surfaces here.
    let _ = outcome;
    let mut missing = vec![];
    let mut calls = 0;
    for line in fs::read_to_string(&log).unwrap().lines() {
        let args: Vec<String> = serde_json::from_str(line).unwrap();
        if args.iter().any(|a| a == "--version") {
            continue;
        }
        calls += 1;
        let has = |flag: &str| args.iter().any(|a| a == flag);
        let pair = |flag: &str, value: &str| args.windows(2).any(|w| w[0] == flag && w[1] == value);
        if args[1] == "claude" {
            for flag in ["--strict-mcp-config", "--disable-slash-commands"] {
                if !has(flag) {
                    missing.push(format!("claude: missing {flag}"));
                }
            }
            if !pair("--mcp-config", r#"{"mcpServers":{}}"#) {
                missing.push("claude: missing empty --mcp-config".into());
            }
            let settings = args.windows(2).find(|w| w[0] == "--settings").map(|w| w[1].clone());
            if !settings.is_some_and(|s| s.contains("disableAllHooks")) {
                missing.push("claude: missing --settings disableAllHooks".into());
            }
        } else {
            for flag in ["--ignore-user-config", "--ignore-rules", "--ephemeral"] {
                if !has(flag) {
                    missing.push(format!("codex: missing {flag}"));
                }
            }
        }
    }
    assert!(calls >= 2, "setup: expected model calls in the argv log, found {calls}");
    assert!(missing.is_empty(), "provider calls are not isolated: {missing:#?}");
}
