// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! Official noninteractive CLI adapters.
use crate::{cli, process, AuthorResponse, Provider, Review, Role, Usage};
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
/// Complete UTF-8 prompt budget below the provider stdin limit.
pub const MAX_PROMPT_BYTES: usize = 9 * 1024 * 1024;
pub type CancelToken = Arc<AtomicBool>;
#[derive(Debug, Clone, Copy)]
pub enum SchemaKind {
    Review,
    AuthorResponse,
}
impl SchemaKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Review => "review",
            Self::AuthorResponse => "author-response",
        }
    }
    pub fn text(self) -> &'static str {
        match self {
            Self::Review => include_str!("../../../schemas/review.schema.json"),
            Self::AuthorResponse => include_str!("../../../schemas/author-response.schema.json"),
        }
    }
}
pub struct CallRequest {
    pub role: Role,
    pub round: u32,
    pub prompt: String,
    pub schema: SchemaKind,
    pub workdir: PathBuf,
    pub timeout: Duration,
    pub model: Option<String>,
}
#[derive(Debug)]
pub struct CallOutcome {
    pub structured: Value,
    pub model: Option<String>,
    pub session_id: Option<String>,
    pub usage: Option<Usage>,
    pub stdout_lines: Vec<String>,
    pub stderr: String,
    pub duration_s: f64,
    pub exit_code: Option<i32>,
}
#[derive(Debug, thiserror::Error)]
pub enum CallError {
    #[error("input exceeds the 9 MB UTF-8 prompt limit: {0}")]
    InputTooLarge(String),
    #[error("CLI is not installed")]
    NotInstalled,
    #[error("authentication failed: {0}")]
    AuthFailed(String),
    #[error("provider timed out")]
    Timeout,
    #[error("provider cancelled")]
    Cancelled,
    #[error("malformed structured output: {0}")]
    Malformed(String),
    #[error("process failed ({exit_code:?}): {stderr_tail}")]
    ProcessFailed {
        exit_code: Option<i32>,
        stderr_tail: String,
    },
    #[error("provider IO: {0}")]
    Io(String),
}
#[derive(Debug, Clone)]
pub struct StreamLine {
    pub raw: String,
    pub readable: Option<String>,
    pub is_stderr: bool,
}
pub trait ProviderClient: Send + Sync {
    fn provider(&self) -> Provider;
    fn call(
        &self,
        req: &CallRequest,
        sink: &mut dyn FnMut(StreamLine),
        cancel: &CancelToken,
    ) -> Result<CallOutcome, CallError>;
}
pub struct ClaudeCli;
pub struct CodexCli;
impl ProviderClient for ClaudeCli {
    fn provider(&self) -> Provider {
        Provider::Claude
    }
    fn call(
        &self,
        r: &CallRequest,
        s: &mut dyn FnMut(StreamLine),
        c: &CancelToken,
    ) -> Result<CallOutcome, CallError> {
        call(Provider::Claude, r, s, c)
    }
}
impl ProviderClient for CodexCli {
    fn provider(&self) -> Provider {
        Provider::Codex
    }
    fn call(
        &self,
        r: &CallRequest,
        s: &mut dyn FnMut(StreamLine),
        c: &CancelToken,
    ) -> Result<CallOutcome, CallError> {
        call(Provider::Codex, r, s, c)
    }
}
fn auth(text: &str, claude: bool) -> bool {
    let s = text.to_ascii_lowercase();
    [
        "login",
        "authentication",
        "not logged in",
        "unauthorized",
        "401",
    ]
    .iter()
    .any(|x| s.contains(x))
        || (claude && s.contains("api key"))
}
fn string(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_owned)
}
fn tokens(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}
fn call(
    p: Provider,
    r: &CallRequest,
    sink: &mut dyn FnMut(StreamLine),
    cancel: &CancelToken,
) -> Result<CallOutcome, CallError> {
    if r.prompt.len() > MAX_PROMPT_BYTES {
        return Err(CallError::InputTooLarge("complete prompt".into()));
    }
    let cli = cli::resolve(p).ok_or(CallError::NotInstalled)?;
    let io = |e: std::io::Error| CallError::Io(e.to_string());
    fs::create_dir_all(r.workdir.join(".symvoulio")).map_err(io)?;
    let schema_file = r
        .workdir
        .join(".symvoulio")
        .join(format!("{}.schema.json", r.schema.name()));
    // Measured 30.10 with claude 2.1.309: its --json-schema validator rejects the draft 2020-12 `$schema` URI
    // ("no schema with key or ref"). The CLIs get the schema without the `$schema`/`$id` meta keys; the public files
    // in schemas/ keep them.
    let provider_schema = provider_schema(r.schema.text());
    fs::write(&schema_file, &provider_schema).map_err(io)?;
    let mut cmd = process::command(&cli);
    cmd.current_dir(&r.workdir);
    if p == Provider::Claude {
        if cli
            .program
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"))
        {
            return Err(CallError::Io(
                "cannot pass a JSON schema through cmd.exe".into(),
            ));
        }
        // https://code.claude.com/docs/en/settings-reference (fetched 2026-10-30):
        // disableAllHooks (Boolean): "Claude Code turns off hooks".
        // permissions.blockReadsOutsideWorkingDirectories (Boolean):
        // "Claude's file tools refuse reads outside the working directories".
        // Only managed settings can disable managed hooks.
        let compact = provider_schema;
        cmd.args([
            "-p",
            "Follow the instructions given on stdin. Reply only through the structured output.",
            "--output-format",
            "stream-json",
            "--verbose",
            "--json-schema",
            &compact,
            "--tools",
            "",
            "--strict-mcp-config",
            "--mcp-config",
            r#"{"mcpServers":{}}"#,
            "--disable-slash-commands",
            "--settings",
            r#"{"disableAllHooks":true,"permissions":{"blockReadsOutsideWorkingDirectories":true}}"#,
            "--no-session-persistence",
            "--permission-mode",
            "dontAsk",
        ]);
        if let Some(m) = &r.model {
            cmd.args(["--model", m]);
        }
        // The roles judge only what the prompt gives them: keep the user's own CLAUDE.md files and auto memory
        // out of the call (code.claude.com/docs/en/env-vars). `--bare` is not an option: it ignores subscription login.
        cmd.env("CLAUDE_CODE_DISABLE_CLAUDE_MDS", "1")
            .env("CLAUDE_CODE_DISABLE_AUTO_MEMORY", "1");
    } else {
        cmd.args([
            "exec",
            "--json",
            "--skip-git-repo-check",
            "--ignore-user-config",
            "--ignore-rules",
            "--ephemeral",
            "--sandbox",
            "read-only",
            "-c",
            "approval_policy=\"never\"",
            "--output-schema",
        ])
        .arg(&schema_file)
        .arg("-C")
        .arg(&r.workdir);
        if let Some(m) = &r.model {
            cmd.args(["-m", m]);
        }
        for feature in [
            "shell_tool",
            "unified_exec",
            "hooks",
            "plugins",
            "apps",
            "browser_use",
            "computer_use",
            "in_app_browser",
        ] {
            cmd.args(["--disable", feature]);
        }
        // The prompt is read from stdin; the marker goes last, after every option.
        cmd.arg("-");
    }
    let mut model = if p == Provider::Codex {
        r.model.clone()
    } else {
        None
    };
    let mut session = None;
    let mut usage = None;
    let mut answer = None;
    let mut failure = None;
    let mut auth_failure = false;
    let captured = process::run(cmd, r.prompt.clone(), r.timeout, cancel, &mut |mut line| {
        if !line.is_stderr {
            if let Ok(v) = serde_json::from_str::<Value>(&line.raw) {
                let t = v["type"].as_str().unwrap_or("");
                if p == Provider::Claude {
                    match t {
                        "system" if v["subtype"] == "init" => {
                            model = string(&v, "model");
                            session = string(&v, "session_id");
                            line.readable = Some(format!(
                                "Session started · {}",
                                model.as_deref().unwrap_or("unknown")
                            ));
                        }
                        "system" if v["subtype"] == "api_retry" => {
                            let e = v["error"]
                                .as_str()
                                .or_else(|| v["error"]["type"].as_str())
                                .unwrap_or("");
                            auth_failure |= [
                                "authentication_failed",
                                "oauth_org_not_allowed",
                                "billing_error",
                            ]
                            .contains(&e);
                            line.readable = Some(format!("Retrying: {e}"));
                        }
                        "assistant" => {
                            if let Some(blocks) = v["message"]["content"].as_array() {
                                line.readable = blocks
                                    .iter()
                                    .find_map(|b| {
                                        string(b, "text").map(|s| s.chars().take(120).collect())
                                    })
                                    .or_else(|| {
                                        blocks.iter().find_map(|b| {
                                            string(b, "name").map(|s| format!("Using {s}"))
                                        })
                                    });
                            }
                        }
                        "result" => {
                            line.readable = Some("Completed".into());
                            if v["is_error"] == true || v.get("structured_output").is_none() {
                                let message = string(&v, "result")
                                    .unwrap_or_else(|| "missing structured_output".into());
                                failure = Some(if auth_failure || auth(&message, true) {
                                    CallError::AuthFailed(message)
                                } else {
                                    CallError::Malformed(message)
                                });
                            } else {
                                answer = v.get("structured_output").cloned();
                            }
                            if model.is_none() {
                                model = v["modelUsage"]
                                    .as_object()
                                    .and_then(|m| m.keys().next().cloned());
                            }
                            let u = &v["usage"];
                            usage = Some(Usage {
                                input_tokens: tokens(u, "input_tokens"),
                                output_tokens: tokens(u, "output_tokens"),
                                cache_read_tokens: tokens(u, "cache_read_input_tokens"),
                                cache_creation_tokens: u["cache_creation_input_tokens"].as_u64(),
                            });
                        }
                        _ => (),
                    }
                } else {
                    match t {
                        "thread.started" => session = string(&v, "thread_id"),
                        "item.completed" => {
                            let i = &v["item"];
                            match i["type"].as_str().unwrap_or("") {
                                "agent_message" => {
                                    answer =
                                        Some(Value::String(string(i, "text").unwrap_or_default()));
                                    line.readable = Some("Answer received".into());
                                }
                                "reasoning" => line.readable = Some("Thinking…".into()),
                                "command_execution" => {
                                    line.readable = Some(format!(
                                        "Ran: {}",
                                        i["command"]
                                            .as_str()
                                            .unwrap_or("")
                                            .chars()
                                            .take(80)
                                            .collect::<String>()
                                    ))
                                }
                                _ => (),
                            }
                        }
                        "turn.completed" => {
                            let u = &v["usage"];
                            usage = Some(Usage {
                                input_tokens: tokens(u, "input_tokens"),
                                output_tokens: tokens(u, "output_tokens"),
                                cache_read_tokens: tokens(u, "cached_input_tokens"),
                                cache_creation_tokens: None,
                            });
                            line.readable = Some("Completed".into());
                        }
                        "turn.failed" | "error" => {
                            let message = string(&v, "message")
                                .or_else(|| string(&v["error"], "message"))
                                .unwrap_or_else(|| v.to_string());
                            failure = Some(if auth(&message, false) {
                                CallError::AuthFailed(message)
                            } else {
                                CallError::ProcessFailed {
                                    exit_code: None,
                                    stderr_tail: message,
                                }
                            });
                        }
                        _ => (),
                    }
                }
            }
        }
        sink(line);
    })?;
    if let Some(f) = failure {
        return Err(f);
    }
    if captured.exit_code != Some(0) {
        return Err(
            if auth_failure || auth(&captured.stderr, p == Provider::Claude) {
                CallError::AuthFailed("provider authentication failed".into())
            } else {
                CallError::ProcessFailed {
                    exit_code: captured.exit_code,
                    stderr_tail: captured
                        .stderr
                        .chars()
                        .rev()
                        .take(2000)
                        .collect::<String>()
                        .chars()
                        .rev()
                        .collect(),
                }
            },
        );
    }
    let mut structured =
        answer.ok_or_else(|| CallError::Malformed("missing final answer".into()))?;
    if p == Provider::Codex {
        structured = serde_json::from_str(structured.as_str().unwrap_or(""))
            .map_err(|_| CallError::Malformed("not JSON".into()))?;
    }
    normalize_issue_ids(r.schema, &mut structured, r.round)?;
    validate_contract(r.schema, &structured)?;
    Ok(CallOutcome {
        structured,
        model,
        session_id: session,
        usage,
        stdout_lines: captured.stdout,
        stderr: captured.stderr,
        duration_s: captured.duration_s,
        exit_code: captured.exit_code,
    })
}

/// The schema as handed to a provider CLI: compact JSON without the `$schema` and `$id` meta keys.
pub fn provider_schema(text: &str) -> String {
    let mut v: Value = serde_json::from_str(text).expect("embedded schema");
    if let Some(o) = v.as_object_mut() {
        o.remove("$schema");
        o.remove("$id");
    }
    serde_json::to_string(&v).expect("schema JSON")
}

fn schema_fields() -> &'static [String] {
    static FIELDS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    fn collect(v: &Value, fields: &mut Vec<String>) {
        match v {
            Value::Object(o) => {
                if let Some(Value::Object(props)) = o.get("properties") { fields.extend(props.keys().cloned()); }
                for child in o.values() { collect(child, fields); }
            }
            Value::Array(a) => for child in a { collect(child, fields); },
            _ => (),
        }
    }
    FIELDS.get_or_init(|| {
        let mut fields = vec!["output".into()];
        for kind in [SchemaKind::Review, SchemaKind::AuthorResponse] {
            collect(&serde_json::from_str(kind.text()).expect("schema"), &mut fields);
        }
        fields
    })
}
/// Only fixed labels and schema-owned field names may reach a diagnostic.
pub fn validation_reason(detail: &str) -> Option<String> {
    if ["not JSON", "unexpected field", "finding conflicts with question id"].contains(&detail) { return Some(detail.into()); }
    for field in schema_fields() {
        for reason in [format!("missing field {field}"), format!("field {field} does not match its pattern"), format!("value not allowed for {field}"), format!("wrong type for {field}"), format!("constraint failed for {field}")] {
            if detail == reason { return Some(reason); }
        }
    }
    if regex::Regex::new(r"^issue_id does not match R[0-9]{2,}-<nnn>$").expect("regex").is_match(detail) { return Some(detail.into()); }
    None
}
/// Canonicalize only unambiguous new IDs in the requested round; canonical ledger IDs remain intact.
pub fn normalize_issue_ids(kind: SchemaKind, value: &mut Value, round: u32) -> Result<(), CallError> {
    if !matches!(kind, SchemaKind::Review) { return Ok(()); }
    let canonical = regex::Regex::new(r"^R[0-9]{2}-[0-9]{3}$").expect("regex");
    let relaxed = regex::Regex::new(r"(?i)^r\s*([0-9]{1,2})\s*-\s*([0-9]{1,3})$").expect("regex");
    if let Some(issues) = value.get_mut("issues").and_then(Value::as_array_mut) {
        for issue in issues {
            if let Some(id) = issue["issue_id"].as_str() {
                if canonical.is_match(id) { continue; }
                let normalized = relaxed.captures(id.trim()).and_then(|c| {
                    let prefix = c[1].parse::<u32>().ok()?;
                    let number = c[2].parse::<u32>().ok()?;
                    (prefix == round).then(|| format!("R{round:02}-{number:03}"))
                }).ok_or_else(|| CallError::Malformed(format!("issue_id does not match R{round:02}-<nnn>")))?;
                issue["issue_id"] = Value::String(normalized);
            }
        }
    }
    Ok(())
}
/// Validate every schema constraint and context-free semantic rule before using an answer.
pub fn validate_contract(kind: SchemaKind, value: &Value) -> Result<(), CallError> {
    let schema: Value = serde_json::from_str(kind.text()).expect("embedded schema");
    let validator = jsonschema::validator_for(&schema).expect("valid embedded schema");
    validator
        .validate(value)
        .map_err(|e| {
            use jsonschema::error::ValidationErrorKind;
            // schema_path is owned by the embedded schema; instance paths may contain provider keys.
            let field = e.schema_path.to_string().split('/').rfind(|part| schema_fields().contains(&part.to_string())).unwrap_or("output").to_owned();
            let reason = match &e.kind {
                ValidationErrorKind::Required { property } => {
                    let name = property.as_str().filter(|name| schema_fields().contains(&name.to_string())).unwrap_or("output");
                    format!("missing field {name}")
                }
                ValidationErrorKind::Pattern { .. } => format!("field {field} does not match its pattern"),
                ValidationErrorKind::Enum { .. } => format!("value not allowed for {field}"),
                ValidationErrorKind::Type { .. } => format!("wrong type for {field}"),
                ValidationErrorKind::AdditionalProperties { .. } => "unexpected field".into(),
                _ => format!("constraint failed for {field}"),
            };
            CallError::Malformed(reason)
        })?;
    match kind {
        SchemaKind::Review => {
            let review: Review = serde_json::from_value(value.clone())
                .map_err(|e| CallError::Malformed(e.to_string()))?;
            let mut ids = std::collections::BTreeSet::new();
            for issue in review.issues {
                if !ids.insert(issue.issue_id.clone()) {
                    return Err(CallError::Malformed(format!(
                        "duplicate issue id: {}",
                        issue.issue_id
                    )));
                }
            }
        }
        SchemaKind::AuthorResponse => {
            let author: AuthorResponse = serde_json::from_value(value.clone())
                .map_err(|e| CallError::Malformed(e.to_string()))?;
            for response in author.responses {
                if response.response == crate::IssueResponse::RejectedWithEvidence
                    && response.evidence.trim().is_empty()
                {
                    return Err(CallError::Malformed(format!(
                        "{}: rejected_with_evidence requires non-empty evidence",
                        response.issue_id
                    )));
                }
            }
        }
    }
    Ok(())
}
/// Validate references and coverage against the ledger before committing a round artifact.
pub fn validate_answer(
    kind: SchemaKind,
    value: &Value,
    ledger: &crate::IssueLedger,
    _round: u32,
) -> Result<(), CallError> {
    validate_contract(kind, value)?;
    match kind {
        SchemaKind::Review => {
            let review: Review = serde_json::from_value(value.clone()).expect("validated review");
            for issue in review.issues {
                if ledger
                    .human_questions()
                    .iter()
                    .any(|q| q.issue_id == issue.issue_id)
                {
                    return Err(CallError::Malformed(
                        "finding conflicts with question id".into(),
                    ));
                }
            }
        }
        SchemaKind::AuthorResponse => {
            let author: AuthorResponse =
                serde_json::from_value(value.clone()).expect("validated author");
            let mut ids = std::collections::HashSet::new();
            for response in &author.responses {
                if !ids.insert(&response.issue_id) || ledger.history(&response.issue_id).is_none() {
                    return Err(CallError::Malformed(
                        "duplicate or unknown response id".into(),
                    ));
                }
            }
            for issue in ledger.open() {
                if !author
                    .responses
                    .iter()
                    .any(|r| r.issue_id == issue.issue_id)
                {
                    return Err(CallError::Malformed(format!(
                        "missing response for open issue {}",
                        issue.issue_id
                    )));
                }
            }
        }
    }
    Ok(())
}

/// Controlled diagnostic representation: never interpolate provider-supplied strings.
pub fn controlled_summary(raw: &str) -> String {
    let Ok(v) = serde_json::from_str::<Value>(raw) else {
        return "Provider event".into();
    };
    // Claude partial messages wrap the API event; completed assistant messages do not.
    let v = if v["type"] == "stream_event" {
        &v["event"]
    } else {
        &v
    };
    let event = match v["type"].as_str().unwrap_or("") {
        "assistant" => "assistant",
        "system" => "system",
        "result" | "turn.completed" => return "Completed".into(),
        "thread.started" => "thread.started",
        "turn.started" => "turn.started",
        "item.started" => "item.started",
        "item.updated" => "item.updated",
        "item.completed" => "item.completed",
        "content_block_start" => "content_block_start",
        "content_block_delta" => "content_block_delta",
        _ => return "Provider event".into(),
    };
    let words = v
        .pointer("/message/content")
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .filter(|i| i["type"] == "text")
                .filter_map(|i| i["text"].as_str())
                .map(|t| t.split_whitespace().count())
                // A thinking/tool-only message is not a zero-word text message.
                .reduce(|total, count| total + count)
        })
        .or_else(|| {
            v.pointer("/content_block/text")
                .and_then(Value::as_str)
                .map(|t| t.split_whitespace().count())
        })
        .or_else(|| {
            v.pointer("/item/text")
                .and_then(Value::as_str)
                .map(|t| t.split_whitespace().count())
        })
        .or_else(|| {
            v.pointer("/delta/text")
                .and_then(Value::as_str)
                .map(|t| t.split_whitespace().count())
        });
    match words {
        Some(n) if n > 0 => format!("{event}: assistant text, {n} words"),
        _ => format!("{event}: provider event"),
    }
}

/// Test/evidence seam observes actual tool events before diagnostics are controlled.
pub fn is_tool_execution(raw: &str) -> bool {
    let Ok(v) = serde_json::from_str::<Value>(raw) else {
        return false;
    };
    matches!(
        v["item"]["type"].as_str(),
        Some("command_execution" | "mcp_tool_call" | "web_search" | "file_change")
    ) || v["type"] == "command_execution"
        || v.pointer("/message/content")
            .and_then(Value::as_array)
            .is_some_and(|items| items.iter().any(|i| i["type"] == "tool_use" && i["name"] != "StructuredOutput"))
        || v.get("content_block")
            .is_some_and(|b| b["type"] == "tool_use" && b["name"] != "StructuredOutput")
        || (v["type"] == "tool_use" && v["name"] != "StructuredOutput")
}

#[cfg(test)]
mod schema_tests {
    #[test]
    fn provider_schema_drops_meta_keys_only() {
        let s = super::provider_schema(include_str!("../../../schemas/review.schema.json"));
        assert!(!s.contains("$schema") && !s.contains("$id") && !s.contains('\n'));
        assert!(s.contains("\"additionalProperties\":false") && s.contains("issue_id"));
    }
}
