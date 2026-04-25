//! Per-session insights extracted from a CC JSONL file: token totals, tool/skill
//! histogram, sub-agent dispatches, error counts. Powers the "Insights" panel
//! that sits above the summary in the TUI right pane.
//!
//! v1 supports CC sessions only. OC and Copilot sources should call [`SessionInsights::placeholder`].

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

/// Category of a turn glyph — drives color in the timeline render.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GlyphCategory {
    /// Sub-agent dispatch (`Task` tool).
    Task,
    /// Skill invocation.
    Skill,
    /// Tool use (Bash/Read/Edit/Write/Grep/etc.).
    Tool,
    /// Pure thinking with no tool use.
    Thinking,
    /// Plain text response with no tool use, no thinking.
    #[default]
    Text,
}

/// One assistant turn rendered as a single glyph in the timeline.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TurnGlyph {
    /// 0-indexed assistant turn number.
    pub turn: u32,
    /// Single ASCII char representing the dominant action.
    pub glyph: char,
    /// Color category.
    pub category: GlyphCategory,
    /// True if any tool_use in this turn produced a tool_result with is_error.
    pub has_error: bool,
    /// Short label rendered below the timeline when this turn is focused.
    pub label: String,
}

/// Aggregated, display-ready stats for one session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionInsights {
    /// Last `message.model` seen (the freshest model used in the session).
    pub model: String,
    /// Number of `type:"assistant"` JSONL lines.
    pub assistant_turns: u32,
    /// Sum of `message.usage.input_tokens` across all assistant lines.
    pub input_tokens: u64,
    /// Sum of `message.usage.output_tokens`.
    pub output_tokens: u64,
    /// Sum of `message.usage.cache_creation_input_tokens`.
    pub cache_creation: u64,
    /// Sum of `message.usage.cache_read_input_tokens`.
    pub cache_read: u64,
    /// Tool-use histogram, sorted by total count desc, then alpha. `(name, total, errors)`.
    pub tool_counts: Vec<(String, u32, u32)>,
    /// Skill names invoked, in first-occurrence order. May contain duplicates.
    pub skills: Vec<String>,
    /// `Task` subagent types dispatched, in first-occurrence order.
    pub tasks: Vec<String>,
    /// Count of JSONL lines flagged `isSidechain: true`.
    pub sidechain_lines: u32,
    /// Total `tool_result` blocks with `is_error: true`.
    pub tool_errors: u32,
    /// One glyph per assistant turn, in conversation order. Phase 2 addition;
    /// `#[serde(default)]` so Phase 1 cache rows still deserialize (they'll
    /// surface as empty and trigger a re-parse).
    #[serde(default)]
    pub turns: Vec<TurnGlyph>,
}

impl SessionInsights {
    /// Stable placeholder for sources we don't parse yet (OC, Copilot).
    pub fn placeholder() -> Self {
        Self::default()
    }

    /// True iff the parser found nothing usable — used by the renderer to
    /// fall back to a "no insights" line.
    pub fn is_empty(&self) -> bool {
        self.assistant_turns == 0 && self.tool_counts.is_empty()
    }

    /// Cache-hit ratio as a 0-100 integer percentage. Defined as
    /// `cache_read / (input_tokens + cache_read)`. Returns 0 when the
    /// denominator is zero.
    pub fn cache_hit_pct(&self) -> u32 {
        let denom = self.input_tokens + self.cache_read;
        if denom == 0 {
            0
        } else {
            ((self.cache_read * 100) / denom) as u32
        }
    }
}

/// Parse a CC session JSONL into [`SessionInsights`]. Malformed lines are skipped.
pub fn parse_insights(path: &Path) -> Result<SessionInsights> {
    let content = std::fs::read_to_string(path)?;
    Ok(parse_insights_from_str(&content))
}

/// String-input variant — convenient for tests and avoiding double IO.
pub fn parse_insights_from_str(content: &str) -> SessionInsights {
    let mut out = SessionInsights::default();
    // tool name → (count, errors). Built as an in-order Vec to preserve the
    // first-seen order before final sort; the sort is by count desc.
    let mut tool_idx: std::collections::HashMap<String, usize> = Default::default();

    // tool_use id → (tool name, owning turn index). Used to attribute
    // tool_result errors back to both the histogram and the per-turn glyph.
    let mut tool_id_to_owner: std::collections::HashMap<String, (String, usize)> =
        Default::default();
    let mut seen_skill: std::collections::HashSet<String> = Default::default();
    let mut seen_task: std::collections::HashSet<String> = Default::default();

    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let kind = v["type"].as_str().unwrap_or("");

        if v["isSidechain"].as_bool().unwrap_or(false) {
            out.sidechain_lines += 1;
        }

        match kind {
            "assistant" => {
                let turn_idx = out.assistant_turns as usize;
                out.assistant_turns += 1;
                if let Some(m) = v["message"]["model"].as_str() {
                    if !m.is_empty() {
                        out.model = m.to_string();
                    }
                }
                let usage = &v["message"]["usage"];
                out.input_tokens += usage["input_tokens"].as_u64().unwrap_or(0);
                out.output_tokens += usage["output_tokens"].as_u64().unwrap_or(0);
                out.cache_creation += usage["cache_creation_input_tokens"].as_u64().unwrap_or(0);
                out.cache_read += usage["cache_read_input_tokens"].as_u64().unwrap_or(0);

                let empty: Vec<Value> = Vec::new();
                let blocks = v["message"]["content"].as_array().unwrap_or(&empty);
                for block in blocks {
                    if block["type"].as_str() != Some("tool_use") {
                        continue;
                    }
                    let name = block["name"].as_str().unwrap_or("").to_string();
                    if name.is_empty() {
                        continue;
                    }
                    if let Some(id) = block["id"].as_str() {
                        tool_id_to_owner.insert(id.to_string(), (name.clone(), turn_idx));
                    }
                    increment_tool(&mut out.tool_counts, &mut tool_idx, &name);

                    match name.as_str() {
                        "Skill" => {
                            if let Some(s) = block["input"]["skill"].as_str() {
                                if seen_skill.insert(s.to_string()) {
                                    out.skills.push(s.to_string());
                                }
                            }
                        }
                        "Task" => {
                            let sub = block["input"]["subagent_type"]
                                .as_str()
                                .unwrap_or("general-purpose")
                                .to_string();
                            if seen_task.insert(sub.clone()) {
                                out.tasks.push(sub);
                            }
                        }
                        _ => {}
                    }
                }

                // Build the glyph for this turn from the same block array.
                let mut g = pick_turn_glyph(blocks);
                g.turn = turn_idx as u32;
                out.turns.push(g);
            }
            "user" => {
                // user.content can be a string OR a tool_result array.
                if let Some(arr) = v["message"]["content"].as_array() {
                    for block in arr {
                        if block["type"].as_str() != Some("tool_result") {
                            continue;
                        }
                        if !block["is_error"].as_bool().unwrap_or(false) {
                            continue;
                        }
                        out.tool_errors += 1;
                        if let Some(id) = block["tool_use_id"].as_str() {
                            if let Some((tname, turn_idx)) = tool_id_to_owner.get(id) {
                                bump_error(&mut out.tool_counts, &tool_idx, tname);
                                if let Some(t) = out.turns.get_mut(*turn_idx) {
                                    t.has_error = true;
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Sort histogram: count desc, then alpha asc.
    out.tool_counts
        .sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    out
}

/// Pick a single glyph + category + label for one assistant turn from its
/// content blocks. Priority: Task > Skill > tool > thinking > text.
fn pick_turn_glyph(blocks: &[Value]) -> TurnGlyph {
    // Pass 1: classify what's present.
    let mut task_sub: Option<String> = None;
    let mut skill_name: Option<String> = None;
    let mut tools: Vec<String> = Vec::new();
    let mut has_thinking = false;
    let mut has_text = false;

    for b in blocks {
        match b["type"].as_str() {
            Some("tool_use") => {
                let name = b["name"].as_str().unwrap_or("");
                match name {
                    "" => {}
                    "Task" => {
                        if task_sub.is_none() {
                            task_sub = Some(
                                b["input"]["subagent_type"]
                                    .as_str()
                                    .unwrap_or("general")
                                    .to_string(),
                            );
                        }
                    }
                    "Skill" => {
                        if skill_name.is_none() {
                            skill_name =
                                Some(b["input"]["skill"].as_str().unwrap_or("skill").to_string());
                        }
                    }
                    other => tools.push(other.to_string()),
                }
            }
            Some("thinking") => has_thinking = true,
            Some("text") => has_text = true,
            _ => {}
        }
    }

    // Pass 2: pick the dominant glyph.
    if let Some(sub) = task_sub {
        return TurnGlyph {
            glyph: 'A',
            category: GlyphCategory::Task,
            label: format!("Agent → {}", sub),
            ..Default::default()
        };
    }
    if let Some(s) = skill_name {
        let short = s.split(':').next_back().unwrap_or(&s).to_string();
        return TurnGlyph {
            glyph: 'S',
            category: GlyphCategory::Skill,
            label: format!("Skill: {}", short),
            ..Default::default()
        };
    }
    if !tools.is_empty() {
        // Most-common tool wins; ties broken by first-occurrence.
        let mut counts: std::collections::HashMap<&str, (u32, usize)> =
            std::collections::HashMap::new();
        for (i, t) in tools.iter().enumerate() {
            let e = counts.entry(t.as_str()).or_insert((0, i));
            e.0 += 1;
        }
        let dominant = counts
            .iter()
            .max_by(|a, b| a.1 .0.cmp(&b.1 .0).then(b.1 .1.cmp(&a.1 .1)))
            .map(|(name, _)| *name)
            .unwrap_or(tools[0].as_str());

        // Build label "Bash×3 + Read" style, capped at three names.
        let mut name_counts: Vec<(String, u32)> = Vec::new();
        for t in &tools {
            if let Some(e) = name_counts.iter_mut().find(|(n, _)| n == t) {
                e.1 += 1;
            } else {
                name_counts.push((t.clone(), 1));
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

/// Map a tool name to its single-char timeline glyph. ASCII-only for portability.
pub fn tool_to_glyph(name: &str) -> char {
    match name {
        "Bash" => 'B',
        "Read" => 'R',
        "Edit" | "MultiEdit" => 'E',
        "Write" => 'W',
        "Grep" => 'g',
        "Glob" => 'G',
        "WebFetch" | "WebSearch" => 'F',
        "TodoWrite" | "TaskCreate" | "TaskUpdate" | "TaskList" | "TaskGet" => '*',
        "ToolSearch" => '?',
        _ => '+',
    }
}

fn increment_tool(
    counts: &mut Vec<(String, u32, u32)>,
    idx: &mut std::collections::HashMap<String, usize>,
    name: &str,
) {
    if let Some(&i) = idx.get(name) {
        counts[i].1 += 1;
    } else {
        idx.insert(name.to_string(), counts.len());
        counts.push((name.to_string(), 1, 0));
    }
}

fn bump_error(
    counts: &mut [(String, u32, u32)],
    idx: &std::collections::HashMap<String, usize>,
    name: &str,
) {
    if let Some(&i) = idx.get(name) {
        counts[i].2 += 1;
    }
}

/// Format a token count as a short human string: 1234 → "1.2k", 56789 → "57k".
pub fn fmt_tokens(n: u64) -> String {
    if n < 1_000 {
        n.to_string()
    } else if n < 10_000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else if n < 1_000_000 {
        format!("{}k", n / 1000)
    } else {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    }
}
