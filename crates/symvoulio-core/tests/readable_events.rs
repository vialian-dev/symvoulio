// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
use serde_json::json;
use symvoulio_core::provider::controlled_summary;

#[test]
fn claude_stream_json_counts_text_without_misclassifying_non_text_blocks() {
    // Real stream-json emits separate assistant messages for thinking, tools and text.
    for block in [
        json!({"type":"thinking","thinking":"Private reasoning here","signature":"private"}),
        json!({"type":"tool_use","id":"toolu_1","name":"StructuredOutput","input":{"answer":"Non-empty tool input"}}),
    ] {
        let event = json!({"type":"assistant","message":{"id":"msg_1","role":"assistant","content":[block]},"session_id":"session"});
        assert_eq!(controlled_summary(&event.to_string()), "assistant: provider event");
    }
    let event = json!({"type":"assistant","message":{"id":"msg_2","role":"assistant","content":[
        {"type":"thinking","thinking":"Do not count these words"},
        {"type":"text","text":"Reviewing the business case."},
        {"type":"text","text":"Totals reconcile."}
    ]},"session_id":"session"});
    assert_eq!(controlled_summary(&event.to_string()), "assistant: assistant text, 6 words");
    for wrapped in [false, true] {
        let delta = json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Totals reconcile."}});
        let event = if wrapped { json!({"type":"stream_event","event":delta,"session_id":"session"}) } else { delta };
        assert_eq!(controlled_summary(&event.to_string()), "content_block_delta: assistant text, 2 words");
    }
    let start = json!({"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":"Review ready"}}});
    assert_eq!(controlled_summary(&start.to_string()), "content_block_start: assistant text, 2 words");
}

#[test]
fn codex_json_counts_agent_message_text_at_each_item_lifecycle_event() {
    for kind in ["item.started", "item.updated", "item.completed"] {
        let event = json!({"type":kind,"item":{"id":"item_3","type":"agent_message","text":"Totals reconcile.\nReady for review."}});
        let summary = controlled_summary(&event.to_string());
        assert_eq!(summary, format!("{kind}: assistant text, 5 words"));
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
