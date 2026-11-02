// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! Row R08 (the reviewer never receives the original primary document). Pre-fix commit 9b67929, fix commit 73ae3e5.
//! Old-API red variant: `reviewer_prompt` changed its signature in the fix, so the check is end to end. A spy
//! ProviderClient wraps the real CLI adapter (on the fake CLI), records the prompt of every Reviewer call and asserts
//! that, in rounds 2 and 3 (where the current revision no longer contains the original wording: "$4.2M" became
//! "$4.6M"), the reviewer prompt still carries the original primary text. Uses only `start`, `ProviderClient`,
//! `CallRequest.{role,round,prompt}` and `RunConfig`, all present at 9b67929.
use std::{
    fs,
    sync::{Arc, Mutex},
    time::Duration,
};
use symvoulio_core::{engine::*, provider::*, *};

static ENV: Mutex<()> = Mutex::new(());
const ORIGINAL: &str = "# Northwind\nCapital cost $4.2M. Growth 18 %. owner to be confirmed by Operations";

struct Spy {
    inner: Arc<dyn ProviderClient>,
    seen: Arc<Mutex<Vec<(Role, u32, String)>>>,
}
impl ProviderClient for Spy {
    fn provider(&self) -> Provider {
        self.inner.provider()
    }
    fn call(&self, req: &CallRequest, sink: &mut dyn FnMut(StreamLine), cancel: &CancelToken) -> std::result::Result<CallOutcome, CallError> {
        self.seen.lock().unwrap().push((req.role, req.round, req.prompt.clone()));
        self.inner.call(req, sink, cancel)
    }
}

#[test]
fn r08_reviewer_prompt_carries_the_original_primary_document() {
    let _guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("SYMVOULIO_FAKE_CLI", env!("CARGO_BIN_EXE_fake-cli"));
    for key in ["FAKE_CLI_SCENARIO", "FAKE_CLI_LOGGED_OUT", "FAKE_CLI_LOG"] {
        std::env::remove_var(key);
    }
    let dir = tempfile::tempdir().unwrap();
    let primary = dir.path().join("business.md");
    fs::write(&primary, ORIGINAL).unwrap();
    let cfg = RunConfig {
        run_id: "r08-run".into(),
        name: "Northwind".into(),
        run_dir: dir.path().join("run"),
        author: Provider::Claude,
        objective: "Improve for executive review".into(),
        criteria: vec!["All totals must reconcile.".into()],
        policy: Policy { new_content_limit_pct: 100.0, ..Policy::default() },
        primary,
        supporting: vec![],
        call_timeout: Duration::from_secs(10),
        author_model: None,
        reviewer_model: None,
    };
    let seen = Arc::new(Mutex::new(vec![]));
    let clients: (Arc<dyn ProviderClient>, Arc<dyn ProviderClient>) = (
        Arc::new(Spy { inner: Arc::new(ClaudeCli), seen: seen.clone() }),
        Arc::new(Spy { inner: Arc::new(CodexCli), seen: seen.clone() }),
    );
    let s = start(cfg, clients, Arc::default(), |_| {}).unwrap();
    std::env::remove_var("SYMVOULIO_FAKE_CLI");
    assert!(s.rounds_done >= 3, "setup: the default scenario runs three rounds, got {}", s.rounds_done);
    let seen = seen.lock().unwrap();
    let late: Vec<_> = seen.iter().filter(|(role, round, _)| *role == Role::Reviewer && *round >= 2).collect();
    assert!(late.len() >= 2, "setup: reviewer calls of rounds 2 and 3 were not seen: {}", seen.len());
    for (_, round, prompt) in late {
        assert!(
            prompt.contains("Capital cost $4.2M. Growth 18 %. owner to be confirmed by Operations"),
            "round {round}: the reviewer prompt does not carry the original primary document"
        );
    }
}
