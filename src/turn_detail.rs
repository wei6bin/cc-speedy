//! Per-turn detail extracted from a CC JSONL file. Powers the full-screen
//! detail modal opened from the Insights timeline cursor (Phase 3).
//!
//! Scope: a single assistant turn — the user message that triggered it, every
//! content block (thinking / tool_use / text), plus tool_results matched back
//! by tool_use_id. Tool result content is truncated at parse time so the modal
//! stays manageable even for huge Bash outputs.

use anyhow::{anyhow, Result};
use serde_json::Value;
use std::path::Path;

/// Cap a single tool_result content body at this many bytes during parse.
/// Keeps the modal usable even when an upstream Bash dumped megabytes.
pub const RESULT_BYTE_CAP: usize = 8 * 1024;

/// Token usage for the focused turn.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TurnUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation: u64,
    pub cache_read: u64,
}

impl TurnUsage {
    /// Cache hit ratio for this single turn — same formula as
    /// `SessionInsights::cache_hit_pct` but turn-scoped.
    pub fn cache_hit_pct(&self) -> u32 {
        let denom = self.input_tokens + self.cache_read;
        if denom == 0 {
            0
        } else {
            ((self.cache_read * 100) / denom) as u32
        }
    }
}

/// One paired tool_result. `content` may be truncated; check `truncated`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolResultDetail {
    pub is_error: bool,
    pub content: String,
    pub truncated: bool,
    /// Original byte length before truncation. Useful for the renderer label.
    pub original_bytes: usize,
}

/// One content block within an assistant turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetailBlock {
    Thinking {
        /// Plaintext thinking text. Often empty in practice — CC redacts the
        /// actual reasoning into an opaque encrypted `signature` field. When
        /// `text` is empty but `redacted` is true the renderer should show a
        /// placeholder instead of a blank block.
        text: String,
        redacted: bool,
    },
    Text {
        text: String,
    },
    Tool {
        /// Tool name, e.g. "Bash".
        name: String,
        /// Tool input as pretty-printed JSON.
        input_pretty: String,
        /// Matching result, if found in a subsequent user line.
        result: Option<ToolResultDetail>,
    },
}

/// Everything the modal needs to render one assistant turn.
#[derive(Debug, Clone)]
pub struct TurnDetail {
    /// 0-indexed turn position within the session.
    pub turn_idx: u32,
    /// User message text that immediately preceded this turn (if any).
    /// `None` for the first assistant line in the file.
    pub user_msg: Option<String>,
    pub blocks: Vec<DetailBlock>,
    pub usage: TurnUsage,
    pub model: String,
}

/// Extract the Nth assistant turn from a CC JSONL file. `turn_idx` is 0-based.
/// Returns `Err` if the file has fewer than `turn_idx + 1` assistant lines.
pub fn extract_turn(path: &Path, turn_idx: u32) -> Result<TurnDetail> {
    let content = std::fs::read_to_string(path)?;
    extract_turn_from_str(&content, turn_idx)
}

/// String-input variant — the actual implementation; tests call this directly.
pub fn extract_turn_from_str(content: &str, turn_idx: u32) -> Result<TurnDetail> {
    let mut assistant_seen: u32 = 0;
    let mut last_user_msg: Option<String> = None;
    // We collect the target turn here, then in a second loop walk forward to
    // pair tool_results.
    let mut target: Option<TurnDetail> = None;
    let mut tool_use_ids: Vec<String> = Vec::new();

    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let kind = v["type"].as_str().unwrap_or("");

        if target.is_some() {
            // We've already captured the target turn; now scan forward for
            // tool_results until we hit the *next* assistant line.
            if kind == "assistant" {
                break;
            }
            if kind == "user" {
                if let Some(arr) = v["message"]["content"].as_array() {
                    for block in arr {
                        if block["type"].as_str() != Some("tool_result") {
                            continue;
                        }
                        let id = block["tool_use_id"].as_str().unwrap_or("");
                        if id.is_empty() || !tool_use_ids.contains(&id.to_string()) {
                            continue;
                        }
                        let is_error = block["is_error"].as_bool().unwrap_or(false);
                        let raw = stringify_result_content(&block["content"]);
                        let original_bytes = raw.len();
                        let (content, truncated) = if original_bytes > RESULT_BYTE_CAP {
                            (truncate_at_char_boundary(&raw, RESULT_BYTE_CAP), true)
                        } else {
                            (raw, false)
                        };
                        // Attach to the first Tool block without a result. The id
                        // already established this result belongs to *this* turn;
                        // results arrive in the same order as their tool_uses, so
                        // first-unfilled-wins handles the common case correctly.
                        if let Some(td) = target.as_mut() {
                            for b in td.blocks.iter_mut() {
                                if let DetailBlock::Tool { result, .. } = b {
                                    if result.is_none() {
                                        *result = Some(ToolResultDetail {
                                            is_error,
                                            content: content.clone(),
                                            truncated,
                                            original_bytes,
                                        });
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            continue;
        }

        match kind {
            "user" => {
                // Capture string user prompts so we can attribute them to the
                // next assistant turn. tool_result-only user lines don't count.
                let c = &v["message"]["content"];
                if let Some(s) = c.as_str() {
                    last_user_msg = Some(s.to_string());
                } else if c.is_array() {
                    // Skip tool_result-only arrays — they belong to the previous turn.
                    let only_tool_results = c
                        .as_array()
                        .map(|a| {
                            !a.is_empty()
                                && a.iter().all(|b| b["type"].as_str() == Some("tool_result"))
                        })
                        .unwrap_or(false);
                    if !only_tool_results {
                        // Mixed or text array — pull out text-ish content.
                        let text = stringify_user_content(c);
                        if !text.is_empty() {
                            last_user_msg = Some(text);
                        }
                    }
                }
            }
            "assistant" => {
                if assistant_seen == turn_idx {
                    // This is our target.
                    let model = v["message"]["model"].as_str().unwrap_or("").to_string();
                    let usage = parse_usage(&v["message"]["usage"]);
                    let mut blocks: Vec<DetailBlock> = Vec::new();
                    if let Some(arr) = v["message"]["content"].as_array() {
                        for block in arr {
                            match block["type"].as_str() {
                                Some("thinking") => {
                                    let text = block["thinking"]
                                        .as_str()
                                        .or_else(|| block["text"].as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let has_signature = block["signature"]
                                        .as_str()
                                        .map(|s| !s.is_empty())
                                        .unwrap_or(false);
                                    let redacted = text.is_empty() && has_signature;
                                    blocks.push(DetailBlock::Thinking { text, redacted });
                                }
                                Some("text") => {
                                    let text = block["text"].as_str().unwrap_or("").to_string();
                                    blocks.push(DetailBlock::Text { text });
                                }
                                Some("tool_use") => {
                                    let name = block["name"].as_str().unwrap_or("").to_string();
                                    let input_pretty =
                                        serde_json::to_string_pretty(&block["input"])
                                            .unwrap_or_else(|_| block["input"].to_string());
                                    if let Some(id) = block["id"].as_str() {
                                        tool_use_ids.push(id.to_string());
                                    }
                                    blocks.push(DetailBlock::Tool {
                                        name,
                                        input_pretty,
                                        result: None,
                                    });
                                }
                                _ => {}
                            }
                        }
                    }
                    target = Some(TurnDetail {
                        turn_idx,
                        user_msg: last_user_msg.take(),
                        blocks,
                        usage,
                        model,
                    });
                }
                assistant_seen += 1;
            }
            _ => {}
        }
    }

    target.ok_or_else(|| anyhow!("turn {} out of range (saw {})", turn_idx, assistant_seen))
}

fn parse_usage(v: &Value) -> TurnUsage {
    TurnUsage {
        input_tokens: v["input_tokens"].as_u64().unwrap_or(0),
        output_tokens: v["output_tokens"].as_u64().unwrap_or(0),
        cache_creation: v["cache_creation_input_tokens"].as_u64().unwrap_or(0),
        cache_read: v["cache_read_input_tokens"].as_u64().unwrap_or(0),
    }
}

fn stringify_result_content(v: &Value) -> String {
    if let Some(s) = v.as_str() {
        return s.to_string();
    }
    if let Some(arr) = v.as_array() {
        let mut out = String::new();
        for item in arr {
            if item["type"] == "text" {
                if let Some(s) = item["text"].as_str() {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(s);
                }
            } else if let Some(s) = item.as_str() {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(s);
            }
        }
        return out;
    }
    String::new()
}

/// Stringify a user-message `content` field that may be a string, an array of
/// blocks (mixing text/attachments), or null.
fn stringify_user_content(v: &Value) -> String {
    if let Some(s) = v.as_str() {
        return s.to_string();
    }
    if let Some(arr) = v.as_array() {
        let mut out = String::new();
        for item in arr {
            if item["type"] == "text" {
                if let Some(s) = item["text"].as_str() {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(s);
                }
            }
        }
        return out;
    }
    String::new()
}

/// Truncate `s` to at most `max_bytes`, but never split a UTF-8 codepoint.
/// Caller should set `truncated: true` if the returned len differs from s.len().
pub(crate) fn truncate_at_char_boundary(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}
