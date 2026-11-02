// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! Real smoke: one small review with the signed-in CLIs on this machine (spends provider quota).
//! usage: cargo run -p symvoulio-core --example smoke -- <run_dir> <document> <rounds> [claude|codex author]
use std::{io::Write, path::PathBuf, sync::{Arc, Mutex, atomic::Ordering}, time::Duration};
use symvoulio_core::{
    engine::{self, RunConfig, RunControl, RunEvent},
    provider::{ClaudeCli, CodexCli, ProviderClient},
    Policy, Provider,
};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (run_dir, doc, rounds) = (
        PathBuf::from(&a[1]),
        PathBuf::from(&a[2]),
        a[3].parse::<u32>().unwrap(),
    );
    let author = if a.get(4).map(String::as_str) == Some("codex") {
        Provider::Codex
    } else {
        Provider::Claude
    };
    let claude: Arc<dyn ProviderClient> = Arc::new(ClaudeCli);
    let codex: Arc<dyn ProviderClient> = Arc::new(CodexCli);
    let clients = if author == Provider::Claude {
        (claude, codex)
    } else {
        (codex, claude)
    };
    let policy = Policy {
        max_rounds: rounds,
        min_rounds: 1,
        ..Policy::default()
    };
    let cfg = RunConfig {
        run_id: run_dir.file_name().unwrap().to_string_lossy().into_owned(),
        name: "Smoke: short business case".into(),
        run_dir,
        author,
        objective: "Improve this document for executive review without adding unsupported claims."
            .into(),
        criteria: vec![
            "All factual claims must be supported.".into(),
            "All totals must reconcile.".into(),
        ],
        policy,
        primary: doc,
        supporting: vec![],
        call_timeout: Duration::from_secs(600),
        author_model: None,
        reviewer_model: None,
    };
    // Evidence-only raw capture: explicit path supplied by the canary, outside the product package.
    let raw = a.get(5).map(|path| Arc::new(Mutex::new(std::fs::File::create(path).expect("raw evidence file"))));
    let mut control = RunControl::default();
    control.raw_output.store(raw.is_some(), Ordering::SeqCst);
    control.raw_observer = raw.map(|file| Arc::new(move |line: &str| { writeln!(file.lock().expect("raw evidence lock"), "{line}").expect("write raw evidence"); }) as Arc<dyn Fn(&str) + Send + Sync>);
    control.tool_observer = Some(Arc::new(|| eprintln!("{{\"toolExecution\":true}}")));
    let snap = engine::start(cfg, clients, Arc::new(control), |e| match e {
        RunEvent::Line { provider, line, .. } => {
            if let Some(r) = line.readable {
                eprintln!("[{provider:?}] {r}");
            }
        }
        other => eprintln!("{other:?}"),
    })
    .expect("run");
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
        "state": snap.state, "stopReason": snap.stop_reason, "lastError": snap.last_error, "roundsDone": snap.rounds_done,
        "rounds": snap.rounds, "counts": snap.counts, "outputTree": snap.output_tree,
    })).unwrap());
}
