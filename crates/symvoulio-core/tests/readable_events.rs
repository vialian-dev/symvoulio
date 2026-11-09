// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
use serde_json::json;
use symvoulio_core::provider::controlled_summary;

#[test]
fn claude_stream_json_counts_text_without_misclassifying_non_text_blocks() {
    // Real stream-json emits separate assistant messages for thinking, tools and text.
    for (block, expected) in [
        (json!({"type":"thinking","thinking":"Private reasoning here","signature":"private"}), "thinking"),
        (json!({"type":"tool_use","id":"toolu_1","name":"StructuredOutput","input":{"answer":"Non-empty tool input"}}), "writing the structured answer"),
    ] {
        let event = json!({"type":"assistant","message":{"id":"msg_1","role":"assistant","content":[block]},"session_id":"session"});
        assert_eq!(controlled_summary(&event.to_string()), expected);
    }
    let event = json!({"type":"assistant","message":{"id":"msg_2","role":"assistant","content":[
        {"type":"thinking","thinking":"Do not count these words"},
        {"type":"text","text":"Reviewing the business case."},
        {"type":"text","text":"Totals reconcile."}
    ]},"session_id":"session"});
    assert_eq!(controlled_summary(&event.to_string()), "assistant text, 6 words");
    for wrapped in [false, true] {
        let delta = json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Totals reconcile."}});
        let event = if wrapped { json!({"type":"stream_event","event":delta,"session_id":"session"}) } else { delta };
        assert_eq!(controlled_summary(&event.to_string()), "assistant text, 2 words");
    }
    let start = json!({"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":"Review ready"}}});
    assert_eq!(controlled_summary(&start.to_string()), "assistant text, 2 words");
}

#[test]
fn codex_json_counts_agent_message_text_at_each_item_lifecycle_event() {
    for kind in ["item.started", "item.updated", "item.completed"] {
        let event = json!({"type":kind,"item":{"id":"item_3","type":"agent_message","text":"Totals reconcile.\nReady for review."}});
        let summary = controlled_summary(&event.to_string());
        assert_eq!(summary, "assistant text, 5 words");
        assert!(!summary.contains("Totals"));
    }
    for event in [
        json!({"type":"item.started","item":{"id":"item_0","type":"agent_message","text":""}}),
        json!({"type":"assistant","message":{"content":[]}}),
        json!({"type":"assistant","message":{"content":[{"type":"text","text":" \n "}]}}),
    ] {
        let summary = controlled_summary(&event.to_string());
        assert!(!summary.contains("assistant text"));
        assert!(!summary.contains("0 words"));
    }
}

#[test]
fn claude_events_describe_activity_using_fixed_phrases() {
    for (event, expected) in [
        (json!({"type":"system","subtype":"init"}), "session started"),
        (json!({"type":"system","subtype":"api_retry","attempt":2}), "retrying the request, attempt 2"),
        (json!({"type":"system","subtype":"api_retry"}), "retrying the request"),
        (json!({"type":"system","subtype":"status"}), "session status updated"),
        (json!({"type":"system","subtype":"compact_boundary"}), "conversation compacted"),
        (json!({"type":"system","subtype":"unknown"}), "session info"),
        (json!({"type":"system","subtype":"hook_started"}), "running a hook"),
        (json!({"type":"system","subtype":"hook_progress"}), "running a hook"),
        (json!({"type":"system","subtype":"hook_response"}), "hook finished"),
        (json!({"type":"rate_limit_event","rate_limit_info":{"status":"allowed"}}), "rate limit status: OK"),
        (json!({"type":"rate_limit_event","rate_limit_info":{"status":"allowed_warning"}}), "rate limit status: close to the limit"),
        (json!({"type":"rate_limit_event","rate_limit_info":{"status":"rejected"}}), "rate limit reached"),
        (json!({"type":"rate_limit_event"}), "rate limit status"),
        (json!({"type":"user","message":{"content":[{"type":"text","text":"private"},{"type":"tool_result","content":"private"}]}}), "tool result received"),
        (json!({"type":"user","message":{"content":[{"type":"text","text":"private"}]}}), "input sent"),
        (json!({"type":"user","message":{"content":"private"}}), "input sent"),
        (json!({"type":"user"}), "input sent"),
        (json!({"type":"unknown"}), "other provider activity"),
        (json!({"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash"}]}}), "calling a tool"),
        (json!({"type":"assistant","message":{"content":[]}}), "assistant message"),
        (json!({"type":"result"}), "Completed"),
    ] {
        assert_eq!(controlled_summary(&event.to_string()), expected, "{event}");
    }
}

#[test]
fn claude_thinking_tokens_only_use_non_negative_integers() {
    for tokens in [json!(0), json!(123), json!(u64::MAX)] {
        let event = json!({"type":"system","subtype":"thinking_tokens","estimated_tokens":tokens});
        assert_eq!(controlled_summary(&event.to_string()), format!("thinking, about {tokens} tokens"));
    }
    for tokens in [json!(-1), json!(1.5), json!("123"), json!("sk-ant-secret"), json!(true), json!(null), json!({}), json!([])] {
        let event = json!({"type":"system","subtype":"thinking_tokens","estimated_tokens":tokens});
        assert_eq!(controlled_summary(&event.to_string()), "thinking");
    }
    assert_eq!(controlled_summary(r#"{"type":"system","subtype":"thinking_tokens"}"#), "thinking");
}

#[test]
fn recorded_claude_stream_describes_every_event_without_provider_strings() {
    let secret = "sk-ant-secretCredential123";
    let cwd = r"C:\private\client-project";
    let unknown_status = "private-future-rate-status";
    let mut events = vec![
        json!({"type":"system","subtype":"init","cwd":cwd,"session_id":"session","tools":["StructuredOutput"],"model":"private-model"}),
        json!({"type":"rate_limit_event","rate_limit_info":{"status":unknown_status,"resetsAt":1794100000,"rateLimitType":"five_hour","utilization":0.2,"overageStatus":"disabled","overageDisabledReason":"private-reason","isUsingOverage":false,"surpassedThreshold":null,"unifiedWindows":[],"overageEnabled":false},"uuid":"rate-1","session_id":"session"}),
    ];
    for tokens in 1..=8 {
        events.push(json!({"type":"system","subtype":"thinking_tokens","estimated_tokens":tokens * 100,"estimated_tokens_delta":100,"session_id":"session","uuid":format!("thinking-{tokens}")}));
    }
    events.extend([
        json!({"type":"assistant","message":{"content":[{"type":"thinking","thinking":secret,"signature":"private-signature"}]}}),
        json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"tool-1","name":"StructuredOutput","input":{"answer":secret}}]}}),
        json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"tool-1","content":secret}]},"parent_tool_use_id":null,"tool_use_result":secret,"tool_result_meta":[]}),
        json!({"type":"result","subtype":"success","result":secret}),
    ]);
    let recording = events.iter().map(|event| event.to_string()).collect::<Vec<_>>().join("\n");
    let summaries = recording.lines().map(controlled_summary).collect::<Vec<_>>();
    assert_eq!(summaries.len(), 14);
    for summary in &summaries {
        assert!(!["Provider event", "other provider activity", "system message"].contains(&summary.as_str()), "{summary}");
        assert_eq!(summary.lines().count(), 1);
        for private in [secret, cwd, unknown_status, "private-model", "private-reason", "private-signature"] {
            assert!(!summary.contains(private), "{summary}");
        }
    }
    let mut expected = vec!["session started".to_string(), "rate limit status".to_string()];
    expected.extend((1..=8).map(|tokens| format!("thinking, about {} tokens", tokens * 100)));
    expected.extend(["thinking", "writing the structured answer", "tool result received", "Completed"].map(str::to_string));
    assert_eq!(summaries, expected);
}

#[test]
fn claude_partial_events_describe_block_activity_wrapped_or_unwrapped() {
    for (event, expected) in [
        (json!({"type":"message_start"}), "answer started"),
        (json!({"type":"message_delta"}), "answer finished"),
        (json!({"type":"message_stop"}), "answer finished"),
        (json!({"type":"content_block_start","content_block":{"type":"thinking"}}), "thinking"),
        (json!({"type":"content_block_start","content_block":{"type":"redacted_thinking"}}), "thinking"),
        (json!({"type":"content_block_start","content_block":{"type":"tool_use","name":"StructuredOutput"}}), "writing the structured answer"),
        (json!({"type":"content_block_start","content_block":{"type":"tool_use","name":"Bash"}}), "calling a tool"),
        (json!({"type":"content_block_start","content_block":{"type":"text","text":""}}), "answer text started"),
        (json!({"type":"content_block_delta","delta":{"type":"thinking_delta"}}), "thinking"),
        (json!({"type":"content_block_delta","delta":{"type":"signature_delta"}}), "thinking"),
        (json!({"type":"content_block_delta","delta":{"type":"input_json_delta","partial_json":"{}"}}), "calling a tool"),
        (json!({"type":"content_block_stop","content_block":{"type":"thinking"}}), "thinking"),
        (json!({"type":"content_block_stop","content_block":{"type":"tool_use","name":"StructuredOutput"}}), "writing the structured answer"),
        (json!({"type":"content_block_stop","content_block":{"type":"text","text":"All done"}}), "assistant text, 2 words"),
        // The API's stop event normally carries only an index, not a block type.
        (json!({"type":"content_block_stop","index":0}), "answer section finished"),
        (json!({"type":"content_block_start"}), "answer section started"),
        (json!({"type":"content_block_delta"}), "answer updated"),
    ] {
        for wrapped in [false, true] {
            let raw = if wrapped { json!({"type":"stream_event","event":event}) } else { event.clone() };
            assert_eq!(controlled_summary(&raw.to_string()), expected, "{raw}");
        }
    }
}

#[test]
fn codex_events_describe_each_item_lifecycle_without_payload_text() {
    for (event, expected) in [
        (json!({"type":"thread.started"}), "session started"),
        (json!({"type":"turn.started"}), "turn started"),
        (json!({"type":"turn.completed"}), "Completed"),
        (json!({"type":"turn.failed","error":{"message":"private"}}), "provider reported an error"),
        (json!({"type":"error","message":"private"}), "provider reported an error"),
    ] {
        assert_eq!(controlled_summary(&event.to_string()), expected, "{event}");
    }
    for (item_type, started, updated, completed) in [
        ("reasoning", "reasoning", "reasoning", "reasoning"),
        ("agent_message", "assistant text, 2 words", "assistant text, 2 words", "assistant text, 2 words"),
        ("command_execution", "running a command", "running a command", "command finished"),
        ("file_change", "changing files", "changing files", "files changed"),
        ("mcp_tool_call", "calling a tool", "calling a tool", "tool finished"),
        ("web_search", "searching the web", "searching the web", "web search finished"),
        ("todo_list", "task list updated", "task list updated", "task list updated"),
        ("error", "provider reported an error", "provider reported an error", "provider reported an error"),
        ("unknown", "activity started", "activity updated", "activity finished"),
    ] {
        for (kind, expected) in [("item.started", started), ("item.updated", updated), ("item.completed", completed)] {
            let event = json!({"type":kind,"item":{"type":item_type,"text":"Private payload","command":"private"}});
            assert_eq!(controlled_summary(&event.to_string()), expected, "{event}");
        }
    }
}

#[test]
fn summaries_never_interpolate_provider_strings() {
    let secret = "sk-ant-secretCredential123";
    for event in [
        json!({"type":"system","subtype":"api_retry","attempt":secret,"message":secret}),
        json!({"type":"system","subtype":secret}),
        json!({"type":"assistant","message":{"content":[{"type":"text","text":secret}]}}),
        json!({"type":"assistant","message":{"content":[{"type":"thinking","thinking":secret}]}}),
        json!({"type":"assistant","message":{"content":[{"type":"tool_use","name":secret,"input":{"text":secret}}]}}),
        json!({"type":"content_block_start","content_block":{"type":"tool_use","name":secret}}),
        json!({"type":"content_block_delta","delta":{"type":"text_delta","text":secret}}),
        json!({"type":"content_block_delta","delta":{"type":"input_json_delta","partial_json":secret}}),
        json!({"type":"item.started","item":{"type":"command_execution","command":secret,"text":secret,"aggregated_output":secret}}),
        json!({"type":"item.completed","item":{"type":"agent_message","text":secret}}),
        json!({"type":"item.updated","item":{"type":secret,"text":secret}}),
        json!({"type":"error","message":secret}),
        json!({"type":"turn.failed","error":{"message":secret}}),
        json!({"type":secret,"message":secret}),
        json!({"type":"stream_event","event":{"type":secret,"text":secret}}),
    ] {
        let summary = controlled_summary(&event.to_string());
        assert!(!summary.contains(secret), "{summary}");
        assert!(!summary.is_empty(), "every event must produce a visible line");
    }
    assert_eq!(controlled_summary(secret), "Provider event");
    for raw in ["null", "{}"] {
        assert_eq!(controlled_summary(raw), "other provider activity");
    }
}
