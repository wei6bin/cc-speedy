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

const PARALLEL: &str = r#"{"type":"user.message","data":{"content":"do three things"},"id":"u1","timestamp":"2026-04-26T10:00:00Z"}
{"type":"assistant.turn_start","data":{"turnId":"0"},"id":"e1","timestamp":"2026-04-26T10:00:01Z"}
{"type":"assistant.message","data":{"content":"","toolRequests":[{"toolCallId":"A","name":"bash","arguments":{"cmd":"a"},"type":"function"},{"toolCallId":"B","name":"bash","arguments":{"cmd":"b"},"type":"function"},{"toolCallId":"C","name":"bash","arguments":{"cmd":"c"},"type":"function"}],"reasoningText":"three at once","outputTokens":5},"id":"e2","timestamp":"2026-04-26T10:00:02Z"}
{"type":"tool.execution_complete","data":{"toolCallId":"C","model":"claude-sonnet-4.6","success":true,"result":{"content":"","detailedContent":"out-c"}},"id":"e3","timestamp":"2026-04-26T10:00:03Z"}
{"type":"tool.execution_complete","data":{"toolCallId":"B","model":"claude-sonnet-4.6","success":true,"result":{"content":"","detailedContent":"out-b"}},"id":"e4","timestamp":"2026-04-26T10:00:04Z"}
{"type":"tool.execution_complete","data":{"toolCallId":"A","model":"claude-sonnet-4.6","success":true,"result":{"content":"","detailedContent":"out-a"}},"id":"e5","timestamp":"2026-04-26T10:00:05Z"}
{"type":"assistant.turn_end","data":{"turnId":"0"},"id":"e6","timestamp":"2026-04-26T10:00:06Z"}
"#;

#[test]
fn parallel_tool_completions_pair_by_id() {
    let t = extract_turn_from_str(PARALLEL, 0).unwrap();
    let outputs: Vec<&str> = t
        .blocks
        .iter()
        .filter_map(|b| match b {
            DetailBlock::Tool {
                result: Some(r), ..
            } => Some(r.content.as_str()),
            _ => None,
        })
        .collect();
    // Block order matches toolRequests order: A, B, C.
    assert_eq!(outputs, vec!["out-a", "out-b", "out-c"]);
}

const FAILED_TOOL: &str = r#"{"type":"user.message","data":{"content":"try it"},"id":"u1","timestamp":"2026-04-26T10:00:00Z"}
{"type":"assistant.turn_start","data":{"turnId":"0"},"id":"e1","timestamp":"2026-04-26T10:00:01Z"}
{"type":"assistant.message","data":{"content":"","toolRequests":[{"toolCallId":"f1","name":"bash","arguments":{"cmd":"missing"},"type":"function"}],"reasoningText":"running","outputTokens":3},"id":"e2","timestamp":"2026-04-26T10:00:02Z"}
{"type":"tool.execution_complete","data":{"toolCallId":"f1","model":"claude-sonnet-4.6","success":false,"result":null},"id":"e3","timestamp":"2026-04-26T10:00:03Z"}
{"type":"assistant.turn_end","data":{"turnId":"0"},"id":"e4","timestamp":"2026-04-26T10:00:04Z"}
"#;

#[test]
fn failed_tool_marked_as_error() {
    let t = extract_turn_from_str(FAILED_TOOL, 0).unwrap();
    let tool = t
        .blocks
        .iter()
        .find_map(|b| match b {
            DetailBlock::Tool { result, .. } => result.as_ref(),
            _ => None,
        })
        .expect("tool block with result");
    assert!(tool.is_error);
    assert_eq!(tool.content, "");
    assert_eq!(tool.original_bytes, 0);
    assert!(!tool.truncated);
}

const REDACTED: &str = r#"{"type":"user.message","data":{"content":"think"},"id":"u1","timestamp":"2026-04-26T10:00:00Z"}
{"type":"assistant.turn_start","data":{"turnId":"0"},"id":"e1","timestamp":"2026-04-26T10:00:01Z"}
{"type":"assistant.message","data":{"content":"ok","toolRequests":[],"reasoningText":"","reasoningOpaque":"OPAQUE_BLOB","outputTokens":2},"id":"e2","timestamp":"2026-04-26T10:00:02Z"}
{"type":"assistant.turn_end","data":{"turnId":"0"},"id":"e3","timestamp":"2026-04-26T10:00:03Z"}
"#;

#[test]
fn redacted_thinking_block() {
    let t = extract_turn_from_str(REDACTED, 0).unwrap();
    match &t.blocks[0] {
        DetailBlock::Thinking { text, redacted } => {
            assert!(text.is_empty());
            assert!(*redacted);
        }
        other => panic!("expected redacted Thinking, got {:?}", other),
    }
}

const SUBAGENT: &str = r#"{"type":"user.message","data":{"content":"delegate"},"id":"u1","timestamp":"2026-04-26T10:00:00Z"}
{"type":"assistant.turn_start","data":{"turnId":"0"},"id":"e1","timestamp":"2026-04-26T10:00:01Z"}
{"type":"assistant.message","data":{"content":"","toolRequests":[{"toolCallId":"task1","name":"task","arguments":{"agent_type":"general","prompt":"go"},"type":"function"}],"reasoningText":"hand off","outputTokens":7},"id":"e2","timestamp":"2026-04-26T10:00:02Z"}
{"type":"assistant.message","data":{"parentToolCallId":"task1","content":"sub: thinking","toolRequests":[],"reasoningText":"sub-internal","outputTokens":1},"id":"e3","timestamp":"2026-04-26T10:00:03Z"}
{"type":"assistant.message","data":{"parentToolCallId":"task1","content":"sub: done","toolRequests":[],"reasoningText":"","outputTokens":1},"id":"e4","timestamp":"2026-04-26T10:00:04Z"}
{"type":"tool.execution_complete","data":{"toolCallId":"task1","model":"claude-sonnet-4.6","success":true,"result":{"content":"","detailedContent":"sub-result"}},"id":"e5","timestamp":"2026-04-26T10:00:05Z"}
{"type":"assistant.message","data":{"content":"All done.","toolRequests":[],"reasoningText":"","outputTokens":3},"id":"e6","timestamp":"2026-04-26T10:00:06Z"}
{"type":"assistant.turn_end","data":{"turnId":"0"},"id":"e7","timestamp":"2026-04-26T10:00:07Z"}
"#;

#[test]
fn subagent_messages_excluded_from_index() {
    // Two main-thread messages: the task-dispatcher (idx 0) and the wrap-up
    // text (idx 1). Sub-agent messages must not occupy idx slots.
    let t0 = extract_turn_from_str(SUBAGENT, 0).unwrap();
    assert!(matches!(t0.blocks.last().unwrap(),
        DetailBlock::Tool { name, .. } if name == "task"));

    let t1 = extract_turn_from_str(SUBAGENT, 1).unwrap();
    assert_eq!(t1.blocks.len(), 1);
    match &t1.blocks[0] {
        DetailBlock::Text { text } => assert_eq!(text, "All done."),
        other => panic!("expected Text wrap-up, got {:?}", other),
    }

    let t2 = extract_turn_from_str(SUBAGENT, 2);
    assert!(t2.is_err(), "no third main-thread message exists");
}

#[test]
fn large_result_truncated_at_char_boundary() {
    // 4-byte char "🦀" (U+1F980). Add a leading "a" so the byte at index
    // RESULT_BYTE_CAP lands mid-char and forces the truncator to walk back to
    // the previous char boundary. Pure-ASCII fixtures wouldn't exercise this.
    let huge = format!("a{}", "🦀".repeat(RESULT_BYTE_CAP));
    let fixture = format!(
        r#"{{"type":"user.message","data":{{"content":"big"}},"id":"u1","timestamp":"2026-04-26T10:00:00Z"}}
{{"type":"assistant.turn_start","data":{{"turnId":"0"}},"id":"e1","timestamp":"2026-04-26T10:00:01Z"}}
{{"type":"assistant.message","data":{{"content":"","toolRequests":[{{"toolCallId":"h1","name":"bash","arguments":{{}},"type":"function"}}],"reasoningText":"","outputTokens":1}},"id":"e2","timestamp":"2026-04-26T10:00:02Z"}}
{{"type":"tool.execution_complete","data":{{"toolCallId":"h1","model":"claude-sonnet-4.6","success":true,"result":{{"content":"short","detailedContent":"{}"}}}},"id":"e3","timestamp":"2026-04-26T10:00:03Z"}}
{{"type":"assistant.turn_end","data":{{"turnId":"0"}},"id":"e4","timestamp":"2026-04-26T10:00:04Z"}}
"#,
        huge
    );
    let t = extract_turn_from_str(&fixture, 0).unwrap();
    let r = t
        .blocks
        .iter()
        .find_map(|b| match b {
            DetailBlock::Tool { result, .. } => result.as_ref(),
            _ => None,
        })
        .unwrap();
    assert!(r.truncated);
    assert!(r.content.len() <= RESULT_BYTE_CAP);
    // Walkback for a 4-byte char is at most 3 bytes; longer would mean the
    // truncator walked too far.
    assert!(
        r.content.len() >= RESULT_BYTE_CAP - 3,
        "walked back too far: got {}",
        r.content.len()
    );
    // Truncated content must be a valid UTF-8 prefix of the original — would
    // panic in `r.content.len()` slice if it weren't a char boundary.
    assert_eq!(&huge[..r.content.len()], r.content);
    assert_eq!(r.original_bytes, huge.len());
}

#[test]
fn out_of_range_returns_err() {
    let res = extract_turn_from_str(SIMPLE, 99);
    assert!(res.is_err());
}
