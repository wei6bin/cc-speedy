use cc_speedy::copilot_insights::parse_insights_from_str;
use cc_speedy::insights::GlyphCategory;

const BASICS: &str = r#"{"type":"session.start","data":{"sessionId":"s1"},"id":"e0","timestamp":"2026-04-26T10:00:00Z"}
{"type":"user.message","data":{"content":"hi"},"id":"u1","timestamp":"2026-04-26T10:00:01Z"}
{"type":"assistant.turn_start","data":{"turnId":"0"},"id":"e1","timestamp":"2026-04-26T10:00:02Z"}
{"type":"assistant.message","data":{"content":"hello","toolRequests":[],"reasoningText":"","outputTokens":11},"id":"e2","timestamp":"2026-04-26T10:00:03Z"}
{"type":"assistant.message","data":{"parentToolCallId":"x","content":"sub","toolRequests":[],"reasoningText":"","outputTokens":1},"id":"e2b","timestamp":"2026-04-26T10:00:03Z"}
{"type":"assistant.message","data":{"content":"","toolRequests":[{"toolCallId":"t1","name":"bash","arguments":{},"type":"function"}],"reasoningText":"","outputTokens":7},"id":"e3","timestamp":"2026-04-26T10:00:04Z"}
{"type":"tool.execution_complete","data":{"toolCallId":"t1","model":"claude-sonnet-4.6","success":true,"result":{"content":"","detailedContent":"ok"}},"id":"e4","timestamp":"2026-04-26T10:00:05Z"}
{"type":"assistant.turn_end","data":{"turnId":"0"},"id":"e5","timestamp":"2026-04-26T10:00:06Z"}
"#;

#[test]
fn basic_counts() {
    let i = parse_insights_from_str(BASICS);
    assert_eq!(i.assistant_turns, 2, "two main-thread messages");
    assert_eq!(i.sidechain_lines, 1, "one parentToolCallId message");
    assert_eq!(i.output_tokens, 11 + 7);
    assert_eq!(i.input_tokens, 0);
    assert_eq!(i.cache_creation, 0);
    assert_eq!(i.cache_read, 0);
    assert_eq!(i.model, "claude-sonnet-4.6");
    assert_eq!(i.turns.len(), 2);
    assert_eq!(i.turns[0].turn, 0);
    assert_eq!(i.turns[1].turn, 1);
    assert!(matches!(i.turns[0].category, GlyphCategory::Text));
    assert!(matches!(i.turns[1].category, GlyphCategory::Tool));
}
