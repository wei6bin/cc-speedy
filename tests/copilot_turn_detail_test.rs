use cc_speedy::copilot_turn_detail::extract_turn_from_str;
use cc_speedy::turn_detail::{DetailBlock, RESULT_BYTE_CAP};

const SIMPLE: &str = r#"{"type":"session.start","data":{"sessionId":"s1"},"id":"e0","timestamp":"2026-04-26T10:00:00Z"}
{"type":"user.message","data":{"content":"hello"},"id":"u1","timestamp":"2026-04-26T10:00:01Z"}
{"type":"assistant.turn_start","data":{"turnId":"0"},"id":"e1","timestamp":"2026-04-26T10:00:02Z"}
{"type":"assistant.message","data":{"content":"Listing.","toolRequests":[{"toolCallId":"t1","name":"bash","arguments":{"cmd":"ls"},"type":"function"}],"reasoningText":"User wants a listing.","outputTokens":42},"id":"e2","timestamp":"2026-04-26T10:00:03Z"}
{"type":"tool.execution_start","data":{"toolCallId":"t1","toolName":"bash","arguments":{"cmd":"ls"}},"id":"e3","timestamp":"2026-04-26T10:00:04Z"}
{"type":"tool.execution_complete","data":{"toolCallId":"t1","model":"claude-sonnet-4.6","success":true,"result":{"content":"file1\nfile2","detailedContent":"file1\nfile2\n"}},"id":"e4","timestamp":"2026-04-26T10:00:05Z"}
{"type":"assistant.turn_end","data":{"turnId":"0"},"id":"e5","timestamp":"2026-04-26T10:00:06Z"}
"#;

#[test]
fn turn_0_basic_extraction() {
    let t = extract_turn_from_str(SIMPLE, 0).unwrap();
    assert_eq!(t.turn_idx, 0);
    assert_eq!(t.user_msg.as_deref(), Some("hello"));
    assert_eq!(t.model, "claude-sonnet-4.6");
    assert_eq!(t.usage.output_tokens, 42);
    assert_eq!(t.usage.input_tokens, 0);
    assert_eq!(t.usage.cache_creation, 0);
    assert_eq!(t.usage.cache_read, 0);
    assert_eq!(t.blocks.len(), 3, "thinking + text + tool");
    match &t.blocks[0] {
        DetailBlock::Thinking { text, redacted } => {
            assert_eq!(text, "User wants a listing.");
            assert!(!*redacted);
        }
        other => panic!("expected Thinking, got {:?}", other),
    }
    match &t.blocks[1] {
        DetailBlock::Text { text } => assert_eq!(text, "Listing."),
        other => panic!("expected Text, got {:?}", other),
    }
    match &t.blocks[2] {
        DetailBlock::Tool {
            name,
            input_pretty,
            result,
        } => {
            assert_eq!(name, "bash");
            assert!(input_pretty.contains("\"cmd\""));
            assert!(input_pretty.contains("\"ls\""));
            let r = result.as_ref().unwrap();
            assert!(!r.is_error);
            assert_eq!(r.content, "file1\nfile2\n");
            assert!(!r.truncated);
            assert_eq!(r.original_bytes, "file1\nfile2\n".len());
        }
        other => panic!("expected Tool, got {:?}", other),
    }
    let _ = RESULT_BYTE_CAP;
}

const MULTI_ROUND: &str = r#"{"type":"user.message","data":{"content":"refactor X"},"id":"u1","timestamp":"2026-04-26T10:00:00Z"}
{"type":"assistant.turn_start","data":{"turnId":"0"},"id":"e1","timestamp":"2026-04-26T10:00:01Z"}
{"type":"assistant.message","data":{"content":"","toolRequests":[{"toolCallId":"a1","name":"view","arguments":{"path":"/x"},"type":"function"}],"reasoningText":"Look at X first.","outputTokens":10},"id":"e2","timestamp":"2026-04-26T10:00:02Z"}
{"type":"tool.execution_complete","data":{"toolCallId":"a1","model":"claude-sonnet-4.6","success":true,"result":{"content":"X contents","detailedContent":"X contents"}},"id":"e3","timestamp":"2026-04-26T10:00:03Z"}
{"type":"assistant.message","data":{"content":"","toolRequests":[{"toolCallId":"b1","name":"edit","arguments":{"path":"/x","new":"Y"},"type":"function"}],"reasoningText":"Now edit it.","outputTokens":15},"id":"e4","timestamp":"2026-04-26T10:00:04Z"}
{"type":"tool.execution_complete","data":{"toolCallId":"b1","model":"claude-sonnet-4.6","success":true,"result":{"content":"ok","detailedContent":"ok"}},"id":"e5","timestamp":"2026-04-26T10:00:05Z"}
{"type":"assistant.message","data":{"content":"Done.","toolRequests":[],"reasoningText":"","outputTokens":4},"id":"e6","timestamp":"2026-04-26T10:00:06Z"}
{"type":"assistant.turn_end","data":{"turnId":"0"},"id":"e7","timestamp":"2026-04-26T10:00:07Z"}
"#;

#[test]
fn multi_round_turn_carries_originating_user_msg() {
    for idx in 0..3u32 {
        let t = extract_turn_from_str(MULTI_ROUND, idx).unwrap();
        assert_eq!(
            t.user_msg.as_deref(),
            Some("refactor X"),
            "round {} should keep the originating prompt",
            idx
        );
        assert_eq!(t.turn_idx, idx);
    }
    let final_round = extract_turn_from_str(MULTI_ROUND, 2).unwrap();
    assert_eq!(final_round.blocks.len(), 1);
    match &final_round.blocks[0] {
        DetailBlock::Text { text } => assert_eq!(text, "Done."),
        other => panic!("expected Text, got {:?}", other),
    }
}
