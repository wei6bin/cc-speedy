use cc_speedy::turn_detail::{extract_turn_from_str, DetailBlock, RESULT_BYTE_CAP};

const FIXTURE: &str = r#"{"type":"user","message":{"role":"user","content":"first prompt"}}
{"type":"assistant","message":{"role":"assistant","model":"claude-opus-4-7","usage":{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":200,"cache_read_input_tokens":300},"content":[{"type":"text","text":"hi"},{"type":"tool_use","id":"toolu_01","name":"Bash","input":{"command":"ls"}}]}}
{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_01","is_error":false,"content":"file1\nfile2"}]}}
{"type":"user","message":{"role":"user","content":"second prompt"}}
{"type":"assistant","message":{"role":"assistant","model":"claude-opus-4-7","usage":{"input_tokens":120,"output_tokens":80},"content":[{"type":"thinking","thinking":"plan"},{"type":"tool_use","id":"toolu_02","name":"Bash","input":{"command":"cat x"}},{"type":"tool_use","id":"toolu_03","name":"Read","input":{"file_path":"/x"}}]}}
{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_02","is_error":true,"content":"oops"},{"type":"tool_result","tool_use_id":"toolu_03","is_error":false,"content":"ok"}]}}
"#;

#[test]
fn extracts_first_turn_with_user_msg_and_paired_result() {
    let t = extract_turn_from_str(FIXTURE, 0).unwrap();
    assert_eq!(t.turn_idx, 0);
    assert_eq!(t.user_msg.as_deref(), Some("first prompt"));
    assert_eq!(t.model, "claude-opus-4-7");
    assert_eq!(t.usage.input_tokens, 100);
    assert_eq!(t.usage.output_tokens, 50);
    assert_eq!(t.usage.cache_creation, 200);
    assert_eq!(t.usage.cache_read, 300);
    assert_eq!(t.blocks.len(), 2);
    match &t.blocks[1] {
        DetailBlock::Tool { name, result, .. } => {
            assert_eq!(name, "Bash");
            let r = result.as_ref().unwrap();
            assert!(!r.is_error);
            assert_eq!(r.content, "file1\nfile2");
            assert!(!r.truncated);
        }
        other => panic!("expected Tool block, got {:?}", other),
    }
}

#[test]
fn extracts_second_turn_with_thinking_and_two_tools() {
    let t = extract_turn_from_str(FIXTURE, 1).unwrap();
    assert_eq!(t.turn_idx, 1);
    assert_eq!(t.user_msg.as_deref(), Some("second prompt"));
    assert_eq!(t.blocks.len(), 3);
    match &t.blocks[0] {
        DetailBlock::Thinking { text, redacted } => {
            assert_eq!(text, "plan");
            assert!(!redacted, "no signature → not redacted");
        }
        other => panic!("expected Thinking, got {:?}", other),
    }
    match &t.blocks[1] {
        DetailBlock::Tool { name, result, .. } => {
            assert_eq!(name, "Bash");
            let r = result.as_ref().unwrap();
            assert!(r.is_error, "first tool should carry the is_error flag");
            assert_eq!(r.content, "oops");
        }
        other => panic!("got {:?}", other),
    }
    match &t.blocks[2] {
        DetailBlock::Tool { name, result, .. } => {
            assert_eq!(name, "Read");
            assert_eq!(result.as_ref().unwrap().content, "ok");
        }
        other => panic!("got {:?}", other),
    }
}

#[test]
fn out_of_range_turn_returns_err() {
    let res = extract_turn_from_str(FIXTURE, 99);
    assert!(res.is_err());
}

#[test]
fn truncates_oversized_tool_result() {
    let big = "x".repeat(RESULT_BYTE_CAP * 2);
    let src = format!(
        r#"{{"type":"user","message":{{"role":"user","content":"go"}}}}
{{"type":"assistant","message":{{"role":"assistant","model":"x","usage":{{}},"content":[{{"type":"tool_use","id":"toolu_big","name":"Bash","input":{{"command":"echo x"}}}}]}}}}
{{"type":"user","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"toolu_big","is_error":false,"content":"{}"}}]}}}}
"#,
        big
    );
    let t = extract_turn_from_str(&src, 0).unwrap();
    let DetailBlock::Tool { result, .. } = &t.blocks[0] else {
        panic!("expected Tool block")
    };
    let r = result.as_ref().unwrap();
    assert!(r.truncated);
    assert_eq!(r.content.len(), RESULT_BYTE_CAP);
    assert_eq!(r.original_bytes, big.len());
}

#[test]
fn user_msg_none_for_first_assistant_without_prior_user() {
    let src = r#"{"type":"assistant","message":{"role":"assistant","model":"x","usage":{},"content":[{"type":"text","text":"orphan"}]}}
"#;
    let t = extract_turn_from_str(src, 0).unwrap();
    assert!(t.user_msg.is_none());
}

#[test]
fn user_tool_result_only_lines_dont_overwrite_user_msg() {
    // Verify the post-tool-results user line in FIXTURE between turns 0 and 1
    // does NOT shadow the "second prompt" — the parser must skip user lines
    // whose content is only tool_result blocks.
    let t = extract_turn_from_str(FIXTURE, 1).unwrap();
    assert_eq!(t.user_msg.as_deref(), Some("second prompt"));
}

#[test]
fn pretty_prints_tool_input_json() {
    let t = extract_turn_from_str(FIXTURE, 1).unwrap();
    if let DetailBlock::Tool { input_pretty, .. } = &t.blocks[1] {
        assert!(input_pretty.contains("\"command\""));
        assert!(
            input_pretty.contains('\n'),
            "should be multi-line pretty-print"
        );
    } else {
        panic!()
    }
}

#[test]
fn thinking_redacted_when_signature_present_and_text_empty() {
    let src = r#"{"type":"assistant","message":{"role":"assistant","model":"x","usage":{},"content":[{"type":"thinking","thinking":"","signature":"opaque-blob"}]}}
"#;
    let t = extract_turn_from_str(src, 0).unwrap();
    match &t.blocks[0] {
        DetailBlock::Thinking { text, redacted } => {
            assert_eq!(text, "");
            assert!(redacted);
        }
        _ => panic!(),
    }
}

#[test]
fn cache_hit_pct_per_turn() {
    let t = extract_turn_from_str(FIXTURE, 0).unwrap();
    // input=100, cache_read=300 → 300/400 = 75
    assert_eq!(t.usage.cache_hit_pct(), 75);
}
