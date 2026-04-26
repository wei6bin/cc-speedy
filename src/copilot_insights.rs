//! Per-session insights extracted from a Copilot `events.jsonl`. Companion to
//! `insights.rs` (Claude Code). Both produce the same `SessionInsights` so the
//! Insights panel renderer is source-agnostic.

use crate::copilot_turn_detail::SUPPRESSED_TOOL;
use crate::insights::{bump_error, increment_tool, GlyphCategory, SessionInsights, TurnGlyph};
use anyhow::Result;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub fn parse_insights(path: &Path) -> Result<SessionInsights> {
    let content = std::fs::read_to_string(path)?;
    Ok(parse_insights_from_str(&content))
}

pub fn parse_insights_from_str(content: &str) -> SessionInsights {
    let mut out = SessionInsights::default();
    let mut tool_idx: HashMap<String, usize> = HashMap::new();
    // toolCallId → (tool name, owning glyph index). Used to attribute a
    // `success: false` completion back to its histogram row + glyph.
    let mut tool_id_to_owner: HashMap<String, (String, usize)> = HashMap::new();
    let mut seen_task: HashSet<String> = HashSet::new();

    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let kind = v["type"].as_str().unwrap_or("");

        match kind {
            "session.model_change" => {
                if let Some(m) = v["data"]["model"].as_str() {
                    if !m.is_empty() {
                        out.model = m.to_string();
                    }
                }
            }
            "assistant.message" => {
                if v["data"]["parentToolCallId"].is_string() {
                    out.sidechain_lines += 1;
                    continue;
                }

                let reasoning = v["data"]["reasoningText"].as_str().unwrap_or("");
                let opaque = v["data"]["reasoningOpaque"].as_str().unwrap_or("");
                let text = v["data"]["content"].as_str().unwrap_or("");
                let mut tool_names: Vec<String> = Vec::new();
                if let Some(arr) = v["data"]["toolRequests"].as_array() {
                    for tr in arr {
                        let name = tr["name"].as_str().unwrap_or("").to_string();
                        if !name.is_empty() {
                            tool_names.push(name);
                        }
                    }
                }
                let has_blocks = !reasoning.is_empty()
                    || !opaque.is_empty()
                    || !text.is_empty()
                    || !tool_names.is_empty();
                if !has_blocks {
                    continue;
                }

                let glyph_idx = out.assistant_turns as usize;
                out.assistant_turns += 1;
                out.output_tokens += v["data"]["outputTokens"].as_u64().unwrap_or(0);

                // Histogram + task-list update + owner tracking.
                if let Some(arr) = v["data"]["toolRequests"].as_array() {
                    for tr in arr {
                        let name = tr["name"].as_str().unwrap_or("").to_string();
                        if name.is_empty() || name == SUPPRESSED_TOOL {
                            continue;
                        }
                        increment_tool(&mut out.tool_counts, &mut tool_idx, &name);
                        if name == "task" {
                            let sub = tr["arguments"]["agent_type"]
                                .as_str()
                                .or_else(|| tr["arguments"]["name"].as_str())
                                .unwrap_or("task")
                                .to_string();
                            if seen_task.insert(sub.clone()) {
                                out.tasks.push(sub);
                            }
                        }
                        if let Some(id) = tr["toolCallId"].as_str() {
                            tool_id_to_owner.insert(id.to_string(), (name.clone(), glyph_idx));
                        }
                    }
                }

                let mut glyph = pick_turn_glyph(
                    &tool_names,
                    !reasoning.is_empty() || !opaque.is_empty(),
                    !text.is_empty(),
                    &v["data"]["toolRequests"],
                );
                glyph.turn = glyph_idx as u32;
                out.turns.push(glyph);
            }
            "tool.execution_complete" => {
                if let Some(m) = v["data"]["model"].as_str() {
                    if !m.is_empty() {
                        out.model = m.to_string();
                    }
                }
                let success = v["data"]["success"].as_bool().unwrap_or(true);
                if !success {
                    out.tool_errors += 1;
                    if let Some(id) = v["data"]["toolCallId"].as_str() {
                        if let Some((tname, turn_idx)) = tool_id_to_owner.get(id) {
                            if tname != SUPPRESSED_TOOL {
                                bump_error(&mut out.tool_counts, &tool_idx, tname);
                            }
                            if let Some(g) = out.turns.get_mut(*turn_idx) {
                                g.has_error = true;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Sort histogram: count desc, then alpha asc (matches CC convention).
    out.tool_counts
        .sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    out
}

/// Pick a single glyph + category + label for one Copilot `assistant.message`.
/// Priority: Task > Tool > Thinking > Text. Skill never matches (no Copilot
/// equivalent). `report_intent` is filtered from the dominant-glyph set.
fn pick_turn_glyph(
    tool_names: &[String],
    has_thinking: bool,
    has_text: bool,
    tool_requests: &Value,
) -> TurnGlyph {
    // Task wins outright if any toolReq is a `task` call.
    if let Some(arr) = tool_requests.as_array() {
        for tr in arr {
            if tr["name"].as_str() == Some("task") {
                let sub = tr["arguments"]["agent_type"]
                    .as_str()
                    .or_else(|| tr["arguments"]["name"].as_str())
                    .unwrap_or("task");
                return TurnGlyph {
                    glyph: 'A',
                    category: GlyphCategory::Task,
                    label: format!("Agent → {}", sub),
                    ..Default::default()
                };
            }
        }
    }

    let active: Vec<&str> = tool_names
        .iter()
        .map(String::as_str)
        .filter(|n| *n != SUPPRESSED_TOOL && *n != "task")
        .collect();

    if !active.is_empty() {
        // Dominant tool: most frequent; ties broken by first occurrence.
        let mut counts: HashMap<&str, (u32, usize)> = HashMap::new();
        for (i, n) in active.iter().enumerate() {
            let e = counts.entry(*n).or_insert((0, i));
            e.0 += 1;
        }
        let dominant = counts
            .iter()
            .max_by(|a, b| a.1 .0.cmp(&b.1 .0).then(b.1 .1.cmp(&a.1 .1)))
            .map(|(name, _)| *name)
            .unwrap_or(active[0]);

        // Build "name×count" label, capped at three names.
        let mut name_counts: Vec<(String, u32)> = Vec::new();
        for n in &active {
            if let Some(e) = name_counts.iter_mut().find(|(s, _)| s == *n) {
                e.1 += 1;
            } else {
                name_counts.push((n.to_string(), 1));
            }
        }
        name_counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let label_parts: Vec<String> = name_counts
            .iter()
            .take(3)
            .map(|(n, c)| {
                if *c > 1 {
                    format!("{}×{}", n, c)
                } else {
                    n.clone()
                }
            })
            .collect();
        let mut label = label_parts.join(" + ");
        if name_counts.len() > 3 {
            label.push_str(" + …");
        }

        return TurnGlyph {
            glyph: tool_to_glyph(dominant),
            category: GlyphCategory::Tool,
            label,
            ..Default::default()
        };
    }

    if has_thinking {
        return TurnGlyph {
            glyph: 't',
            category: GlyphCategory::Thinking,
            label: "thinking".to_string(),
            ..Default::default()
        };
    }
    let _ = has_text;
    TurnGlyph {
        glyph: '·',
        category: GlyphCategory::Text,
        label: "text response".to_string(),
        ..Default::default()
    }
}

/// Map a Copilot tool name to its single-char timeline glyph. ASCII-only.
pub fn tool_to_glyph(name: &str) -> char {
    match name {
        "bash" => 'B',
        "view" => 'R',
        "edit" | "str_replace" => 'E',
        "create" | "write" => 'W',
        "task" => 'A',
        "web_fetch" => 'F',
        n if n.starts_with("fetch_") => 'F',
        _ => '+',
    }
}
