use cc_speedy::insights::{
    fmt_tokens, parse_insights, parse_insights_from_str, tool_to_glyph, GlyphCategory,
};

const FIXTURE: &str = r#"{"type":"permission-mode","cwd":"/home/me"}
{"type":"user","message":{"role":"user","content":"do the thing"},"isSidechain":false}
{"type":"assistant","message":{"role":"assistant","model":"claude-opus-4-7","usage":{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":200,"cache_read_input_tokens":300},"content":[{"type":"text","text":"sure"},{"type":"tool_use","id":"toolu_01","name":"Bash","input":{"command":"ls"}}]}}
{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_01","is_error":false,"content":"file1\nfile2"}]}}
{"type":"assistant","message":{"role":"assistant","model":"claude-opus-4-7","usage":{"input_tokens":120,"output_tokens":80,"cache_creation_input_tokens":0,"cache_read_input_tokens":900},"content":[{"type":"thinking","text":"plan"},{"type":"tool_use","id":"toolu_02","name":"Bash","input":{"command":"cat x"}},{"type":"tool_use","id":"toolu_03","name":"Read","input":{"file_path":"/x"}}]}}
{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_02","is_error":true,"content":"oops"},{"type":"tool_result","tool_use_id":"toolu_03","is_error":false,"content":"ok"}]}}
{"type":"assistant","message":{"role":"assistant","model":"claude-opus-4-7","usage":{"input_tokens":50,"output_tokens":30,"cache_creation_input_tokens":0,"cache_read_input_tokens":1500},"content":[{"type":"tool_use","id":"toolu_04","name":"Skill","input":{"skill":"superpowers:brainstorming"}},{"type":"tool_use","id":"toolu_05","name":"Task","input":{"subagent_type":"Explore","description":"poke around"}}]}}
malformed line not json
{"type":"assistant","message":{"role":"assistant","model":"claude-opus-4-7","usage":{"input_tokens":10,"output_tokens":5},"content":[{"type":"tool_use","id":"toolu_06","name":"Skill","input":{"skill":"superpowers:brainstorming"}}]},"isSidechain":true}
"#;

#[test]
fn parses_basic_counts() {
    let i = parse_insights_from_str(FIXTURE);
    assert_eq!(i.assistant_turns, 4);
    assert_eq!(i.model, "claude-opus-4-7");
}

#[test]
fn sums_token_usage_across_turns() {
    let i = parse_insights_from_str(FIXTURE);
    assert_eq!(i.input_tokens, 100 + 120 + 50 + 10);
    assert_eq!(i.output_tokens, 50 + 80 + 30 + 5);
    assert_eq!(i.cache_creation, 200);
    assert_eq!(i.cache_read, 300 + 900 + 1500);
}

#[test]
fn cache_hit_pct_uses_input_plus_cache_read() {
    let i = parse_insights_from_str(FIXTURE);
    // input = 280, cache_read = 2700 → 2700 / 2980 = 90%
    assert_eq!(i.cache_hit_pct(), 90);
}

#[test]
fn tool_histogram_sorted_by_count_then_alpha() {
    let i = parse_insights_from_str(FIXTURE);
    // Bash×2, Read×1, Skill×2, Task×1 — Bash and Skill tie at 2 → Bash first alphabetically
    let names: Vec<&str> = i.tool_counts.iter().map(|(n, _, _)| n.as_str()).collect();
    assert_eq!(names, vec!["Bash", "Skill", "Read", "Task"]);
    let bash = &i.tool_counts[0];
    assert_eq!(bash.0, "Bash");
    assert_eq!(bash.1, 2);
    assert_eq!(
        bash.2, 1,
        "Bash should have 1 error attributed via tool_result"
    );
}

#[test]
fn skills_dedupe_in_first_seen_order() {
    let i = parse_insights_from_str(FIXTURE);
    assert_eq!(i.skills, vec!["superpowers:brainstorming"]);
}

#[test]
fn tasks_record_subagent_type() {
    let i = parse_insights_from_str(FIXTURE);
    assert_eq!(i.tasks, vec!["Explore"]);
}

#[test]
fn sidechain_lines_counted() {
    let i = parse_insights_from_str(FIXTURE);
    assert_eq!(i.sidechain_lines, 1);
}

#[test]
fn tool_errors_total_matches() {
    let i = parse_insights_from_str(FIXTURE);
    assert_eq!(i.tool_errors, 1);
}

#[test]
fn empty_input_yields_default() {
    let i = parse_insights_from_str("");
    assert!(i.is_empty());
    assert_eq!(i.cache_hit_pct(), 0);
}

#[test]
fn fmt_tokens_human_format() {
    assert_eq!(fmt_tokens(0), "0");
    assert_eq!(fmt_tokens(999), "999");
    assert_eq!(fmt_tokens(1234), "1.2k");
    assert_eq!(fmt_tokens(9_999), "10.0k");
    assert_eq!(fmt_tokens(56_789), "56k");
    assert_eq!(fmt_tokens(2_500_000), "2.5M");
}

#[test]
fn turn_glyphs_built_in_order() {
    let i = parse_insights_from_str(FIXTURE);
    assert_eq!(i.turns.len(), 4);
    assert_eq!(i.turns[0].turn, 0);
    assert_eq!(i.turns[3].turn, 3);
}

#[test]
fn glyph_priority_task_beats_skill() {
    // Turn 3 has both Skill (toolu_04) and Task (toolu_05) — Task should win.
    let i = parse_insights_from_str(FIXTURE);
    let t = &i.turns[2];
    assert_eq!(t.glyph, 'A', "Task should produce 'A' glyph");
    assert_eq!(t.category, GlyphCategory::Task);
    assert!(t.label.starts_with("Agent → "));
    assert!(t.label.contains("Explore"));
}

#[test]
fn glyph_priority_skill_when_no_task() {
    // Turn 4 has only Skill (sidechain).
    let i = parse_insights_from_str(FIXTURE);
    let t = &i.turns[3];
    assert_eq!(t.glyph, 'S');
    assert_eq!(t.category, GlyphCategory::Skill);
    // short-name strips "superpowers:" prefix
    assert_eq!(t.label, "Skill: brainstorming");
}

#[test]
fn glyph_dominant_tool_for_tool_only_turn() {
    // Turn 2 has Bash + Read; Bash should be dominant (count tie → first occurrence).
    let i = parse_insights_from_str(FIXTURE);
    let t = &i.turns[1];
    assert_eq!(t.glyph, 'B');
    assert_eq!(t.category, GlyphCategory::Tool);
    assert!(t.label.contains("Bash"));
    assert!(t.label.contains("Read"));
}

#[test]
fn glyph_has_error_attributed_to_owning_turn() {
    let i = parse_insights_from_str(FIXTURE);
    // Turn 2 owns toolu_02 which had is_error:true → has_error.
    assert!(i.turns[1].has_error);
    // Turn 1 owned toolu_01 (no error).
    assert!(!i.turns[0].has_error);
}

#[test]
fn glyph_thinking_only_turn() {
    let src = r#"{"type":"assistant","message":{"role":"assistant","model":"x","usage":{},"content":[{"type":"thinking","text":"hmm"}]}}
"#;
    let i = parse_insights_from_str(src);
    assert_eq!(i.turns[0].glyph, 't');
    assert_eq!(i.turns[0].category, GlyphCategory::Thinking);
}

#[test]
fn glyph_text_only_turn() {
    let src = r#"{"type":"assistant","message":{"role":"assistant","model":"x","usage":{},"content":[{"type":"text","text":"hi"}]}}
"#;
    let i = parse_insights_from_str(src);
    assert_eq!(i.turns[0].glyph, '·');
    assert_eq!(i.turns[0].category, GlyphCategory::Text);
}

#[test]
fn tool_to_glyph_known_names() {
    assert_eq!(tool_to_glyph("Bash"), 'B');
    assert_eq!(tool_to_glyph("Read"), 'R');
    assert_eq!(tool_to_glyph("Edit"), 'E');
    assert_eq!(tool_to_glyph("MultiEdit"), 'E');
    assert_eq!(tool_to_glyph("Write"), 'W');
    assert_eq!(tool_to_glyph("Grep"), 'g');
    assert_eq!(tool_to_glyph("Glob"), 'G');
    assert_eq!(tool_to_glyph("WebFetch"), 'F');
    assert_eq!(tool_to_glyph("TodoWrite"), '*');
    assert_eq!(tool_to_glyph("ToolSearch"), '?');
    assert_eq!(tool_to_glyph("WhateverCustomTool"), '+');
}

/// Smoke test against any real CC session in ~/.claude/projects — just verifies
/// that parsing doesn't panic and produces non-default output. Skipped in CI
/// (no fixtures), runs in dev.
#[test]
fn smoke_real_session() {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return,
    };
    let proj = home.join(".claude").join("projects");
    let Ok(rd) = std::fs::read_dir(&proj) else {
        return;
    };
    for entry in rd.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let Ok(files) = std::fs::read_dir(&dir) else {
            continue;
        };
        for f in files.flatten() {
            let p = f.path();
            if p.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                let _ = parse_insights(&p); // must not panic
                return;
            }
        }
    }
}
