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

const HISTOGRAM: &str = r#"{"type":"user.message","data":{"content":"go"},"id":"u1","timestamp":"2026-04-26T10:00:00Z"}
{"type":"assistant.turn_start","data":{"turnId":"0"},"id":"e1","timestamp":"2026-04-26T10:00:01Z"}
{"type":"assistant.message","data":{"content":"","toolRequests":[{"toolCallId":"r1","name":"report_intent","arguments":{"intent":"x"},"type":"function"},{"toolCallId":"b1","name":"bash","arguments":{"cmd":"ls"},"type":"function"},{"toolCallId":"b2","name":"bash","arguments":{"cmd":"cat x"},"type":"function"},{"toolCallId":"v1","name":"view","arguments":{"path":"/y"},"type":"function"}],"reasoningText":"","outputTokens":1},"id":"e2","timestamp":"2026-04-26T10:00:02Z"}
{"type":"tool.execution_complete","data":{"toolCallId":"r1","model":"claude-sonnet-4.6","success":true,"result":{"content":"","detailedContent":""}},"id":"e3","timestamp":"2026-04-26T10:00:03Z"}
{"type":"tool.execution_complete","data":{"toolCallId":"b1","model":"claude-sonnet-4.6","success":true,"result":{"content":"","detailedContent":"ok"}},"id":"e4","timestamp":"2026-04-26T10:00:04Z"}
{"type":"tool.execution_complete","data":{"toolCallId":"b2","model":"claude-sonnet-4.6","success":false,"result":null},"id":"e5","timestamp":"2026-04-26T10:00:05Z"}
{"type":"tool.execution_complete","data":{"toolCallId":"v1","model":"claude-sonnet-4.6","success":true,"result":{"content":"","detailedContent":"file"}},"id":"e6","timestamp":"2026-04-26T10:00:06Z"}
{"type":"assistant.turn_end","data":{"turnId":"0"},"id":"e7","timestamp":"2026-04-26T10:00:07Z"}
"#;

#[test]
fn tool_histogram_excludes_report_intent() {
    let i = parse_insights_from_str(HISTOGRAM);
    let names: Vec<&str> = i.tool_counts.iter().map(|(n, _, _)| n.as_str()).collect();
    assert!(!names.contains(&"report_intent"));
    // bash appears twice, view once. Sorted by count desc then alpha asc.
    assert_eq!(i.tool_counts[0], ("bash".to_string(), 2, 1));
    assert_eq!(i.tool_counts[1], ("view".to_string(), 1, 0));
}

#[test]
fn tool_errors_counts_failed_completions() {
    let i = parse_insights_from_str(HISTOGRAM);
    assert_eq!(i.tool_errors, 1);
    // The owning glyph (turn 0) should have has_error set.
    assert!(i.turns[0].has_error);
}

const TASKS: &str = r#"{"type":"user.message","data":{"content":"delegate twice"},"id":"u1","timestamp":"2026-04-26T10:00:00Z"}
{"type":"assistant.turn_start","data":{"turnId":"0"},"id":"e1","timestamp":"2026-04-26T10:00:01Z"}
{"type":"assistant.message","data":{"content":"","toolRequests":[{"toolCallId":"t1","name":"task","arguments":{"agent_type":"explorer","prompt":"look around"},"type":"function"}],"reasoningText":"","outputTokens":1},"id":"e2","timestamp":"2026-04-26T10:00:02Z"}
{"type":"tool.execution_complete","data":{"toolCallId":"t1","model":"claude-sonnet-4.6","success":true,"result":{"content":"","detailedContent":"done"}},"id":"e3","timestamp":"2026-04-26T10:00:03Z"}
{"type":"assistant.message","data":{"content":"","toolRequests":[{"toolCallId":"t2","name":"task","arguments":{"agent_type":"explorer","prompt":"again"},"type":"function"},{"toolCallId":"t3","name":"task","arguments":{"agent_type":"reviewer","prompt":"review"},"type":"function"}],"reasoningText":"","outputTokens":1},"id":"e4","timestamp":"2026-04-26T10:00:04Z"}
{"type":"tool.execution_complete","data":{"toolCallId":"t2","model":"claude-sonnet-4.6","success":true,"result":{"content":"","detailedContent":"done"}},"id":"e5","timestamp":"2026-04-26T10:00:05Z"}
{"type":"tool.execution_complete","data":{"toolCallId":"t3","model":"claude-sonnet-4.6","success":true,"result":{"content":"","detailedContent":"done"}},"id":"e6","timestamp":"2026-04-26T10:00:06Z"}
{"type":"assistant.turn_end","data":{"turnId":"0"},"id":"e7","timestamp":"2026-04-26T10:00:07Z"}
"#;

#[test]
fn tasks_populated_dedup_first_occurrence_order() {
    let i = parse_insights_from_str(TASKS);
    assert_eq!(
        i.tasks,
        vec!["explorer".to_string(), "reviewer".to_string()]
    );
}

#[test]
fn skills_empty_for_copilot() {
    let i = parse_insights_from_str(TASKS);
    assert!(i.skills.is_empty());
}

#[test]
fn model_picked_up_from_tool_complete() {
    // BASICS has no session.model_change; model must come from tool.execution_complete.
    let i = parse_insights_from_str(BASICS);
    assert_eq!(i.model, "claude-sonnet-4.6");
}
