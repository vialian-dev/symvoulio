// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! Deterministic offline provider emulator. Never invokes a real provider.
use serde_json::{json, Value};
use std::{
    env, fs,
    io::{self, Read, Write},
    thread,
    time::Duration,
};
fn emit(v: Value) {
    println!("{v}");
    io::stdout().flush().unwrap();
}
#[cfg(debug_assertions)]
fn install_shell() {
    use std::io::BufRead;
    if env::var_os("FAKE_CLI_SUPPRESS_MARKER").is_none() { print!("\x1b]133;D;{};0\x07", env::var("SYMVOULIO_PROMPT_TOKEN").unwrap()); }
    print!("PS fake> "); io::stdout().flush().unwrap();
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let panel = env::var("SYMVOULIO_PANEL").unwrap_or_default();
        let is_install = (panel == "claude" && line.contains("claude.ai/install.")) || (panel == "codex" && line.contains("chatgpt.com/codex/install."));
        let mut code = 0;
        if is_install {
            let root = std::path::PathBuf::from(env::var_os("SYMVOULIO_DATA_DIR").unwrap());
            let mut started = fs::OpenOptions::new().create(true).append(true).open(root.join("fake_installer_started")).unwrap();
            writeln!(started, "{panel}").unwrap();
            // CT R67: a slow installer (still running after the app's 15 s fallback).
            if let Some(ms) = env::var("FAKE_CLI_INSTALL_DELAY_MS").ok().and_then(|v| v.parse::<u64>().ok()) { thread::sleep(Duration::from_millis(ms)); }
            code = env::var("FAKE_CLI_INSTALL_EXIT").ok().and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
            if code == 0 && env::var("FAKE_CLI_INSTALL_NOOP").as_deref() != Ok("1") {
                fs::write(root.join(format!("fake_installed_{panel}")), env::var("FAKE_CLI_INSTALL_TARGET_VERSION").unwrap_or_else(|_| "9.8.7".into())).unwrap();
                // CT R69: a native installer changes the USER PATH (a child process cannot touch this shell's own PATH).
                let bin = root.join("fake_bin");
                fs::create_dir_all(&bin).unwrap();
                if env::var_os("FAKE_CLI_INSTALL_NO_PATH").is_some() {
                    let executable = format!("{panel}{}", env::consts::EXE_SUFFIX);
                    fs::copy(env::current_exe().unwrap(), bin.join(executable)).unwrap();
                }
                let stand_in = env::var_os("SYMVOULIO_USER_PATH_FILE").map(std::path::PathBuf::from).unwrap_or_else(|| root.join("fake_user_path"));
                if env::var_os("FAKE_CLI_INSTALL_NO_PATH").is_none() { fs::write(stand_in, bin.to_string_lossy().as_bytes()).unwrap(); }
            }
            // CT R66: FAKE_CLI_INSTALL_RAW=1 emits the last line UNREDACTED, like a real installer would.
            let last = env::var("FAKE_CLI_INSTALL_LAST_LINE").unwrap_or_else(|_| "Installer finished".into());
            println!("{last}");
        } else if let Some(word) = line.split_whitespace().next().filter(|w| *w == "claude" || *w == "codex") {
            // CT R69: this shell resolves a provider command only if ITS OWN PATH lists the installer's directory.
            let root = std::path::PathBuf::from(env::var_os("SYMVOULIO_DATA_DIR").unwrap());
            let bin = root.join("fake_bin").to_string_lossy().to_string();
            let on_path = env::var("PATH").unwrap_or_default().contains(&bin);
            match fs::read_to_string(root.join(format!("fake_installed_{word}"))) {
                Ok(v) if on_path => println!("{word}-fake {}", v.trim()),
                _ => { println!("{word}: command not found"); code = 127; }
            }
        }
        if env::var_os("FAKE_CLI_SUPPRESS_MARKER").is_none() { print!("\x1b]133;D;{};{code}\x07", env::var("SYMVOULIO_PROMPT_TOKEN").unwrap()); }
        print!("PS fake> "); io::stdout().flush().unwrap();
    }
}
fn main() {
    #[cfg(debug_assertions)]
    if env::args().any(|a| a == "--install-shell") { install_shell(); return; }
    let mut args: Vec<String> = env::args().skip(1).collect();
    // Opt-in test knob: FAKE_CLI_AS=claude|codex stands in for the `--as <provider>` prefix the engine adds, so the
    // binary can be installed as a real `claude.exe` behind an npm shim (no prefix args there).
    if let Ok(role) = env::var("FAKE_CLI_AS") {
        args.splice(0..0, ["--as".to_string(), role]);
    }
    if args.first().is_some_and(|a| a == "--pipe-holder") {
        thread::sleep(Duration::from_secs(5));
        return;
    }
    if let Ok(path) = env::var("FAKE_CLI_LOG") {
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        writeln!(f, "{}", serde_json::to_string(&args).unwrap()).unwrap();
    }
    let claude = args.get(1).is_some_and(|s| s == "claude");
    let a = &args[2..];
    #[cfg(debug_assertions)]
    if env::var("FAKE_CLI_ABSENT").is_ok_and(|v| v.split(',').any(|p| p == if claude { "claude" } else { "codex" })) {
        let root = std::path::PathBuf::from(env::var_os("SYMVOULIO_DATA_DIR").unwrap_or_default());
        let version = fs::read_to_string(root.join(format!("fake_installed_{}", if claude { "claude" } else { "codex" })));
        if a.iter().any(|s| s == "--version") {
            match version { Ok(v) => { println!("{v}"); return }, Err(_) => std::process::exit(127) }
        }
    }
    if a.iter().any(|s| s == "--version") {
        println!(
            "{}",
            if claude {
                "2.1.309 (Claude Code)"
            } else {
                "codex-cli 0.165.0"
            }
        );
        return;
    }
    if a.first().is_some_and(|s| s == "auth" || s == "login") {
        let out = env::var("FAKE_CLI_LOGGED_OUT").is_ok_and(|s| s == "1");
        if claude {
            if out {
                emit(json!({"loggedIn":false,"authMethod":"none"}));
            } else {
                emit(
                    json!({"loggedIn":true,"authMethod":"claude.ai","apiProvider":"firstParty","analyticsDisabled":true,"projectsDirectory":"fake","configDirectory":"fake","email":"fake.user@example.com","orgId":"fake","orgName":"Fake Org","subscriptionType":"max"}),
                );
            }
        } else {
            eprintln!(
                "{}",
                if out {
                    "Not logged in"
                } else {
                    "Logged in using ChatGPT"
                }
            );
        }
        std::process::exit(if out { 1 } else { 0 });
    }
    let has = |flag: &str| a.iter().any(|arg| arg == flag);
    let pair = |flag: &str, value: &str| a.windows(2).any(|w| w[0] == flag && w[1] == value);
    let isolated = if claude {
        has("--strict-mcp-config") && has("--disable-slash-commands")
            && pair("--mcp-config", r#"{"mcpServers":{}}"#) && pair("--tools", "")
            && a.windows(2).find(|w| w[0] == "--settings").and_then(|w| serde_json::from_str::<Value>(&w[1]).ok()).is_some_and(|v| v == json!({"disableAllHooks":true,"permissions":{"blockReadsOutsideWorkingDirectories":true}}))
            && env::var("CLAUDE_CODE_DISABLE_CLAUDE_MDS").as_deref() == Ok("1")
            && env::var("CLAUDE_CODE_DISABLE_AUTO_MEMORY").as_deref() == Ok("1")
    } else {
        ["--ignore-user-config", "--ignore-rules", "--ephemeral"]
            .iter()
            .all(|f| has(f))
            && [
                "shell_tool",
                "unified_exec",
                "hooks",
                "plugins",
                "apps",
                "browser_use",
                "computer_use",
                "in_app_browser",
            ]
            .iter()
            .all(|f| pair("--disable", f))
            && pair("--sandbox", "read-only")
    };
    if !isolated {
        eprintln!("isolation flags missing");
        std::process::exit(2);
    }
    let mut prompt = String::new();
    io::stdin().read_to_string(&mut prompt).unwrap();
    // Opt-in test knob: FAKE_CLI_STDIN_LOG appends each call's prompt as one JSON line.
    if let Ok(path) = env::var("FAKE_CLI_STDIN_LOG") {
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        writeln!(f, "{}", serde_json::to_string(&prompt).unwrap()).unwrap();
    }
    let mut scenario = env::var("FAKE_CLI_SCENARIO").unwrap_or_default();
    // FAKE_CLI_SCENARIO_AUTHOR / FAKE_CLI_SCENARIO_REVIEWER override the scenario for one role only.
    let author_override = env::var("FAKE_CLI_SCENARIO_AUTHOR").ok().filter(|s| !s.is_empty());
    let reviewer_override = env::var("FAKE_CLI_SCENARIO_REVIEWER").ok().filter(|s| !s.is_empty());
    if author_override.is_some() || reviewer_override.is_some() {
        let early = if claude {
            a.windows(2).find(|w| w[0] == "--json-schema").map(|w| w[1].clone())
        } else {
            a.windows(2)
                .find(|w| w[0] == "--output-schema")
                .and_then(|w| fs::read_to_string(&w[1]).ok())
        }
        .unwrap_or_default();
        let role_override = if early.contains("revised_document") { author_override } else { reviewer_override };
        if let Some(v) = role_override {
            scenario = v;
        }
    }
    // Harness guard (L55): record which scenario this call actually ran, next to FAKE_CLI_LOG, so a test can
    // assert that its named scenario took effect instead of silently running the default.
    if let Ok(path) = env::var("FAKE_CLI_LOG") {
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(format!("{path}.scenarios"))
            .unwrap();
        writeln!(f, "{}", serde_json::to_string(&scenario).unwrap()).unwrap();
    }
    let prompt_round = prompt
        .lines()
        .find_map(|s| s.strip_prefix("ROUND: "))
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1)
        .max(1);
    if claude {
        emit(
            json!({"type":"system","subtype":"init","model":"claude-fake-1","session_id":"fake-session"}),
        );
    } else {
        emit(json!({"type":"thread.started","thread_id":"fake-thread"}));
    }
    // Opt-in: FAKE_CLI_LIVE_LINES (a JSON array of strings) is printed as live output before the normal answer.
    // With the scenario `live_error` the lines are also written to stderr and the call then fails carrying them.
    if let Some(lines) = env::var("FAKE_CLI_LIVE_LINES")
        .ok()
        .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
    {
        for l in &lines {
            if claude {
                emit(json!({"type":"assistant","message":{"content":[{"type":"text","text":l}]}}));
            } else {
                emit(json!({"type":"item.completed","item":{"type":"reasoning","text":l}}));
            }
            if scenario == "live_error" {
                eprintln!("{l}");
            }
        }
        if scenario == "live_error" {
            let joined = lines.join(" ");
            if claude {
                emit(json!({"type":"result","is_error":true,"result":joined}));
            } else {
                emit(json!({"type":"turn.failed","error":{"message":joined}}));
            }
            return;
        }
    }
    if scenario == "secrets" {
        let secrets = r#"sk-testSyntheticKey sk-ant-testSyntheticKey Bearer syntheticBearerToken eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0In0.syntheticSignature {"api_key":"shortApiSecret","access_token":"shortAccessSecret","refresh_token":"shortRefreshSecret","password":"shortPassword"} ABCDEF0123456789ABCDEF0123456789 QWxhZGRpbjpvcGVuIHNlc2FtZVN5bnRoZXRpYw=="#;
        emit(json!({"type":"assistant","message":{"content":[{"type":"text","text":secrets}]}}));
        eprintln!("{secrets}");
        emit(json!({"type":"result", "is_error":true, "result":secrets}));
        return;
    }
    if scenario == "crash" {
        std::process::exit(3);
    }
    if !claude {
        emit(json!({"type":"turn.started"}));
    }
    if scenario == "grandchild" {
        let mut child = std::process::Command::new(env::current_exe().unwrap());
        child
            .arg("--pipe-holder")
            .stdin(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            child.creation_flags(0x08000000);
        }
        // Intentionally never waited for: the grandchild keeps our stdout open after we exit (R19 test).
        #[allow(clippy::zombie_processes)]
        let _child = child.spawn().unwrap();
        // Exit first: the descendant retains the inherited pipes after its parent exits.
        return;
    }
    // FAKE_CLI_SLOW_ROUND=<n> restricts the slow scenario to that round.
    let slow_round = env::var("FAKE_CLI_SLOW_ROUND").ok().and_then(|s| s.parse::<usize>().ok());
    if scenario == "slow" && slow_round.is_none_or(|n| n == prompt_round) {
        thread::sleep(Duration::from_secs(30));
    }
    // FAKE_CLI_DELAY_MS=<ms> delays every answer (a call that takes a while, then succeeds).
    if let Some(ms) = env::var("FAKE_CLI_DELAY_MS").ok().and_then(|s| s.parse::<u64>().ok()) {
        thread::sleep(Duration::from_millis(ms));
    }
    if scenario == "auth_fail" {
        eprintln!("authentication failed: reconnect fake provider");
        if claude {
            emit(json!({"type":"system","subtype":"api_retry","error":"authentication_failed"}));
            emit(json!({"type":"result","is_error":true,"result":"authentication failed"}));
        } else {
            emit(json!({"type":"turn.failed","error":{"message":"401 unauthorized"}}));
        }
        return;
    }
    let schema = if claude {
        a.windows(2).find(|w| w[0] == "--json-schema").unwrap()[1].clone()
    } else {
        fs::read_to_string(&a.windows(2).find(|w| w[0] == "--output-schema").unwrap()[1]).unwrap()
    };
    let author = schema.contains("revised_document");
    let round = prompt
        .lines()
        .find_map(|s| s.strip_prefix("ROUND: "))
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1)
        .max(1);
    let n = round.min(4);
    let reviews = [
        include_str!("../../fixtures/fake/review-1.json"),
        include_str!("../../fixtures/fake/review-2.json"),
        include_str!("../../fixtures/fake/review-3.json"),
        include_str!("../../fixtures/fake/review-4.json"),
    ];
    let authors = [
        include_str!("../../fixtures/fake/author-1.json"),
        include_str!("../../fixtures/fake/author-2.json"),
        include_str!("../../fixtures/fake/author-3.json"),
        include_str!("../../fixtures/fake/author-4.json"),
    ];
    let edits = [
        include_str!("../../fixtures/fake/edit-1.json"),
        include_str!("../../fixtures/fake/edit-2.json"),
        include_str!("../../fixtures/fake/edit-3.json"),
        include_str!("../../fixtures/fake/edit-4.json"),
    ];
    let mut answer: Value = serde_json::from_str(if author {
        authors[n - 1]
    } else {
        reviews[n - 1]
    })
    .unwrap();
    if author && scenario == "human_question" {
        // Opt-in: FAKE_CLI_QUESTIONS_BY_ROUND (a JSON array of strings) gives the question text of round n as entry n-1
        // (the last entry repeats); the id stays R01-099 in every round, so a later round reuses an earlier question's id.
        let text = env::var("FAKE_CLI_QUESTIONS_BY_ROUND")
            .ok()
            .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
            .and_then(|v| v.get(round - 1).or(v.last()).cloned())
            .unwrap_or_else(|| "Who owns the capital budget?".to_string());
        answer["human_decisions_required"] = json!([{"issue_id":"R01-099", "question":text}]);
    }
    // Opt-in (author only): FAKE_CLI_AUTHOR_QUESTIONS (a JSON array of arrays of {issue_id, question}) gives the author's
    // human_decisions_required of round n as entry n-1, whatever the scenario; a round without an entry is untouched.
    if author {
        if let Some(qs) = env::var("FAKE_CLI_AUTHOR_QUESTIONS")
            .ok()
            .and_then(|raw| serde_json::from_str::<Vec<Value>>(&raw).ok())
            .and_then(|v| v.get(round - 1).cloned())
        {
            answer["human_decisions_required"] = qs;
        }
    }
    if author {
        let doc = prompt
            .split("<<<CURRENT_DOCUMENT\n")
            .nth(1)
            .unwrap_or("")
            .split("\nCURRENT_DOCUMENT>>>")
            .next()
            .unwrap_or("");
        let edit: Vec<String> = serde_json::from_str(edits[n - 1]).unwrap();
        let changed = if scenario == "rewrite" {
            // The author replaces every word of the document (headings keep their # marks and the line structure).
            let mut i = 0usize;
            doc.split('\n')
                .map(|line| {
                    line.split(' ')
                        .map(|w| {
                            if w.is_empty() || w.chars().all(|c| c == '#') {
                                w.to_string()
                            } else {
                                i += 1;
                                format!("rw{i}")
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect::<Vec<_>>()
                .join("\n")
        } else if doc.contains(&edit[0]) {
            doc.replacen(&edit[0], &edit[1], 1)
        } else {
            format!("{doc}\n{}", answer["changes_made"][0].as_str().unwrap())
        };
        answer["revised_document"] = json!(format!("{changed}\n<!-- revision {round} -->"));
        let open = prompt
            .split("<<<OPEN_FINDINGS\n")
            .nth(1)
            .unwrap_or("")
            .split("\nOPEN_FINDINGS>>>")
            .next()
            .unwrap_or("[]");
        let ids: Vec<Value> = serde_json::from_str(open).unwrap_or_default();
        let responses = answer["responses"].as_array_mut().unwrap();
        responses.retain(|r| ids.iter().any(|i| i["issue_id"] == r["issue_id"]));
        for i in ids {
            if !responses.iter().any(|r| r["issue_id"] == i["issue_id"]) {
                responses.push(json!({"issue_id":i["issue_id"],"response":"human_decision_required","explanation":"Human review required.","evidence":i["evidence"]}));
            }
        }
    }
    if !author && scenario == "high_human" {
        // Two High findings that need a human decision; the Critical one is downgraded so only the Highs gate the run.
        for issue in answer["issues"].as_array_mut().unwrap() {
            match issue["issue_id"].as_str() {
                Some("R01-001") => issue["severity"] = json!("medium"),
                Some("R01-002") | Some("R01-003") => issue["human_decision_required"] = json!(true),
                _ => {}
            }
        }
    }
    if !author && scenario == "duplicate_id" {
        let duplicate = answer["issues"][0].clone();
        answer["issues"].as_array_mut().unwrap().push(duplicate);
    }
    if !author && scenario == "bad_id" {
        answer["issues"][0]["issue_id"] = json!("BAD");
    }
    // Opt-in (round 1 reviewer only): FAKE_CLI_REVIEWER_ISSUE_ID replaces the findings with ONE probe finding carrying
    // that issue_id; FAKE_CLI_REVIEWER_SEVERITY sets the severity of the first finding. Defaults are untouched.
    if !author && round == 1 {
        if let Ok(id) = env::var("FAKE_CLI_REVIEWER_ISSUE_ID") {
            let mut probe = answer["issues"][0].clone();
            probe["issue_id"] = json!(id);
            probe["finding"] = json!("Probe finding for the issue id format");
            answer["issues"] = json!([probe]);
        }
        if let Ok(severity) = env::var("FAKE_CLI_REVIEWER_SEVERITY") {
            answer["issues"][0]["severity"] = json!(severity);
        }
    }
    // Opt-in (round 1 reviewer only): FAKE_CLI_REVIEWER_FILE names a JSON file whose content IS the reviewer's answer.
    if !author && round == 1 {
        if let Ok(path) = env::var("FAKE_CLI_REVIEWER_FILE") {
            answer = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        }
    }
    if !author && scenario == "unknown_resolved" {
        answer["issues"][0]["status"] = json!("resolved");
    }
    if author && round == 2 && scenario == "missing_response" {
        answer["responses"] = json!([]);
    }
    if author && round == 2 && scenario == "empty_evidence" {
        answer["responses"][0]["response"] = json!("rejected_with_evidence");
        answer["responses"][0]["evidence"] = json!("   ");
    }
    if scenario == "malformed" {
        answer = json!({"unexpected":true});
    }
    if claude {
        emit(
            json!({"type":"assistant","message":{"content":[{"type":"text","text":"Reviewing the business case."}]}}),
        );
        emit(
            json!({"type":"result","is_error":false,"structured_output":answer,"usage":{"input_tokens":100,"output_tokens":50,"cache_read_input_tokens":10,"cache_creation_input_tokens":0},"modelUsage":{"claude-fake-1":{}}}),
        );
    } else {
        emit(
            json!({"type":"item.completed","item":{"type":"reasoning","text":"Checking criteria"}}),
        );
        emit(
            json!({"type":"item.completed","item":{"type":"agent_message","text":answer.to_string()}}),
        );
        emit(
            json!({"type":"turn.completed","usage":{"input_tokens":100,"cached_input_tokens":10,"output_tokens":50}}),
        );
    }
}
