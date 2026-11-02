// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! Row R06 (approval and export can refer to different documents). Pre-fix commit 0f4453c, fix commit 52f6d4d.
//! Old-API red variant: before the fix `write_final` published the highest revision file found on disk, not the
//! committed revision. Two cases using only `start`, `decide`, `OutputPackage::write_revision` and `RunSnapshot.documents`:
//! (1) a run Stopped inside review of round 2 has revision_02.md on disk but rounds_done == 1: final/ and the
//!     approval must refer to the committed revision 1;
//! (2) an orphan revision_99 written next to a finished run must not become final.
//! (The approval's revision/sha256 fields do not exist at 0f4453c, so they are not asserted here.)
use std::{
    fs,
    sync::{Arc, Mutex},
    time::Duration,
};
use symvoulio_core::{engine::*, output::OutputPackage, provider::*, *};

static ENV: Mutex<()> = Mutex::new(());

fn config(dir: &std::path::Path) -> RunConfig {
    let primary = dir.join("business.md");
    fs::write(&primary, "# Northwind\nCapital cost $4.2M. Growth 18 %. owner to be confirmed by Operations").unwrap();
    RunConfig {
        run_id: "r06-run".into(),
        name: "Northwind".into(),
        run_dir: dir.join("run"),
        author: Provider::Claude,
        objective: "Improve for executive review".into(),
        criteria: vec!["All totals must reconcile.".into()],
        policy: Policy::default(),
        primary,
        supporting: vec![],
        call_timeout: Duration::from_secs(5),
        author_model: None,
        reviewer_model: None,
    }
}
fn clients() -> (Arc<dyn ProviderClient>, Arc<dyn ProviderClient>) {
    (Arc::new(ClaudeCli), Arc::new(CodexCli))
}
fn env_on() {
    std::env::set_var("SYMVOULIO_FAKE_CLI", env!("CARGO_BIN_EXE_fake-cli"));
    for key in ["FAKE_CLI_SCENARIO", "FAKE_CLI_LOGGED_OUT", "FAKE_CLI_LOG"] {
        std::env::remove_var(key);
    }
}
fn env_off() {
    for key in ["SYMVOULIO_FAKE_CLI", "FAKE_CLI_SCENARIO", "FAKE_CLI_LOGGED_OUT", "FAKE_CLI_LOG"] {
        std::env::remove_var(key);
    }
}

#[test]
fn r06_final_is_the_committed_revision_when_a_later_revision_file_exists() {
    let _guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
    env_on();
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path());
    let run = cfg.run_dir.clone();
    let control = Arc::new(RunControl::default());
    let stopper = control.clone();
    // Stop inside the review of round 2: revision_02 is written but not committed.
    let s = start(cfg, clients(), control, move |e| {
        if let RunEvent::Phase { round: 2, role: Role::Reviewer, .. } = e {
            std::env::set_var("FAKE_CLI_SCENARIO", "slow");
            let c = stopper.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(250));
                c.stop();
            });
        }
    })
    .unwrap();
    env_off();
    assert_eq!(s.rounds_done, 1, "setup: stopped with one committed round");
    assert!(run.join("output/revisions/revision_02.md").is_file(), "setup: the uncommitted revision is on disk");
    let before = fs::read_to_string(run.join("output/final/final_document.md")).unwrap();
    assert_eq!(before, s.documents[1], "final must be the committed revision 1, not revision_02 found on disk");
    env_on();
    decide(&run, DecisionKind::Approved, "Owner", "Reviewed").unwrap();
    env_off();
    let after = fs::read_to_string(run.join("output/final/final_document.md")).unwrap();
    assert_eq!(after, s.documents[1], "after approval final must still be the committed revision 1");
}

#[test]
fn r06_orphan_revision_without_saved_state_never_becomes_final() {
    let _guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
    env_on();
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path());
    let run = cfg.run_dir.clone();
    let s = start(cfg, clients(), Arc::default(), |_| {}).unwrap();
    OutputPackage::reopen(&run, s.documents[0].clone()).write_revision(99, "Orphan").unwrap();
    decide(&run, DecisionKind::Approved, "Owner", "").unwrap();
    env_off();
    assert_eq!(
        fs::read_to_string(run.join("output/final/final_document.md")).unwrap(),
        s.documents[s.rounds_done as usize],
        "orphan revision must not become final"
    );
}
