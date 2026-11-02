// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Cruel-tester checks for card SHIM: the Windows npm-shim launch path, end to end. The shims in
// tests/fixtures/npm_shims are the station's real npm output (claude.cmd, codex.cmd), not simplified one-liners.
// PATH holds only the temp dirs of the test (plus the folder of the real node.exe where node is needed); no real
// provider is ever called.
#![cfg(windows)]
use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};
use symvoulio_core::{cli, provider::*, Provider, Role};

static ENV: Mutex<()> = Mutex::new(());
const FAKE: &str = env!("CARGO_BIN_EXE_fake-cli");
const CLAUDE_CMD: &str = include_str!("fixtures/npm_shims/claude.cmd");
const CODEX_CMD: &str = include_str!("fixtures/npm_shims/codex.cmd");
const CLAUDE_EXE_REL: &str = "node_modules/@anthropic-ai/claude-code/bin/claude.exe";
const CODEX_JS_REL: &str = "node_modules/@openai/codex/bin/codex.js";
const KEYS: [&str; 10] = [
    "PATH",
    "PATHEXT",
    "SYMVOULIO_FAKE_CLI",
    "FAKE_CLI_AS",
    "FAKE_CLI_BIN",
    "FAKE_CLI_LOG",
    "FAKE_CLI_SCENARIO",
    "FAKE_CLI_LOGGED_OUT",
    "CLAUDE_CODE_DISABLE_CLAUDE_MDS",
    "CLAUDE_CODE_DISABLE_AUTO_MEMORY",
];

/// Serialises env use, remembers every variable it touches and restores it on drop.
struct Env {
    _guard: MutexGuard<'static, ()>,
    saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
    node_dir: PathBuf,
}
impl Env {
    fn new() -> Self {
        let guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let saved: Vec<_> = KEYS.iter().map(|k| (*k, env::var_os(k))).collect();
        let node_dir = env::split_paths(&env::var_os("PATH").unwrap_or_default())
            .find(|d| d.join("node.exe").is_file())
            .expect("node.exe must be on the PATH the tests start with (the codex stand-in and shim need node)");
        for k in KEYS {
            env::remove_var(k);
        }
        env::set_var("PATHEXT", ".COM;.EXE;.BAT;.CMD;.VBS;.JS");
        Self { _guard: guard, saved, node_dir }
    }
    fn path_only(&self, dirs: &[&Path]) {
        env::set_var("PATH", env::join_paths(dirs).unwrap());
    }
}
impl Drop for Env {
    fn drop(&mut self) {
        for (k, v) in &self.saved {
            match v {
                Some(v) => env::set_var(k, v),
                None => env::remove_var(k),
            }
        }
    }
}

/// The shim text exactly as npm writes it: CRLF line ends, whatever the checkout did to the fixture.
fn crlf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\n', "\r\n")
}
fn put(dir: &Path, rel: &str, bytes: &[u8]) -> PathBuf {
    let p = dir.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, bytes).unwrap();
    p
}
fn put_fake_exe(dir: &Path, rel: &str) -> PathBuf {
    let p = dir.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::copy(FAKE, &p).unwrap();
    p
}
fn same(a: &Path, b: &Path) -> bool {
    a == b || matches!((a.canonicalize(), b.canonicalize()), (Ok(x), Ok(y)) if x == y)
}
fn is_cmd(p: &Path) -> bool {
    p.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"))
}
fn request(dir: &Path) -> CallRequest {
    CallRequest {
        role: Role::Reviewer,
        round: 1,
        prompt: "ROUND: 1".into(),
        schema: SchemaKind::Review,
        workdir: dir.into(),
        timeout: Duration::from_secs(60),
        model: None,
    }
}
fn call(p: Provider, dir: &Path) -> Result<CallOutcome, CallError> {
    let c: Box<dyn ProviderClient> = match p {
        Provider::Claude => Box::new(ClaudeCli),
        Provider::Codex => Box::new(CodexCli),
    };
    c.call(&request(dir), &mut |_| {}, &Arc::default())
}
fn assert_review(o: &CallOutcome) {
    let issues = o.structured["issues"].as_array().unwrap_or_else(|| panic!("no parsed issues: {:?}", o.structured));
    assert!(!issues.is_empty(), "the stand-in's review answer was not parsed: {:?}", o.structured);
    assert_eq!(o.exit_code, Some(0));
}
/// The stand-in must really have run: the fake CLI appends its argv to FAKE_CLI_LOG.
fn assert_ran(log: &Path, provider: &str) {
    let text = fs::read_to_string(log).unwrap_or_default();
    let argv: Vec<Vec<String>> = text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert!(
        argv.iter().any(|a| a.get(1).map(String::as_str) == Some(provider) && a.iter().any(|s| s == "--json-schema" || s == "--output-schema")),
        "the {provider} stand-in did not run behind the shim (log: {text:?})"
    );
}

// ---------- 1: claude.cmd, verbatim ----------

#[test]
fn shim_claude_verbatim_resolves_to_the_exe_without_prefix_args() {
    let env = Env::new();
    let t = tempfile::tempdir().unwrap();
    let shim = put(t.path(), "claude.cmd", crlf(CLAUDE_CMD).as_bytes());
    let exe = put(t.path(), CLAUDE_EXE_REL, b"MZ stand-in");
    env.path_only(&[t.path()]);
    let r = cli::parse_shim(&shim);
    assert!(same(&r.program, &exe), "program is {:?}, wanted {exe:?}", r.program);
    assert!(r.prefix_args.is_empty(), "{:?}", r.prefix_args);
    assert!(!is_cmd(&r.program));
    assert!(r.shim.as_deref().is_some_and(|s| same(s, &shim)));
}

// ---------- 2: codex.cmd, verbatim ----------

#[test]
fn shim_codex_verbatim_with_local_node_uses_that_node_and_the_js_path() {
    let env = Env::new();
    let t = tempfile::tempdir().unwrap();
    let shim = put(t.path(), "codex.cmd", crlf(CODEX_CMD).as_bytes());
    let js = put(t.path(), CODEX_JS_REL, b"// stand-in");
    let node = put(t.path(), "node.exe", b"MZ local node");
    // Only an unrelated dir on PATH: the local node.exe must win without any PATH lookup.
    let empty = tempfile::tempdir().unwrap();
    env.path_only(&[empty.path()]);
    let r = cli::parse_shim(&shim);
    assert!(same(&r.program, &node), "program is {:?}, wanted the shim's own node.exe {node:?}", r.program);
    assert_eq!(r.prefix_args.len(), 1, "{:?}", r.prefix_args);
    assert!(same(Path::new(&r.prefix_args[0]), &js), "{:?}", r.prefix_args);
}

#[test]
fn shim_codex_verbatim_without_local_node_takes_node_from_path() {
    let env = Env::new();
    let t = tempfile::tempdir().unwrap();
    let shim = put(t.path(), "codex.cmd", crlf(CODEX_CMD).as_bytes());
    let js = put(t.path(), CODEX_JS_REL, b"// stand-in");
    let node_dir = env.node_dir.clone();
    env.path_only(&[&node_dir]);
    let r = cli::parse_shim(&shim);
    assert!(same(&r.program, &node_dir.join("node.exe")), "program is {:?}", r.program);
    assert!(!same(r.program.parent().unwrap(), t.path()));
    assert_eq!(r.prefix_args.len(), 1, "{:?}", r.prefix_args);
    assert!(same(Path::new(&r.prefix_args[0]), &js), "{:?}", r.prefix_args);
}

// ---------- 3: claude, end to end ----------

#[test]
fn shim_claude_end_to_end_runs_the_exe_and_parses_the_structured_output() {
    let env = Env::new();
    let t = tempfile::tempdir().unwrap();
    put(t.path(), "claude.cmd", crlf(CLAUDE_CMD).as_bytes());
    let exe = put_fake_exe(t.path(), CLAUDE_EXE_REL);
    env.path_only(&[t.path()]);
    env::set_var("FAKE_CLI_AS", "claude");
    let log = t.path().join("argv.jsonl");
    env::set_var("FAKE_CLI_LOG", &log);

    let resolved = cli::resolve(Provider::Claude).expect("claude.cmd on PATH must resolve");
    assert!(!is_cmd(&resolved.program), "still a shim: {:?}", resolved.program);
    assert!(same(&resolved.program, &exe), "{:?}", resolved.program);
    assert!(resolved.prefix_args.is_empty());

    let work = t.path().join("work");
    fs::create_dir_all(&work).unwrap();
    let out = call(Provider::Claude, &work).expect("the call through the shim must succeed");
    assert_review(&out);
    assert_ran(&log, "claude");
}

// ---------- 4: codex, end to end ----------

const CODEX_JS_STANDIN: &str = r#"// Stand-in for @openai/codex/bin/codex.js: runs the offline fake CLI as the codex role with the same arguments.
const { spawnSync } = require('child_process');
const r = spawnSync(process.env.FAKE_CLI_BIN, ['--as', 'codex', ...process.argv.slice(2)], { stdio: 'inherit' });
process.exit(r.status === null ? 1 : r.status);
"#;

#[test]
fn shim_codex_end_to_end_runs_node_on_the_js_and_parses_the_structured_output() {
    let env = Env::new();
    let t = tempfile::tempdir().unwrap();
    put(t.path(), "codex.cmd", crlf(CODEX_CMD).as_bytes());
    let js = put(t.path(), CODEX_JS_REL, CODEX_JS_STANDIN.as_bytes());
    let node_dir = env.node_dir.clone();
    env.path_only(&[t.path(), &node_dir]);
    env::set_var("FAKE_CLI_BIN", FAKE);
    let log = t.path().join("argv.jsonl");
    env::set_var("FAKE_CLI_LOG", &log);

    let resolved = cli::resolve(Provider::Codex).expect("codex.cmd on PATH must resolve");
    assert!(!is_cmd(&resolved.program), "still a shim: {:?}", resolved.program);
    assert!(same(&resolved.program, &node_dir.join("node.exe")), "{:?}", resolved.program);
    assert_eq!(resolved.prefix_args.len(), 1);
    assert!(same(Path::new(&resolved.prefix_args[0]), &js), "{:?}", resolved.prefix_args);

    let work = t.path().join("work");
    fs::create_dir_all(&work).unwrap();
    let out = call(Provider::Codex, &work).expect("the call through the shim must succeed");
    assert_review(&out);
    assert_ran(&log, "codex");
}

// ---------- 5: negative controls ----------

const GUARD: &str = "cannot pass a JSON schema through cmd.exe";

fn assert_guard(r: Result<CallOutcome, CallError>) {
    match r {
        Err(CallError::Io(m)) => assert!(m.contains(GUARD), "wrong error: {m}"),
        other => panic!("expected the guard error, got {other:?}"),
    }
}

#[test]
fn shim_with_missing_target_fails_with_the_guard_and_runs_nothing() {
    let env = Env::new();
    let t = tempfile::tempdir().unwrap();
    put(t.path(), "claude.cmd", crlf(CLAUDE_CMD).as_bytes());
    // The target exists nowhere; a decoy exe in a subfolder (not on PATH, not the shim's target) must stay unused.
    put_fake_exe(t.path(), "decoy/claude.exe");
    env.path_only(&[t.path()]);
    env::set_var("FAKE_CLI_AS", "claude");
    let log = t.path().join("argv.jsonl");
    env::set_var("FAKE_CLI_LOG", &log);

    let resolved = cli::resolve(Provider::Claude).unwrap();
    assert!(is_cmd(&resolved.program), "an unresolvable shim must stay the shim: {:?}", resolved.program);
    let work = t.path().join("work");
    fs::create_dir_all(&work).unwrap();
    assert_guard(call(Provider::Claude, &work));
    assert!(!log.exists(), "something ran: {:?}", fs::read_to_string(&log));
}

#[test]
fn shim_without_a_dp0_path_fails_with_the_guard_and_runs_nothing() {
    let env = Env::new();
    let t = tempfile::tempdir().unwrap();
    // A shim that points at a real exe by absolute path (no %dp0%), and records it if it is ever executed.
    let exe = put_fake_exe(t.path(), CLAUDE_EXE_REL);
    let text = format!(
        "@ECHO off\r\necho ran> \"%~dp0ran.marker\"\r\n\"{}\" %*\r\n",
        exe.display()
    );
    put(t.path(), "claude.cmd", text.as_bytes());
    env.path_only(&[t.path()]);
    env::set_var("FAKE_CLI_AS", "claude");
    let log = t.path().join("argv.jsonl");
    env::set_var("FAKE_CLI_LOG", &log);

    let resolved = cli::resolve(Provider::Claude).unwrap();
    assert!(is_cmd(&resolved.program), "{:?}", resolved.program);
    let work = t.path().join("work");
    fs::create_dir_all(&work).unwrap();
    assert_guard(call(Provider::Claude, &work));
    assert!(!t.path().join("ran.marker").exists(), "the shim was executed");
    assert!(!log.exists(), "the exe was executed");
}
