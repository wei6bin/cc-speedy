//! Per-turn detail extracted from a Copilot `events.jsonl`. Companion to
//! `turn_detail.rs` (Claude Code). Both produce the same `TurnDetail` so the
//! modal renderer is source-agnostic.

use crate::turn_detail::{
    truncate_at_char_boundary, DetailBlock, ToolResultDetail, TurnDetail, TurnUsage,
    RESULT_BYTE_CAP,
};
use anyhow::{anyhow, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

/// Tool name that fires every Copilot turn as protocol noise. Suppressed from
/// the histogram and dominant-glyph picking but still rendered as a Tool block
/// in the modal.
pub(crate) const SUPPRESSED_TOOL: &str = "report_intent";

pub fn extract_turn(path: &Path, turn_idx: u32) -> Result<TurnDetail> {
    let content = std::fs::read_to_string(path)?;
    extract_turn_from_str(&content, turn_idx)
}

/// Extract the Nth main-thread `assistant.message` from a Copilot events log.
/// "Main-thread" excludes `assistant.message` events with `parentToolCallId`
/// (sub-agent rounds).
pub fn extract_turn_from_str(content: &str, turn_idx: u32) -> Result<TurnDetail> {
    let mut idx_seen: u32 = 0;
    let mut current_user_msg: Option<String> = None;
    // Originating user prompt for the active turn span. Set on `turn_start`,
    // cleared on `turn_end`. While set it overrides current_user_msg so every
    // round inside the span is attributed to the same prompt.
    let mut span_user_msg: Option<String> = None;
    // Most recent model name seen. Used as the initial value for target.model;
    // the forward scan can overwrite it from a tool.execution_complete in span.
    let mut running_model: String = String::new();

    let mut target: Option<TurnDetail> = None;
    let mut id_to_block_idx: HashMap<String, usize> = HashMap::new();

    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let kind = v["type"].as_str().unwrap_or("");

        // Forward scan: target is captured, walk until next message boundary.
        if let Some(td) = target.as_mut() {
            match kind {
                "assistant.message" | "assistant.turn_end" | "user.message" => break,
                "tool.execution_complete" => {
                    let id = v["data"]["toolCallId"].as_str().unwrap_or("");
                    if let Some(&block_idx) = id_to_block_idx.get(id) {
                        let success = v["data"]["success"].as_bool().unwrap_or(true);
                        let raw = pick_result_text(&v["data"]["result"]);
                        let original_bytes = raw.len();
                        let (content_str, truncated) = if original_bytes > RESULT_BYTE_CAP {
                            (truncate_at_char_boundary(&raw, RESULT_BYTE_CAP), true)
                        } else {
                            (raw, false)
                        };
                        if let Some(DetailBlock::Tool { result, .. }) = td.blocks.get_mut(block_idx)
                        {
                            *result = Some(ToolResultDetail {
                                is_error: !success,
                                content: content_str,
                                truncated,
                                original_bytes,
                            });
                        }
                    }
                    if let Some(m) = v["data"]["model"].as_str() {
                        if !m.is_empty() {
                            td.model = m.to_string();
                        }
                    }
                }
                _ => {}
            }
            continue;
        }

        // Pre-target: track running state, possibly capture target.
        match kind {
            "user.message" => {
                let txt = v["data"]["content"].as_str().unwrap_or("").to_string();
                if !txt.is_empty() {
                    current_user_msg = Some(txt);
                }
            }
            "assistant.turn_start" => {
                span_user_msg = current_user_msg.clone();
            }
            "assistant.turn_end" => {
                span_user_msg = None;
            }
            "session.model_change" | "tool.execution_complete" => {
                if let Some(m) = v["data"]["model"].as_str() {
                    if !m.is_empty() {
                        running_model = m.to_string();
                    }
                }
            }
            "assistant.message" => {
                if v["data"]["parentToolCallId"].is_string() {
                    continue;
                }
                let blocks = build_blocks(&v["data"]);
                if blocks.is_empty() {
                    continue;
                }
                if idx_seen == turn_idx {
                    let user_msg = span_user_msg.clone().or_else(|| current_user_msg.clone());

                    let mut id_map: HashMap<String, usize> = HashMap::new();
                    if let Some(arr) = v["data"]["toolRequests"].as_array() {
                        let mut block_indices = blocks.iter().enumerate().filter_map(|(i, b)| {
                            matches!(b, DetailBlock::Tool { .. }).then_some(i)
                        });
                        for tr in arr {
                            if let Some(id) = tr["toolCallId"].as_str() {
                                if let Some(bi) = block_indices.next() {
                                    id_map.insert(id.to_string(), bi);
                                }
                            }
                        }
                    }

                    let usage = TurnUsage {
                        input_tokens: 0,
                        output_tokens: v["data"]["outputTokens"].as_u64().unwrap_or(0),
                        cache_creation: 0,
                        cache_read: 0,
                    };

                    target = Some(TurnDetail {
                        turn_idx,
                        user_msg,
                        blocks,
                        usage,
                        model: running_model.clone(),
                    });
                    id_to_block_idx = id_map;
                }
                idx_seen += 1;
            }
            _ => {}
        }
    }

    target.ok_or_else(|| anyhow!("turn {} out of range (saw {})", turn_idx, idx_seen))
}

/// Build the block list from a single `assistant.message.data` value.
/// Order: Thinking (if any) → Text (if non-empty) → Tool blocks (in
/// `toolRequests` order). Skips a tool entry whose `name` is empty.
fn build_blocks(data: &Value) -> Vec<DetailBlock> {
    let mut blocks: Vec<DetailBlock> = Vec::new();

    let reasoning_text = data["reasoningText"].as_str().unwrap_or("");
    let reasoning_opaque = data["reasoningOpaque"].as_str().unwrap_or("");
    if !reasoning_text.is_empty() {
        blocks.push(DetailBlock::Thinking {
            text: reasoning_text.to_string(),
            redacted: false,
        });
    } else if !reasoning_opaque.is_empty() {
        blocks.push(DetailBlock::Thinking {
            text: String::new(),
            redacted: true,
        });
    }

    let text = data["content"].as_str().unwrap_or("");
    if !text.is_empty() {
        blocks.push(DetailBlock::Text {
            text: text.to_string(),
        });
    }

    if let Some(arr) = data["toolRequests"].as_array() {
        for tr in arr {
            let name = tr["name"].as_str().unwrap_or("").to_string();
            if name.is_empty() {
                continue;
            }
            let input_pretty = serde_json::to_string_pretty(&tr["arguments"])
                .unwrap_or_else(|_| tr["arguments"].to_string());
            blocks.push(DetailBlock::Tool {
                name,
                input_pretty,
                result: None,
            });
        }
    }

    blocks
}

/// Pull the displayable text out of a `tool.execution_complete.data.result`.
/// Prefers `detailedContent` when non-empty (full output), falls back to
/// `content` (often a short summary), then empty string.
fn pick_result_text(v: &Value) -> String {
    let detailed = v["detailedContent"].as_str().unwrap_or("");
    if !detailed.is_empty() {
        return detailed.to_string();
    }
    v["content"].as_str().unwrap_or("").to_string()
}
