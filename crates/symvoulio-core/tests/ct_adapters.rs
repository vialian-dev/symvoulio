// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Cruel-tester checks for card M2-ADAPTERS: provider argv and isolation (VAL1-VAL3), detection (VAL4),
// shim resolution (VAL5), timeout/cancel (VAL6), classification (VAL7), redaction (VAL8).
// Expectations come from the card, not from the adapter bodies. The paste/keystroke/PTY values
// (VAL9-VAL12) are in src-tauri/src/ct_pty.rs.
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use symvoulio_core::{
    cli,
    detect::{detect, AuthMethod, Detection},
    provider::*,
    redact, Provider, Role,
};

static ENV: Mutex<()> = Mutex::new(());
const FAKE: &str = env!("CARGO_BIN_EXE_fake-cli");
const ENV_KEYS: [&str; 6] = [
    "SYMVOULIO_FAKE_CLI",
    "FAKE_CLI_SCENARIO",
    "FAKE_CLI_LOGGED_OUT",
    "FAKE_CLI_LOG",
    "CLAUDE_CODE_DISABLE_CLAUDE_MDS",
    "CLAUDE_CODE_DISABLE_AUTO_MEMORY",
];

struct Env {
    _guard: std::sync::MutexGuard<'static, ()>,
}
impl Env {
    /// Serialises env use and clears every variable the adapters must set themselves.
    fn new(cli: &str) -> Self {
        let guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
        for k in ENV_KEYS {
            std::env::remove_var(k);
        }
        std::env::set_var("SYMVOULIO_FAKE_CLI", cli);
        Self { _guard: guard }
    }
    fn fake() -> Self {
        Self::new(FAKE)
    }
}
impl Drop for Env {
    fn drop(&mut self) {
        for k in ENV_KEYS {
            std::env::remove_var(k);
        }
    }
}

// ---------- scripted stand-in CLI (output taken verbatim from files; ignores its arguments) ----------

fn stub(dir: &Path, stdout: &str, stderr: &str, code: i32) -> PathBuf {
    fs::write(dir.join("out.txt"), stdout).unwrap();
    fs::write(dir.join("err.txt"), stderr).unwrap();
    #[cfg(windows)]
    {
        let path = dir.join("stub.cmd");
        fs::write(
            &path,
            format!(
                "@echo off\r\nmore >nul\r\nif \"%3\"==\"--version\" (echo 9.9.9& exit /b 0)\r\ntype \"%~dp0out.txt\"\r\ntype \"%~dp0err.txt\" 1>&2\r\nexit /b {code}\r\n"
            ),
        )
        .unwrap();
        path
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("stub.sh");
        fs::write(
            &path,
            format!(
                "#!/bin/sh\ncat >/dev/null\nfor a in \"$@\"; do if [ \"$a\" = \"--version\" ]; then echo 9.9.9; exit 0; fi; done\ncat \"$(dirname \"$0\")/out.txt\"\ncat \"$(dirname \"$0\")/err.txt\" >&2\nexit {code}\n"
            ),
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }
}

fn lines(values: &[Value]) -> String {
    values.iter().map(|v| format!("{v}\n")).collect()
}

const REVIEW_1: &str = include_str!("../fixtures/fake/review-1.json");

fn review_answer() -> Value {
    serde_json::from_str(REVIEW_1).unwrap()
}

fn request(role: Role, schema: SchemaKind, dir: &Path, timeout: Duration) -> CallRequest {
    CallRequest {
        role,
        round: 1,
        prompt: match role {
            Role::Reviewer => "ROUND: 1".into(),
            Role::Author => "ROUND: 1\n<<<CURRENT_DOCUMENT\nA short document.\nCURRENT_DOCUMENT>>>\n<<<OPEN_FINDINGS\n[]\nOPEN_FINDINGS>>>".into(),
        },
        schema,
        workdir: dir.into(),
        timeout,
        model: None,
    }
}

fn client(p: Provider) -> Box<dyn ProviderClient> {
    match p {
        Provider::Claude => Box::new(ClaudeCli),
        Provider::Codex => Box::new(CodexCli),
    }
}

fn run_call(p: Provider, req: &CallRequest) -> Result<CallOutcome, CallError> {
    client(p).call(req, &mut |_| {}, &Arc::default())
}

/// Call through the scripted stub and return the classified error (the call must fail).
fn stub_call(p: Provider, stdout: &str, stderr: &str, code: i32) -> Result<CallOutcome, CallError> {
    let dir = tempfile::tempdir().unwrap();
    let path = stub(dir.path(), stdout, stderr, code);
    let _env = Env::new(path.to_str().unwrap());
    let req = request(Role::Reviewer, SchemaKind::Review, dir.path(), Duration::from_secs(20));
    run_call(p, &req)
}

// ---------- argv helpers ----------

type Argv = Vec<String>;

/// Every logged invocation (without the leading `--as <provider>`) that is not a `--version` probe.
fn logged_calls(log: &Path) -> Vec<Argv> {
    fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str::<Argv>(l).unwrap())
        .map(|a| a[2..].to_vec())
        .filter(|a| !a.iter().any(|s| s == "--version"))
        .collect()
}
fn has(a: &Argv, flag: &str) -> bool {
    a.iter().any(|s| s == flag)
}
fn pair(a: &Argv, flag: &str, value: &str) -> bool {
    a.windows(2).any(|w| w[0] == flag && w[1] == value)
}
fn value_of<'a>(a: &'a Argv, flag: &str) -> Option<&'a String> {
    a.windows(2).find(|w| w[0] == flag).map(|w| &w[1])
}
fn same_path(a: &str, b: &Path) -> bool {
    let a = Path::new(a);
    a == b || matches!((a.canonicalize(), b.canonicalize()), (Ok(x), Ok(y)) if x == y)
}
fn without_meta(mut v: Value) -> Value {
    let o = v.as_object_mut().unwrap();
    o.remove("$schema");
    o.remove("$id");
    v
}

/// One call with FAKE_CLI_LOG on; returns (result, calls).
fn logged_call(p: Provider, role: Role, schema: SchemaKind) -> (Result<CallOutcome, CallError>, Vec<Argv>, PathBuf) {
    let _env = Env::fake();
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("argv.jsonl");
    std::env::set_var("FAKE_CLI_LOG", &log);
    let work = dir.path().join("work");
    fs::create_dir_all(&work).unwrap();
    let r = run_call(p, &request(role, schema, &work, Duration::from_secs(20)));
    let calls = logged_calls(&log);
    (r, calls, work)
}

fn assert_not_isolation_refusal(r: &Result<CallOutcome, CallError>) {
    assert!(
        !matches!(r, Err(CallError::ProcessFailed { exit_code: Some(2), .. })),
        "the test double refused the call because isolation flags/env were missing: {r:?}"
    );
}

const ROLES: [(Role, SchemaKind); 2] = [(Role::Reviewer, SchemaKind::Review), (Role::Author, SchemaKind::AuthorResponse)];

// ================= VAL1: Claude argv and env (both roles: SUB1) =================

fn check_claude(a: &Argv, schema: SchemaKind) {
    assert!(has(a, "-p"), "-p missing: {a:?}");
    assert!(pair(a, "--output-format", "stream-json"), "{a:?}");
    assert!(has(a, "--verbose"), "{a:?}");
    assert!(pair(a, "--tools", ""), "--tools \"\" (empty argument) missing: {a:?}");
    assert!(has(a, "--no-session-persistence"), "{a:?}");
    assert!(pair(a, "--permission-mode", "dontAsk"), "{a:?}");
    assert!(!a.iter().any(|s| s == "bypassPermissions"), "{a:?}");
    assert!(has(a, "--strict-mcp-config"), "{a:?}");
    assert!(has(a, "--disable-slash-commands"), "{a:?}");
    assert!(!has(a, "--bare"), "--bare breaks subscription login: {a:?}");

    let mcp: Value = serde_json::from_str(value_of(a, "--mcp-config").expect("--mcp-config missing")).expect("--mcp-config is JSON");
    assert_eq!(mcp, json!({"mcpServers": {}}), "MCP server list must be empty");

    let settings: Value = serde_json::from_str(value_of(a, "--settings").expect("--settings missing")).expect("--settings is JSON");
    assert_eq!(settings["disableAllHooks"], json!(true), "{settings}");
    assert_eq!(settings["permissions"]["blockReadsOutsideWorkingDirectories"], json!(true), "{settings}");

    // The schema: one argument, valid JSON, equal to the public schema minus the two meta keys (VAL3).
    let arg = value_of(a, "--json-schema").expect("--json-schema missing");
    let given: Value = serde_json::from_str(arg).expect("--json-schema value is one JSON argument");
    let public: Value = serde_json::from_str(schema.text()).unwrap();
    assert!(given.get("$schema").is_none() && given.get("$id").is_none(), "meta keys left in the inline schema");
    assert!(!arg.contains("\"$schema\"") && !arg.contains("\"$id\""));
    assert_eq!(given, without_meta(public));
    assert_eq!(a.iter().filter(|s| s.contains("\"properties\"")).count(), 1, "schema split or repeated: {a:?}");
}

#[test]
fn test_dod1_val1_claude_argv_for_both_roles() {
    for (role, schema) in ROLES {
        let (r, calls, _) = logged_call(Provider::Claude, role, schema);
        assert_not_isolation_refusal(&r);
        assert_eq!(calls.len(), 1, "{role:?}: expected exactly one provider invocation");
        check_claude(&calls[0], schema);
    }
}

#[test]
fn test_dod1_val1_claude_reviewer_call_succeeds_only_with_env_set_by_the_adapter() {
    // The parent env has both variables removed; the test double exits 2 unless the adapter set them to "1".
    let (r, _, _) = logged_call(Provider::Claude, Role::Reviewer, SchemaKind::Review);
    let o = r.expect("reviewer call");
    assert!(o.structured.is_object());
    assert_eq!(o.exit_code, Some(0));
}

#[test]
fn test_sub1_val1_isolation_env_is_not_leaked_into_the_parent_process() {
    let _ = logged_call(Provider::Claude, Role::Reviewer, SchemaKind::Review);
    // Env::drop cleared them; a call must set them on the child only, so the parent stays clean after the call.
    let _env = Env::fake();
    let dir = tempfile::tempdir().unwrap();
    let _ = run_call(Provider::Claude, &request(Role::Reviewer, SchemaKind::Review, dir.path(), Duration::from_secs(20)));
    assert!(std::env::var("CLAUDE_CODE_DISABLE_CLAUDE_MDS").is_err());
    assert!(std::env::var("CLAUDE_CODE_DISABLE_AUTO_MEMORY").is_err());
}

// ================= VAL2: Codex argv (both roles: SUB1) =================

const DISABLED: [&str; 8] = ["shell_tool", "unified_exec", "hooks", "plugins", "apps", "browser_use", "computer_use", "in_app_browser"];

fn check_codex(a: &Argv, work: &Path) {
    assert_eq!(a.first().map(String::as_str), Some("exec"), "{a:?}");
    for f in ["--json", "--skip-git-repo-check", "--ignore-user-config", "--ignore-rules", "--ephemeral"] {
        assert!(has(a, f), "{f} missing: {a:?}");
    }
    assert!(pair(a, "--sandbox", "read-only"), "{a:?}");
    assert!(pair(a, "-c", "approval_policy=\"never\""), "{a:?}");
    assert!(
        !a.iter().any(|s| s.starts_with("approval_policy") && s != "approval_policy=\"never\""),
        "conflicting approval policy: {a:?}"
    );
    assert!(value_of(a, "--output-schema").is_some(), "--output-schema missing: {a:?}");
    let cd = value_of(a, "-C").expect("-C missing");
    assert!(same_path(cd, work), "-C {cd} is not the workdir {}", work.display());
    for f in DISABLED {
        assert!(pair(a, "--disable", f), "--disable {f} missing: {a:?}");
    }
    assert_eq!(a.last().map(String::as_str), Some("-"), "prompt marker `-` must be LAST: {a:?}");
    assert!(!has(a, "--bare") && !has(a, "--dangerously-bypass-approvals-and-sandbox") && !has(a, "--full-auto"));
}

#[test]
fn test_dod1_val2_codex_argv_for_both_roles() {
    for (role, schema) in ROLES {
        let (r, calls, work) = logged_call(Provider::Codex, role, schema);
        assert_not_isolation_refusal(&r);
        assert_eq!(calls.len(), 1, "{role:?}: expected exactly one provider invocation");
        check_codex(&calls[0], &work);
    }
}

#[test]
fn test_dod1_val2_codex_reviewer_call_succeeds_under_the_isolation_check() {
    let (r, _, _) = logged_call(Provider::Codex, Role::Reviewer, SchemaKind::Review);
    let o = r.expect("reviewer call");
    assert!(o.structured.is_object());
}

#[test]
fn test_dod1_val2_codex_prompt_marker_is_last_even_with_a_model() {
    let _env = Env::fake();
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("argv.jsonl");
    std::env::set_var("FAKE_CLI_LOG", &log);
    let mut req = request(Role::Reviewer, SchemaKind::Review, dir.path(), Duration::from_secs(20));
    req.model = Some("some-model".into());
    let _ = run_call(Provider::Codex, &req);
    let calls = logged_calls(&log);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].last().map(String::as_str), Some("-"), "{:?}", calls[0]);
}

// ================= VAL3: schema meta keys =================

#[test]
fn test_dod1_val3_codex_schema_file_has_no_schema_or_id() {
    // The schema file only matters while the child runs: capture it from the stream callback of a call that
    // the test double holds open (scenario "slow"), then let the call time out.
    for (_, schema) in ROLES {
        let _env = Env::fake();
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("argv.jsonl");
        std::env::set_var("FAKE_CLI_LOG", &log);
        std::env::set_var("FAKE_CLI_SCENARIO", "slow");
        let work = dir.path().join("work");
        fs::create_dir_all(&work).unwrap();
        let req = request(Role::Reviewer, schema, &work, Duration::from_millis(2500));
        let mut captured: Option<String> = None;
        let mut sink = |_: StreamLine| {
            if captured.is_none() {
                if let Some(call) = logged_calls(&log).first() {
                    if let Some(path) = value_of(call, "--output-schema") {
                        captured = fs::read_to_string(path).ok();
                    }
                }
            }
        };
        let r = CodexCli.call(&req, &mut sink, &Arc::default());
        assert!(matches!(r, Err(CallError::Timeout)), "{r:?}");
        let text = captured.expect("could not read the --output-schema file while the child was running");
        let given: Value = serde_json::from_str(&text).expect("schema file is JSON");
        assert!(given.get("$schema").is_none(), "$schema left in the Codex schema file");
        assert!(given.get("$id").is_none(), "$id left in the Codex schema file");
        assert!(!text.contains("\"$schema\"") && !text.contains("\"$id\""));
        let public: Value = serde_json::from_str(schema.text()).unwrap();
        assert_eq!(given, without_meta(public), "schema file must be the public schema minus the meta keys");
    }
}

#[test]
fn test_sub2_val3_provider_schema_strips_exactly_the_meta_keys() {
    for schema in [SchemaKind::Review, SchemaKind::AuthorResponse] {
        let public: Value = serde_json::from_str(schema.text()).unwrap();
        let out = provider_schema(schema.text());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert!(parsed.get("$schema").is_none() && parsed.get("$id").is_none());
        assert_eq!(parsed, without_meta(public));
        assert!(!out.contains('\n'), "inline schema must be compact");
        // idempotent
        assert_eq!(provider_schema(&out), out);
    }
    let custom = r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","$id":"https://x/y","type":"object","a":[1,2]}"#;
    let parsed: Value = serde_json::from_str(&provider_schema(custom)).unwrap();
    assert_eq!(parsed, json!({"type":"object","a":[1,2]}));
}

#[test]
fn test_dod1_val3_public_schema_files_keep_schema_and_id() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    for name in ["review.schema.json", "author-response.schema.json"] {
        let v: Value = serde_json::from_str(&fs::read_to_string(dir.join(name)).unwrap()).unwrap();
        assert!(v.get("$schema").is_some(), "{name} lost $schema");
        assert!(v.get("$id").is_some(), "{name} lost $id");
    }
    for schema in [SchemaKind::Review, SchemaKind::AuthorResponse] {
        let v: Value = serde_json::from_str(schema.text()).unwrap();
        assert!(v.get("$schema").is_some() && v.get("$id").is_some(), "SchemaKind::text() must stay the public file");
    }
}

// ================= VAL4: detection =================

fn claude_status(method: &str, logged_in: bool, code: i32) -> Detection {
    let dir = tempfile::tempdir().unwrap();
    let body = json!({
        "loggedIn": logged_in, "authMethod": method, "apiProvider": "firstParty",
        "email": "pii.person@example.org", "orgId": "org-PII-12345", "orgName": "Hidden Org Name",
        "subscriptionType": "max"
    });
    let path = stub(dir.path(), &format!("{body}\n"), "", code);
    let _env = Env::new(path.to_str().unwrap());
    detect(Provider::Claude)
}
fn codex_status(stdout: &str, stderr: &str, code: i32) -> Detection {
    let dir = tempfile::tempdir().unwrap();
    let path = stub(dir.path(), stdout, stderr, code);
    let _env = Env::new(path.to_str().unwrap());
    detect(Provider::Codex)
}
fn assert_no_pii(d: &Detection) {
    let all = format!("{d:?}{}", serde_json::to_string(d).unwrap());
    for pii in ["pii.person@example.org", "example.org", "org-PII-12345", "Hidden Org Name"] {
        assert!(!all.contains(pii), "{pii} kept in the detection result: {all}");
    }
}

#[test]
fn test_dod1_val4_claude_subscription_methods() {
    for method in ["claude.ai", "oauth_token"] {
        let d = claude_status(method, true, 0);
        assert!(d.installed);
        assert_eq!(d.connected, Some(true), "{method}");
        assert_eq!(d.method, Some(AuthMethod::Subscription), "{method}");
        assert_no_pii(&d);
    }
}

#[test]
fn test_dod1_val4_claude_api_key_methods() {
    for method in ["api_key", "api_key_helper"] {
        let d = claude_status(method, true, 0);
        assert_eq!(d.connected, Some(true), "{method}");
        assert_eq!(d.method, Some(AuthMethod::ApiKey), "{method}");
        assert_no_pii(&d);
    }
}

#[test]
fn test_dod1_val4_claude_none_is_not_connected() {
    let d = claude_status("none", false, 1);
    assert_eq!(d.connected, Some(false));
    assert_no_pii(&d);
    // Even when the exit code is 0 an authMethod of none is not a connection.
    let d = claude_status("none", false, 0);
    assert_eq!(d.connected, Some(false));
}

#[test]
fn test_dod1_val4_claude_exit_1_is_not_connected_whatever_the_json_says() {
    for method in ["claude.ai", "oauth_token", "api_key"] {
        let d = claude_status(method, true, 1);
        assert_eq!(d.connected, Some(false), "{method} with exit 1");
        assert_no_pii(&d);
    }
}

#[test]
fn test_sub3_val4_claude_error_text_does_not_keep_the_email() {
    let dir = tempfile::tempdir().unwrap();
    let path = stub(dir.path(), "", "Error for pii.person@example.org in org Hidden Org Name\n", 1);
    let _env = Env::new(path.to_str().unwrap());
    let d = detect(Provider::Claude);
    assert_eq!(d.connected, Some(false));
    let all = format!("{d:?}{}", serde_json::to_string(&d).unwrap());
    assert!(!all.contains("pii.person@example.org"), "{all}");
}

#[test]
fn test_dod1_val4_codex_chatgpt_on_stderr_is_subscription() {
    let d = codex_status("", "Logged in using ChatGPT\n", 0);
    assert_eq!(d.connected, Some(true));
    assert_eq!(d.method, Some(AuthMethod::Subscription));
    assert_no_pii(&d);
}

#[test]
fn test_dod1_val4_codex_api_key_on_stderr_is_api_key() {
    let d = codex_status("", "Logged in using an API key - sk-proj-***abcd\n", 0);
    assert_eq!(d.connected, Some(true));
    assert_eq!(d.method, Some(AuthMethod::ApiKey));
    let all = format!("{d:?}{}", serde_json::to_string(&d).unwrap());
    assert!(!all.contains("abcd"), "key fragment kept: {all}");
}

#[test]
fn test_dod1_val4_codex_nonzero_exit_is_not_connected() {
    for code in [1, 2, 127] {
        let d = codex_status("", "Not logged in\n", code);
        assert_eq!(d.connected, Some(false), "exit {code}");
    }
    // Text on stderr cannot override a failing exit code.
    let d = codex_status("", "Logged in using ChatGPT\n", 1);
    assert_eq!(d.connected, Some(false));
}

#[test]
fn test_dod1_val8_detected_version_text_is_not_treated_as_a_secret() {
    // VAL8: text without secrets is unchanged. A CLI version is not a secret.
    let _env = Env::fake();
    assert_eq!(detect(Provider::Claude).version.as_deref(), Some("2.1.309 (Claude Code)"));
    assert_eq!(detect(Provider::Codex).version.as_deref(), Some("codex-cli 0.165.0"));
}

#[test]
fn test_dod1_val4_detection_with_the_fake_has_no_email() {
    let _env = Env::fake();
    let c = detect(Provider::Claude);
    let x = detect(Provider::Codex);
    assert_eq!((c.connected, c.method), (Some(true), Some(AuthMethod::Subscription)));
    assert_eq!((x.connected, x.method), (Some(true), Some(AuthMethod::Subscription)));
    for d in [&c, &x] {
        let all = format!("{d:?}{}", serde_json::to_string(d).unwrap());
        for pii in ["fake.user@example.com", "example.com", "Fake Org"] {
            assert!(!all.contains(pii), "{pii} in {all}");
        }
    }
    std::env::set_var("FAKE_CLI_LOGGED_OUT", "1");
    for p in [Provider::Claude, Provider::Codex] {
        assert_eq!(detect(p).connected, Some(false), "{p:?}");
    }
}

// ================= VAL5: npm .cmd shims =================

fn write_shim(dir: &Path, target_rel: &str) -> PathBuf {
    let shim = dir.join("tool.cmd");
    let body = format!(
        "@ECHO off\r\nGOTO start\r\n:find_dp0\r\nSET dp0=%~dp0\r\nEXIT /b\r\n:start\r\nSETLOCAL\r\nCALL :find_dp0\r\n\r\nIF EXIST \"%dp0%\\node.exe\" (\r\n  SET \"_prog=%dp0%\\node.exe\"\r\n) ELSE (\r\n  SET \"_prog=node\"\r\n  SET PATHEXT=%PATHEXT:;.JS;=;%\r\n)\r\n\r\nendLocal & goto #_undefined_# 2>NUL || title %COMSPEC% & \"%_prog%\"  \"%dp0%\\{}\" %*\r\n",
        target_rel.replace('/', "\\")
    );
    fs::write(&shim, body).unwrap();
    shim
}

#[test]
fn test_dod1_val5_claude_cmd_resolves_to_the_real_exe() {
    let dir = tempfile::tempdir().unwrap();
    let rel = "node_modules/@anthropic-ai/claude-code/bin/claude.exe";
    let target = dir.path().join(rel);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, b"MZ").unwrap();
    let shim = write_shim(dir.path(), rel);
    let r = cli::parse_shim(&shim);
    assert_eq!(r.program, target, "claude.cmd must resolve to …\\bin\\claude.exe");
    assert!(r.prefix_args.is_empty(), "{:?}", r.prefix_args);
    assert_eq!(r.shim.as_deref(), Some(shim.as_path()));
}

#[test]
fn test_dod1_val5_codex_cmd_resolves_to_node_with_the_js_entry() {
    let dir = tempfile::tempdir().unwrap();
    let rel = "node_modules/@openai/codex/bin/codex.js";
    let target = dir.path().join(rel);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, b"// js").unwrap();
    let node = dir.path().join("node.exe");
    fs::write(&node, b"MZ").unwrap();
    let shim = write_shim(dir.path(), rel);
    let r = cli::parse_shim(&shim);
    assert_eq!(r.program, node, "codex.cmd must run node");
    assert_eq!(r.prefix_args.len(), 1, "{:?}", r.prefix_args);
    assert!(same_path(&r.prefix_args[0], &target), "codex.js expected, got {:?}", r.prefix_args);
    assert!(r.prefix_args[0].ends_with("codex.js"));
    assert_eq!(r.shim.as_deref(), Some(shim.as_path()));
}

#[test]
fn test_dod1_val5_resolved_program_is_never_a_cmd_or_bat() {
    // The JSON schema argument must not go through cmd.exe: whenever the target exists, the program is not the shim.
    let dir = tempfile::tempdir().unwrap();
    let exe_rel = "pkg/bin/claude.exe";
    let exe = dir.path().join(exe_rel);
    fs::create_dir_all(exe.parent().unwrap()).unwrap();
    fs::write(&exe, b"MZ").unwrap();
    let r = cli::parse_shim(&write_shim(dir.path(), exe_rel));
    let ext = r.program.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    assert!(ext != "cmd" && ext != "bat", "{:?}", r.program);

    let dir = tempfile::tempdir().unwrap();
    let js_rel = "pkg/bin/codex.js";
    let js = dir.path().join(js_rel);
    fs::create_dir_all(js.parent().unwrap()).unwrap();
    fs::write(&js, b"//").unwrap();
    fs::write(dir.path().join("node.exe"), b"MZ").unwrap();
    let r = cli::parse_shim(&write_shim(dir.path(), js_rel));
    let ext = r.program.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    assert!(ext != "cmd" && ext != "bat", "{:?}", r.program);
}

#[test]
fn test_dod1_val5_the_last_quoted_dp0_path_is_the_target() {
    // The shim also names node.exe through "%dp0%\node.exe" earlier on; the real target is the last path.
    let dir = tempfile::tempdir().unwrap();
    let rel = "node_modules/@anthropic-ai/claude-code/bin/claude.exe";
    let target = dir.path().join(rel);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, b"MZ").unwrap();
    fs::write(dir.path().join("node.exe"), b"MZ").unwrap();
    let r = cli::parse_shim(&write_shim(dir.path(), rel));
    assert_eq!(r.program, target);
}

// ================= VAL6: timeout and cancel (SUB5) =================

#[test]
fn test_dod1_val6_slow_call_times_out_in_time_for_both_providers() {
    for p in [Provider::Claude, Provider::Codex] {
        let _env = Env::fake();
        std::env::set_var("FAKE_CLI_SCENARIO", "slow");
        let dir = tempfile::tempdir().unwrap();
        let timeout = Duration::from_millis(500);
        let t = Instant::now();
        let r = run_call(p, &request(Role::Reviewer, SchemaKind::Review, dir.path(), timeout));
        let e = t.elapsed();
        assert!(matches!(r, Err(CallError::Timeout)), "{p:?}: {r:?}");
        assert!(e >= Duration::from_millis(450), "{p:?}: returned before the timeout: {e:?}");
        assert!(e < timeout + Duration::from_secs(3), "{p:?}: {e:?}");
    }
}

#[test]
fn test_sub5_val6_grandchild_holding_the_pipes_does_not_extend_the_timeout() {
    for p in [Provider::Claude, Provider::Codex] {
        let _env = Env::fake();
        std::env::set_var("FAKE_CLI_SCENARIO", "grandchild");
        let dir = tempfile::tempdir().unwrap();
        let timeout = Duration::from_millis(300);
        let t = Instant::now();
        let r = run_call(p, &request(Role::Reviewer, SchemaKind::Review, dir.path(), timeout));
        let e = t.elapsed();
        assert!(matches!(r, Err(CallError::Timeout)), "{p:?}: {r:?}");
        assert!(e < timeout + Duration::from_secs(3), "{p:?} waited for the grandchild: {e:?}");
    }
}

#[test]
fn test_dod1_val6_cancel_kills_the_call_within_3_seconds() {
    for p in [Provider::Claude, Provider::Codex] {
        let _env = Env::fake();
        std::env::set_var("FAKE_CLI_SCENARIO", "slow");
        let dir = tempfile::tempdir().unwrap();
        let cancel: CancelToken = Arc::new(AtomicBool::new(false));
        let fired: Arc<Mutex<Option<Instant>>> = Arc::default();
        let (c2, f2) = (cancel.clone(), fired.clone());
        let canceller = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(600));
            *f2.lock().unwrap() = Some(Instant::now());
            c2.store(true, Ordering::SeqCst);
        });
        let req = request(Role::Reviewer, SchemaKind::Review, dir.path(), Duration::from_secs(60));
        let r = client(p).call(&req, &mut |_| {}, &cancel);
        let returned = Instant::now();
        canceller.join().unwrap();
        assert!(matches!(r, Err(CallError::Cancelled)), "{p:?}: {r:?}");
        let fired_at = fired.lock().unwrap().expect("cancel fired");
        assert!(returned.duration_since(fired_at) < Duration::from_secs(3), "{p:?}: cancel took {:?}", returned.duration_since(fired_at));
    }
}

#[test]
fn test_dod1_val6_a_call_started_already_cancelled_does_not_run() {
    for p in [Provider::Claude, Provider::Codex] {
        let _env = Env::fake();
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("argv.jsonl");
        std::env::set_var("FAKE_CLI_LOG", &log);
        let cancel: CancelToken = Arc::new(AtomicBool::new(true));
        let req = request(Role::Reviewer, SchemaKind::Review, dir.path(), Duration::from_secs(20));
        let r = client(p).call(&req, &mut |_| {}, &cancel);
        assert!(matches!(r, Err(CallError::Cancelled)), "{p:?}: {r:?}");
        assert!(logged_calls(&log).is_empty(), "{p:?}: the provider was started although cancelled");
    }
}

// ================= VAL7: classification =================

fn claude_lines(extra: &[Value], result: Value) -> String {
    let mut v = vec![json!({"type":"system","subtype":"init","model":"claude-fake-1","session_id":"s"})];
    v.extend(extra.iter().cloned());
    v.push(result);
    lines(&v)
}
fn claude_err_result(text: &str) -> Value {
    json!({"type":"result","is_error":true,"result":text})
}
fn claude_ok_result(answer: Value) -> Value {
    json!({"type":"result","is_error":false,"structured_output":answer,"usage":{"input_tokens":1,"output_tokens":1},"modelUsage":{"claude-fake-1":{}}})
}
fn retry(error: &str) -> Value {
    json!({"type":"system","subtype":"api_retry","attempt":1,"error_status":401,"error":error})
}
fn is_auth(r: &Result<CallOutcome, CallError>) -> bool {
    matches!(r, Err(CallError::AuthFailed(_)))
}
fn codex_lines(events: &[Value]) -> String {
    let mut v = vec![json!({"type":"thread.started","thread_id":"t"}), json!({"type":"turn.started"})];
    v.extend(events.iter().cloned());
    lines(&v)
}

#[test]
fn test_dod1_val7_claude_api_retry_auth_errors_are_auth_failed() {
    for error in ["authentication_failed", "oauth_org_not_allowed", "billing_error"] {
        for code in [0, 1] {
            let out = claude_lines(&[retry(error)], claude_err_result("Request failed"));
            let r = stub_call(Provider::Claude, &out, "", code);
            assert!(is_auth(&r), "{error} (exit {code}) -> {r:?}");
        }
    }
}

#[test]
fn test_dod1_val7_claude_result_mentioning_login_or_authentication_is_auth_failed() {
    for text in [
        "Not logged in · Please run /login",
        "Please login again",
        "Invalid authentication credentials",
        "Authentication required",
    ] {
        for code in [0, 1] {
            let r = stub_call(Provider::Claude, &claude_lines(&[], claude_err_result(text)), "", code);
            assert!(is_auth(&r), "{text:?} (exit {code}) -> {r:?}");
        }
    }
}

#[test]
fn test_dod1_val7_claude_non_auth_retry_is_not_auth_failed() {
    for error in ["rate_limit", "server_error", "overloaded"] {
        let out = claude_lines(&[retry(error)], claude_err_result("Service is busy, try later"));
        let r = stub_call(Provider::Claude, &out, "", 1);
        assert!(!is_auth(&r), "{error} classified as AuthFailed: {r:?}");
        assert!(r.is_err());
    }
}

#[test]
fn test_dod1_val7_claude_success_mentioning_login_is_not_a_failure() {
    let chatter = json!({"type":"assistant","message":{"content":[{"type":"text","text":"Section 3 asks users to login and authenticate."}]}});
    let out = claude_lines(&[chatter], claude_ok_result(review_answer()));
    let r = stub_call(Provider::Claude, &out, "", 0);
    let o = r.expect("a valid structured_output must succeed");
    assert!(o.structured.is_object());
}

#[test]
fn test_dod1_val7_claude_auth_failed_via_the_fake_scenario() {
    let _env = Env::fake();
    std::env::set_var("FAKE_CLI_SCENARIO", "auth_fail");
    let dir = tempfile::tempdir().unwrap();
    let r = run_call(Provider::Claude, &request(Role::Reviewer, SchemaKind::Review, dir.path(), Duration::from_secs(20)));
    assert!(is_auth(&r), "{r:?}");
}

#[test]
fn test_dod1_val7_codex_auth_errors_are_auth_failed() {
    let cases = [
        json!({"type":"turn.failed","error":{"message":"401 Unauthorized"}}),
        json!({"type":"turn.failed","error":{"message":"unauthorized"}}),
        json!({"type":"turn.failed","error":{"message":"Please login again"}}),
        json!({"type":"turn.failed","error":{"message":"Not logged in"}}),
        json!({"type":"error","message":"401 Unauthorized"}),
        json!({"type":"error","message":"Not logged in"}),
        json!({"type":"error","message":"You must login first"}),
    ];
    for ev in cases {
        for code in [0, 1] {
            let r = stub_call(Provider::Codex, &codex_lines(std::slice::from_ref(&ev)), "", code);
            assert!(is_auth(&r), "{ev} (exit {code}) -> {r:?}");
        }
    }
}

#[test]
fn test_dod1_val7_codex_non_auth_failure_is_not_auth_failed() {
    for msg in ["503 service unavailable", "stream disconnected before completion", "context length exceeded"] {
        for ev in [json!({"type":"turn.failed","error":{"message":msg}}), json!({"type":"error","message":msg})] {
            let r = stub_call(Provider::Codex, &codex_lines(&[ev]), "", 1);
            assert!(!is_auth(&r), "{msg:?} classified as AuthFailed: {r:?}");
            assert!(r.is_err());
        }
    }
}

#[test]
fn test_dod1_val7_codex_auth_failed_via_the_fake_scenario() {
    let _env = Env::fake();
    std::env::set_var("FAKE_CLI_SCENARIO", "auth_fail");
    let dir = tempfile::tempdir().unwrap();
    let r = run_call(Provider::Codex, &request(Role::Reviewer, SchemaKind::Review, dir.path(), Duration::from_secs(20)));
    assert!(is_auth(&r), "{r:?}");
}

#[test]
fn test_dod1_val7_claude_missing_structured_output_is_malformed() {
    let ok_without = json!({"type":"result","is_error":false,"result":"done, no structured output"});
    let r = stub_call(Provider::Claude, &claude_lines(&[], ok_without), "", 0);
    assert!(matches!(r, Err(CallError::Malformed(_))), "{r:?}");
}

#[test]
fn test_dod1_val7_claude_structured_output_not_matching_the_schema_is_malformed() {
    for bad in [json!({"unexpected": true}), json!("a string"), json!([]), json!({}), Value::Null] {
        let r = stub_call(Provider::Claude, &claude_lines(&[], claude_ok_result(bad.clone())), "", 0);
        assert!(matches!(r, Err(CallError::Malformed(_))), "{bad} -> {r:?}");
    }
    // The review with one required key removed must be refused.
    let mut broken = review_answer();
    broken.as_object_mut().unwrap().remove("criteria_results");
    let r = stub_call(Provider::Claude, &claude_lines(&[], claude_ok_result(broken)), "", 0);
    assert!(matches!(r, Err(CallError::Malformed(_))), "{r:?}");
}

#[test]
fn test_dod1_val7_codex_unusable_final_message_is_malformed() {
    let msg = |text: String| json!({"type":"item.completed","item":{"type":"agent_message","text":text}});
    let done = json!({"type":"turn.completed","usage":{"input_tokens":1,"cached_input_tokens":0,"output_tokens":1}});
    for text in ["not json at all".to_string(), "{\"unexpected\":true}".to_string(), "[]".to_string(), String::new()] {
        let r = stub_call(Provider::Codex, &codex_lines(&[msg(text.clone()), done.clone()]), "", 0);
        assert!(matches!(r, Err(CallError::Malformed(_))), "{text:?} -> {r:?}");
    }
    // No agent message at all.
    let r = stub_call(Provider::Codex, &codex_lines(&[done]), "", 0);
    assert!(matches!(r, Err(CallError::Malformed(_))), "{r:?}");
}

#[test]
fn test_dod1_val7_malformed_via_the_fake_scenario_for_both_providers() {
    for p in [Provider::Claude, Provider::Codex] {
        let _env = Env::fake();
        std::env::set_var("FAKE_CLI_SCENARIO", "malformed");
        let dir = tempfile::tempdir().unwrap();
        let r = run_call(p, &request(Role::Reviewer, SchemaKind::Review, dir.path(), Duration::from_secs(20)));
        assert!(matches!(r, Err(CallError::Malformed(_))), "{p:?}: {r:?}");
    }
}

#[test]
fn test_dod1_val7_codex_valid_answer_with_auth_words_in_reasoning_succeeds() {
    let reasoning = json!({"type":"item.completed","item":{"type":"reasoning","text":"The document discusses login and unauthorized access."}});
    let msg = json!({"type":"item.completed","item":{"type":"agent_message","text":review_answer().to_string()}});
    let done = json!({"type":"turn.completed","usage":{"input_tokens":1,"cached_input_tokens":0,"output_tokens":1}});
    let r = stub_call(Provider::Codex, &codex_lines(&[reasoning, msg, done]), "", 0);
    let o = r.expect("valid answer");
    assert!(o.structured.is_object());
}

// ================= VAL8: redaction =================

const R: &str = "[REDACTED]";

#[test]
fn test_dod1_val8_sk_keys_become_redacted() {
    assert_eq!(redact("sk-testSyntheticKey"), R);
    assert_eq!(redact("sk-ant-testSyntheticKey"), R);
    assert_eq!(redact("sk-ant-api03-AbCdEf_123-xyz"), R);
    assert_eq!(redact("key: sk-abcdefghijklmnopqrstuvwxyz0123 end"), format!("key: {R} end"));
    assert_eq!(redact("a sk-one b sk-ant-two c"), format!("a {R} b {R} c"));
}

#[test]
fn test_dod1_val8_bearer_tokens_become_redacted() {
    for text in ["Bearer syntheticBearerToken", "Authorization: Bearer abc.def-ghi_123", "bearer lowercaseToken123"] {
        let out = redact(text);
        assert!(out.contains(R), "{text:?} -> {out:?}");
        assert!(!out.contains("syntheticBearerToken") && !out.contains("abc.def-ghi_123") && !out.contains("lowercaseToken123"), "{out:?}");
    }
    assert_eq!(redact("Bearer syntheticBearerToken").trim_start_matches("Bearer "), R);
}

#[test]
fn test_dod1_val8_jwt_like_tokens_become_redacted() {
    let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0In0.syntheticSignature";
    assert_eq!(redact(jwt), R);
    assert_eq!(redact(&format!("token={jwt};")), format!("token={R};"));
    assert_eq!(redact(&format!("a {jwt} b {jwt} c")), format!("a {R} b {R} c"));
}

#[test]
fn test_dod1_val8_long_runs_boundary_is_32() {
    let c31 = "A".repeat(31);
    assert_eq!(redact(&c31), c31, "31 chars is not a secret");
    assert_eq!(redact(&format!("x {c31} y")), format!("x {c31} y"));
    for n in [32usize, 33, 64, 200] {
        assert_eq!(redact(&"A".repeat(n)), R, "run of {n}");
    }
    assert_eq!(redact(&format!("pre {} post", "a1".repeat(16))), format!("pre {R} post"));
    // Every character class of the run: letters, digits, + / = _ -
    for ch in ['+', '/', '=', '_', '-', '0', 'z', 'Z'] {
        assert_eq!(redact(&ch.to_string().repeat(32)), R, "run of {ch:?}");
    }
    assert_eq!(redact("QWxhZGRpbjpvcGVuIHNlc2FtZVN5bnRoZXRpYw=="), R);
    assert_eq!(redact("ABCDEF0123456789ABCDEF0123456789"), R);
}

#[test]
fn test_dod1_val8_json_secret_values_become_redacted() {
    for key in ["api_key", "access_token", "refresh_token", "password"] {
        let out = redact(&format!("{{\"{key}\":\"short1\"}}"));
        assert_eq!(out, format!("{{\"{key}\":\"{R}\"}}"), "{key}");
    }
    let all = r#"{"api_key":"shortApiSecret","access_token":"shortAccessSecret","refresh_token":"shortRefreshSecret","password":"shortPassword"}"#;
    let out = redact(all);
    for s in ["shortApiSecret", "shortAccessSecret", "shortRefreshSecret", "shortPassword"] {
        assert!(!out.contains(s), "{s} survived: {out}");
    }
    assert_eq!(out.matches(R).count(), 4, "{out}");
    // keys stay, only values go
    for key in ["api_key", "access_token", "refresh_token", "password"] {
        assert!(out.contains(&format!("\"{key}\"")), "{out}");
    }
}

#[test]
fn test_dod1_val8_json_secret_values_with_spaces_escapes_and_unicode() {
    assert_eq!(redact(r#"{"password" : "p a s s"}"#), format!(r#"{{"password" : "{R}"}}"#));
    assert_eq!(redact(r#"{"password":"ab\"cd"}"#), format!(r#"{{"password":"{R}"}}"#));
    assert_eq!(redact(r#"{"refresh_token":"токен"}"#), format!(r#"{{"refresh_token":"{R}"}}"#));
    assert!(redact(r#"{"api_key":""}"#).contains("api_key"));
    // The neighbours of a secret are kept.
    assert_eq!(redact(r#"{"user":"bob","password":"hunter2","note":"ok"}"#), format!(r#"{{"user":"bob","password":"{R}","note":"ok"}}"#));
}

#[test]
fn test_dod1_val8_secrets_inside_a_json_string_are_redacted() {
    // A provider event whose text field embeds the credentials as escaped JSON.
    let inner = r#"{"api_key":"embeddedSecret1","password":"embeddedSecret2"}"#;
    let outer = json!({"type":"result","result": inner}).to_string();
    let out = redact(&outer);
    assert!(!out.contains("embeddedSecret1") && !out.contains("embeddedSecret2"), "{out}");
}

#[test]
fn test_dod1_val8_text_without_secrets_is_unchanged() {
    for text in [
        "",
        " ",
        "The quick brown fox jumps over the lazy dog.",
        "Capital cost $4.2M. Growth 18 %. owner to be confirmed by Operations",
        "Привет, мир — бюджет 4,2 млн ₽",
        "line one\r\nline two\r\n\ttabbed\n",
        "{\"user\":\"bob\",\"note\":\"hello\"}",
        "C:\\Users\\someone\\Documents\\report.docx",
        "short-token and under_score and a+b=c",
        "A".repeat(31).as_str(),
    ] {
        assert_eq!(redact(text), text, "ordinary text changed");
    }
}

#[test]
fn test_dod1_val8_version_strings_are_secret_free_text() {
    for text in ["2.1.309 (Claude Code)", "codex-cli 0.165.0"] {
        assert_eq!(redact(text), text, "a CLI version is not a secret");
    }
}

#[test]
fn test_dod1_val8_redaction_keeps_surrounding_multibyte_text_intact() {
    assert_eq!(redact("до sk-testSyntheticKey после"), format!("до {R} после"));
    assert!(redact("日本語 Bearer abc123 終わり").starts_with("日本語"));
    assert!(redact("日本語 Bearer abc123 終わり").ends_with("終わり"));
}

#[test]
fn test_dod1_val8_a_line_mixing_every_secret_kind_is_cleaned() {
    let line = r#"sk-testSyntheticKey sk-ant-testSyntheticKey Bearer syntheticBearerToken eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0In0.syntheticSignature {"api_key":"shortApiSecret","access_token":"shortAccessSecret","refresh_token":"shortRefreshSecret","password":"shortPassword"} ABCDEF0123456789ABCDEF0123456789 QWxhZGRpbjpvcGVuIHNlc2FtZVN5bnRoZXRpYw=="#;
    let out = redact(line);
    for s in [
        "sk-testSyntheticKey",
        "sk-ant-testSyntheticKey",
        "syntheticBearerToken",
        "syntheticSignature",
        "shortApiSecret",
        "shortAccessSecret",
        "shortRefreshSecret",
        "shortPassword",
        "ABCDEF0123456789ABCDEF0123456789",
        "QWxhZGRpbjpvcGVuIHNlc2FtZVN5bnRoZXRpYw==",
    ] {
        assert!(!out.contains(s), "{s} survived: {out}");
    }
    assert_eq!(redact(&out), out, "idempotent on the cleaned line");
}
