# Copilot Turn Detail + Insights Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring the per-turn detail modal and Insights timeline glyphs to GitHub Copilot CLI sessions in `cc-speedy`, mirroring the existing Claude Code feature.

**Architecture:** Two new sibling modules (`copilot_turn_detail.rs`, `copilot_insights.rs`) parse Copilot's `events.jsonl` into the existing `TurnDetail` and `SessionInsights` types. The TUI rendering layer is unchanged — it just dispatches by `SessionSource` to call the right parser. Tests are inline-fixture style (raw-string JSONL), mirroring `tests/turn_detail_test.rs`.

**Tech Stack:** Rust 1.x, `serde_json::Value` for ad-hoc JSON access, `anyhow` for errors, `tokio::task::spawn_blocking` for the existing background insights loader.

**Reference spec:** `docs/superpowers/specs/2026-04-26-copilot-turn-detail-design.md`

---

## File Structure

| Path | Status | Responsibility |
|---|---|---|
| `src/copilot_turn_detail.rs` | NEW | `extract_turn(path, idx) -> Result<TurnDetail>` for Copilot sessions. Re-uses the shared `TurnDetail`/`DetailBlock` types from `turn_detail.rs`. |
| `src/copilot_insights.rs` | NEW | `parse_insights(path) -> Result<SessionInsights>` for Copilot sessions. Re-uses `SessionInsights`/`TurnGlyph`/`GlyphCategory` from `insights.rs`. |
| `src/turn_detail.rs` | MODIFY | Expose `truncate_at_char_boundary` as `pub(crate)` so the Copilot parser can reuse it. |
| `src/insights.rs` | MODIFY | Expose `increment_tool` and `bump_error` as `pub(crate)` to share the histogram-building helpers. |
| `src/lib.rs` | MODIFY | `pub mod copilot_turn_detail;` and `pub mod copilot_insights;` |
| `src/tui.rs` | MODIFY | Four dispatch points: `open_turn_detail`, `maybe_spawn_insights_load`, the `show_insights` predicate, and the Enter "want_detail" gate. |
| `tests/copilot_turn_detail_test.rs` | NEW | Inline-fixture tests for the Copilot turn extractor. |
| `tests/copilot_insights_test.rs` | NEW | Inline-fixture tests for the Copilot insights parser. |

---

## Task 0: Verify baseline

**Files:** none

- [ ] **Step 1: Build and test from a clean state**

```bash
cargo build
cargo test
```

Expected: build succeeds, all tests pass. If anything is broken at HEAD, stop and fix or rebase before continuing.

---

## Task 1: Stub modules and expose helpers

Create empty parser modules with `unimplemented!()` bodies, register them in `lib.rs`, and widen visibility on three private helpers we'll reuse from CC parsers. No tests yet — this lets every later task write tests that compile.

**Files:**
- Create: `src/copilot_turn_detail.rs`
- Create: `src/copilot_insights.rs`
- Modify: `src/lib.rs`
- Modify: `src/turn_detail.rs:303` — change `fn truncate_at_char_boundary` to `pub(crate) fn`
- Modify: `src/insights.rs:358` — change `fn increment_tool` to `pub(crate) fn`
- Modify: `src/insights.rs:371` — change `fn bump_error` to `pub(crate) fn`

- [ ] **Step 1: Create `src/copilot_turn_detail.rs`**

```rust
//! Per-turn detail extracted from a Copilot `events.jsonl`. Companion to
//! `turn_detail.rs` (Claude Code). Both produce the same `TurnDetail` so the
//! modal renderer is source-agnostic.

use crate::turn_detail::TurnDetail;
use anyhow::Result;
use std::path::Path;

/// Tool name that fires every Copilot turn as protocol noise. Suppressed from
/// the histogram and dominant-glyph picking but still rendered as a Tool block
/// in the modal.
pub(crate) const SUPPRESSED_TOOL: &str = "report_intent";

pub fn extract_turn(path: &Path, turn_idx: u32) -> Result<TurnDetail> {
    let content = std::fs::read_to_string(path)?;
    extract_turn_from_str(&content, turn_idx)
}

pub fn extract_turn_from_str(_content: &str, _turn_idx: u32) -> Result<TurnDetail> {
    unimplemented!("Task 2 will fill this in")
}
```

- [ ] **Step 2: Create `src/copilot_insights.rs`**

```rust
//! Per-session insights extracted from a Copilot `events.jsonl`. Companion to
//! `insights.rs` (Claude Code). Both produce the same `SessionInsights` so the
//! Insights panel renderer is source-agnostic.

use crate::insights::SessionInsights;
use anyhow::Result;
use std::path::Path;

pub fn parse_insights(path: &Path) -> Result<SessionInsights> {
    let content = std::fs::read_to_string(path)?;
    Ok(parse_insights_from_str(&content))
}

pub fn parse_insights_from_str(_content: &str) -> SessionInsights {
    unimplemented!("Task 7 will fill this in")
}
```

- [ ] **Step 3: Register the modules in `src/lib.rs`**

Insert two lines so the file ends up as:

```rust
pub mod copilot_insights;
pub mod copilot_sessions;
pub mod copilot_turn_detail;
pub mod digest;
pub mod git_status;
pub mod insights;
pub mod install;
pub mod obsidian;
pub mod obsidian_cli;
pub mod opencode_sessions;
pub mod sessions;
pub mod settings;
pub mod store;
pub mod summary;
pub mod theme;
pub mod tmux;
pub mod tui;
pub mod turn_detail;
pub mod unified;
pub mod update;
pub mod util;
```

- [ ] **Step 4: Widen `truncate_at_char_boundary` visibility in `src/turn_detail.rs`**

Find the function at line 303 and change:

```rust
fn truncate_at_char_boundary(s: &str, max_bytes: usize) -> String {
```

to:

```rust
pub(crate) fn truncate_at_char_boundary(s: &str, max_bytes: usize) -> String {
```

- [ ] **Step 5: Widen `increment_tool` and `bump_error` visibility in `src/insights.rs`**

Find these two functions near line 358 and 371 respectively. Prepend `pub(crate)` to each `fn`.

- [ ] **Step 6: Verify the crate still builds**

Run: `cargo build`
Expected: clean build, no warnings about unused stubs (the `unimplemented!` bodies aren't called yet).

- [ ] **Step 7: Commit**

```bash
git add src/copilot_turn_detail.rs src/copilot_insights.rs src/lib.rs src/turn_detail.rs src/insights.rs
git commit -m "scaffold: stub copilot_turn_detail + copilot_insights modules"
```

---

## Task 2: Implement `extract_turn_from_str` (TDD: simple turn)

Write a failing test for a simple Copilot turn (one user prompt, one assistant.message with thinking + text + one tool, one tool.execution_complete), then implement the full parser per spec. Subsequent tasks (3–6) only add tests; the implementation here should be complete.

**Files:**
- Create: `tests/copilot_turn_detail_test.rs`
- Modify: `src/copilot_turn_detail.rs`

- [ ] **Step 1: Create the test file with the simple-turn fixture and test**

```rust
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
        DetailBlock::Tool { name, input_pretty, result } => {
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
```

- [ ] **Step 2: Run the test and watch it fail**

Run: `cargo test --test copilot_turn_detail_test turn_0_basic_extraction`
Expected: FAIL — panics with `not implemented: Task 2 will fill this in`.

- [ ] **Step 3: Implement `extract_turn_from_str` in full**

Replace the body of `src/copilot_turn_detail.rs` with:

```rust
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
                        if let Some(DetailBlock::Tool { result, .. }) =
                            td.blocks.get_mut(block_idx)
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
                    let user_msg = span_user_msg
                        .clone()
                        .or_else(|| current_user_msg.clone());

                    let mut id_map: HashMap<String, usize> = HashMap::new();
                    if let Some(arr) = v["data"]["toolRequests"].as_array() {
                        let mut block_indices = blocks
                            .iter()
                            .enumerate()
                            .filter_map(|(i, b)| {
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
```

- [ ] **Step 4: Run the test and watch it pass**

Run: `cargo test --test copilot_turn_detail_test turn_0_basic_extraction`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/copilot_turn_detail.rs tests/copilot_turn_detail_test.rs
git commit -m "feat(copilot): extract per-turn detail from events.jsonl"
```

---

## Task 3: Test multi-round turn (user_msg attribution)

A single user prompt can spawn multiple `assistant.message` rounds inside one `turn_start..turn_end` span. Each round becomes its own `turn_idx`, but all of them must report the same originating prompt. This task adds the test; the Task 2 implementation already handles it via `span_user_msg`.

**Files:**
- Modify: `tests/copilot_turn_detail_test.rs`

- [ ] **Step 1: Append the multi-round fixture and test**

Append to the test file (after the existing test):

```rust
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
        assert_eq!(t.user_msg.as_deref(), Some("refactor X"),
            "round {} should keep the originating prompt", idx);
        assert_eq!(t.turn_idx, idx);
    }
    let final_round = extract_turn_from_str(MULTI_ROUND, 2).unwrap();
    assert_eq!(final_round.blocks.len(), 1);
    match &final_round.blocks[0] {
        DetailBlock::Text { text } => assert_eq!(text, "Done."),
        other => panic!("expected Text, got {:?}", other),
    }
}
```

- [ ] **Step 2: Run the new test**

Run: `cargo test --test copilot_turn_detail_test multi_round_turn`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add tests/copilot_turn_detail_test.rs
git commit -m "test(copilot): user_msg attribution survives multiple rounds"
```

---

## Task 4: Test parallel tool ID matching

Copilot tools execute concurrently; `tool.execution_complete` events can arrive in any order. The parser pairs them by exact `toolCallId`, so reverse-order completions still match the right blocks.

**Files:**
- Modify: `tests/copilot_turn_detail_test.rs`

- [ ] **Step 1: Append the parallel-tools fixture and test**

```rust
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
            DetailBlock::Tool { result: Some(r), .. } => Some(r.content.as_str()),
            _ => None,
        })
        .collect();
    // Block order matches toolRequests order: A, B, C.
    assert_eq!(outputs, vec!["out-a", "out-b", "out-c"]);
}
```

- [ ] **Step 2: Run the test**

Run: `cargo test --test copilot_turn_detail_test parallel_tool_completions_pair_by_id`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add tests/copilot_turn_detail_test.rs
git commit -m "test(copilot): tool results pair by id under reordered completions"
```

---

## Task 5: Test failed tool + redacted thinking

`success: false` propagates as `is_error: true`. When `reasoningText` is empty but `reasoningOpaque` is set, the Thinking block is redacted.

**Files:**
- Modify: `tests/copilot_turn_detail_test.rs`

- [ ] **Step 1: Append the two fixtures and tests**

```rust
const FAILED_TOOL: &str = r#"{"type":"user.message","data":{"content":"try it"},"id":"u1","timestamp":"2026-04-26T10:00:00Z"}
{"type":"assistant.turn_start","data":{"turnId":"0"},"id":"e1","timestamp":"2026-04-26T10:00:01Z"}
{"type":"assistant.message","data":{"content":"","toolRequests":[{"toolCallId":"f1","name":"bash","arguments":{"cmd":"missing"},"type":"function"}],"reasoningText":"running","outputTokens":3},"id":"e2","timestamp":"2026-04-26T10:00:02Z"}
{"type":"tool.execution_complete","data":{"toolCallId":"f1","model":"claude-sonnet-4.6","success":false,"result":{}},"id":"e3","timestamp":"2026-04-26T10:00:03Z"}
{"type":"assistant.turn_end","data":{"turnId":"0"},"id":"e4","timestamp":"2026-04-26T10:00:04Z"}
"#;

#[test]
fn failed_tool_marked_as_error() {
    let t = extract_turn_from_str(FAILED_TOOL, 0).unwrap();
    let tool = t.blocks.iter().find_map(|b| match b {
        DetailBlock::Tool { result, .. } => result.as_ref(),
        _ => None,
    }).expect("tool block with result");
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
```

- [ ] **Step 2: Run the new tests**

Run: `cargo test --test copilot_turn_detail_test failed_tool_marked_as_error redacted_thinking_block`
Expected: both PASS.

- [ ] **Step 3: Commit**

```bash
git add tests/copilot_turn_detail_test.rs
git commit -m "test(copilot): failed tool + redacted thinking edge cases"
```

---

## Task 6: Test sub-agent filter, large result truncation, out-of-range

Three remaining edges. Sub-agent rounds (`parentToolCallId` set) don't claim glyph slots. Tool results larger than `RESULT_BYTE_CAP` get truncated at a UTF-8 char boundary. Asking for a turn beyond the file returns `Err`.

**Files:**
- Modify: `tests/copilot_turn_detail_test.rs`

- [ ] **Step 1: Append the three fixtures and tests**

```rust
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
    // Build a fixture whose detailedContent is well past RESULT_BYTE_CAP.
    let huge = "a".repeat(RESULT_BYTE_CAP * 2);
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
    let r = t.blocks.iter().find_map(|b| match b {
        DetailBlock::Tool { result, .. } => result.as_ref(),
        _ => None,
    }).unwrap();
    assert!(r.truncated);
    assert!(r.content.len() <= RESULT_BYTE_CAP);
    assert_eq!(r.original_bytes, huge.len());
}

#[test]
fn out_of_range_returns_err() {
    let res = extract_turn_from_str(SIMPLE, 99);
    assert!(res.is_err());
}
```

- [ ] **Step 2: Run the new tests**

Run: `cargo test --test copilot_turn_detail_test`
Expected: all 7 tests pass (3 new + 4 from earlier tasks).

- [ ] **Step 3: Commit**

```bash
git add tests/copilot_turn_detail_test.rs
git commit -m "test(copilot): subagent filter, large-result truncation, out-of-range"
```

---

## Task 7: Implement `parse_insights_from_str` (TDD: basic counts)

Write a failing test that exercises model + assistant_turns + output_tokens + sidechain_lines, then implement the full parser. Subsequent tasks (8–10) only add tests.

**Files:**
- Create: `tests/copilot_insights_test.rs`
- Modify: `src/copilot_insights.rs`

- [ ] **Step 1: Create the test file with the basic-counts fixture and test**

```rust
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
```

- [ ] **Step 2: Run the test and watch it fail**

Run: `cargo test --test copilot_insights_test basic_counts`
Expected: FAIL — panics with `not implemented: Task 7 will fill this in`.

- [ ] **Step 3: Replace `src/copilot_insights.rs` with the full implementation**

```rust
//! Per-session insights extracted from a Copilot `events.jsonl`. Companion to
//! `insights.rs` (Claude Code). Both produce the same `SessionInsights` so the
//! Insights panel renderer is source-agnostic.

use crate::copilot_turn_detail::SUPPRESSED_TOOL;
use crate::insights::{
    bump_error, increment_tool, GlyphCategory, SessionInsights, TurnGlyph,
};
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
                            tool_id_to_owner
                                .insert(id.to_string(), (name.clone(), glyph_idx));
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
```

- [ ] **Step 4: Run the test and watch it pass**

Run: `cargo test --test copilot_insights_test basic_counts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/copilot_insights.rs tests/copilot_insights_test.rs
git commit -m "feat(copilot): parse session insights from events.jsonl"
```

---

## Task 8: Test tool histogram + report_intent suppression + tool_errors

Verify that `report_intent` is suppressed from the histogram, that real tool counts come through, and that failed completions land in both `tool_errors` and the per-tool `errors` column.

**Files:**
- Modify: `tests/copilot_insights_test.rs`

- [ ] **Step 1: Append the fixture and tests**

```rust
const HISTOGRAM: &str = r#"{"type":"user.message","data":{"content":"go"},"id":"u1","timestamp":"2026-04-26T10:00:00Z"}
{"type":"assistant.turn_start","data":{"turnId":"0"},"id":"e1","timestamp":"2026-04-26T10:00:01Z"}
{"type":"assistant.message","data":{"content":"","toolRequests":[{"toolCallId":"r1","name":"report_intent","arguments":{"intent":"x"},"type":"function"},{"toolCallId":"b1","name":"bash","arguments":{"cmd":"ls"},"type":"function"},{"toolCallId":"b2","name":"bash","arguments":{"cmd":"cat x"},"type":"function"},{"toolCallId":"v1","name":"view","arguments":{"path":"/y"},"type":"function"}],"reasoningText":"","outputTokens":1},"id":"e2","timestamp":"2026-04-26T10:00:02Z"}
{"type":"tool.execution_complete","data":{"toolCallId":"r1","model":"claude-sonnet-4.6","success":true,"result":{"content":"","detailedContent":""}},"id":"e3","timestamp":"2026-04-26T10:00:03Z"}
{"type":"tool.execution_complete","data":{"toolCallId":"b1","model":"claude-sonnet-4.6","success":true,"result":{"content":"","detailedContent":"ok"}},"id":"e4","timestamp":"2026-04-26T10:00:04Z"}
{"type":"tool.execution_complete","data":{"toolCallId":"b2","model":"claude-sonnet-4.6","success":false,"result":{}},"id":"e5","timestamp":"2026-04-26T10:00:05Z"}
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
```

- [ ] **Step 2: Run the new tests**

Run: `cargo test --test copilot_insights_test`
Expected: 3 tests pass (1 from Task 7 + 2 new).

- [ ] **Step 3: Commit**

```bash
git add tests/copilot_insights_test.rs
git commit -m "test(copilot): histogram suppresses report_intent + counts errors"
```

---

## Task 9: Test tasks list, skills empty, model attribution

Verify that the `task` tool's `agent_type` ends up in `tasks`, that `skills` is empty (no Copilot equivalent), and that `model` is sourced from `tool.execution_complete` even before any `session.model_change`.

**Files:**
- Modify: `tests/copilot_insights_test.rs`

- [ ] **Step 1: Append the fixture and tests**

```rust
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
    assert_eq!(i.tasks, vec!["explorer".to_string(), "reviewer".to_string()]);
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
```

- [ ] **Step 2: Run the new tests**

Run: `cargo test --test copilot_insights_test`
Expected: 6 tests pass.

- [ ] **Step 3: Commit**

```bash
git add tests/copilot_insights_test.rs
git commit -m "test(copilot): tasks dedup, skills empty, model from tool_complete"
```

---

## Task 10: Test glyph picker priority + tool dominance

Verify Task > Tool > Thinking > Text priority, and that the dominant tool wins inside the Tool category.

**Files:**
- Modify: `tests/copilot_insights_test.rs`

- [ ] **Step 1: Append the fixture and tests**

```rust
const PRIORITY: &str = r#"{"type":"user.message","data":{"content":"mix"},"id":"u1","timestamp":"2026-04-26T10:00:00Z"}
{"type":"assistant.turn_start","data":{"turnId":"0"},"id":"e1","timestamp":"2026-04-26T10:00:01Z"}
{"type":"assistant.message","data":{"content":"","toolRequests":[{"toolCallId":"t1","name":"task","arguments":{"agent_type":"explorer"},"type":"function"},{"toolCallId":"b1","name":"bash","arguments":{},"type":"function"}],"reasoningText":"r","outputTokens":1},"id":"e2","timestamp":"2026-04-26T10:00:02Z"}
{"type":"tool.execution_complete","data":{"toolCallId":"t1","model":"m","success":true,"result":{"content":"","detailedContent":""}},"id":"e3","timestamp":"2026-04-26T10:00:03Z"}
{"type":"tool.execution_complete","data":{"toolCallId":"b1","model":"m","success":true,"result":{"content":"","detailedContent":""}},"id":"e4","timestamp":"2026-04-26T10:00:04Z"}
{"type":"assistant.message","data":{"content":"","toolRequests":[{"toolCallId":"r1","name":"report_intent","arguments":{},"type":"function"},{"toolCallId":"b2","name":"bash","arguments":{},"type":"function"},{"toolCallId":"b3","name":"bash","arguments":{},"type":"function"},{"toolCallId":"v1","name":"view","arguments":{},"type":"function"}],"reasoningText":"r","outputTokens":1},"id":"e5","timestamp":"2026-04-26T10:00:05Z"}
{"type":"tool.execution_complete","data":{"toolCallId":"r1","model":"m","success":true,"result":{"content":"","detailedContent":""}},"id":"e6","timestamp":"2026-04-26T10:00:06Z"}
{"type":"tool.execution_complete","data":{"toolCallId":"b2","model":"m","success":true,"result":{"content":"","detailedContent":""}},"id":"e7","timestamp":"2026-04-26T10:00:07Z"}
{"type":"tool.execution_complete","data":{"toolCallId":"b3","model":"m","success":true,"result":{"content":"","detailedContent":""}},"id":"e8","timestamp":"2026-04-26T10:00:08Z"}
{"type":"tool.execution_complete","data":{"toolCallId":"v1","model":"m","success":true,"result":{"content":"","detailedContent":""}},"id":"e9","timestamp":"2026-04-26T10:00:09Z"}
{"type":"assistant.message","data":{"content":"all done","toolRequests":[],"reasoningText":"","outputTokens":1},"id":"e10","timestamp":"2026-04-26T10:00:10Z"}
{"type":"assistant.message","data":{"content":"","toolRequests":[],"reasoningText":"just thinking","outputTokens":1},"id":"e11","timestamp":"2026-04-26T10:00:11Z"}
{"type":"assistant.turn_end","data":{"turnId":"0"},"id":"e12","timestamp":"2026-04-26T10:00:12Z"}
"#;

#[test]
fn glyph_priority_task_over_tool() {
    let i = parse_insights_from_str(PRIORITY);
    assert!(matches!(i.turns[0].category, GlyphCategory::Task));
    assert_eq!(i.turns[0].glyph, 'A');
    assert!(i.turns[0].label.starts_with("Agent → explorer"));
}

#[test]
fn glyph_dominant_tool_wins() {
    let i = parse_insights_from_str(PRIORITY);
    // Turn 1 has report_intent (suppressed) + bash×2 + view; bash wins.
    assert!(matches!(i.turns[1].category, GlyphCategory::Tool));
    assert_eq!(i.turns[1].glyph, 'B');
    assert!(i.turns[1].label.starts_with("bash×2"));
}

#[test]
fn glyph_text_and_thinking_categories() {
    let i = parse_insights_from_str(PRIORITY);
    assert!(matches!(i.turns[2].category, GlyphCategory::Text));
    assert_eq!(i.turns[2].glyph, '·');
    assert!(matches!(i.turns[3].category, GlyphCategory::Thinking));
    assert_eq!(i.turns[3].glyph, 't');
}
```

- [ ] **Step 2: Run the new tests**

Run: `cargo test --test copilot_insights_test`
Expected: 9 tests pass.

- [ ] **Step 3: Commit**

```bash
git add tests/copilot_insights_test.rs
git commit -m "test(copilot): glyph priority + dominant-tool selection"
```

---

## Task 11: TUI dispatch — `open_turn_detail` + Enter gate

Wire the new Copilot extractor into the TUI so `Enter` on the timeline glyph cursor opens the turn-detail modal for Copilot sessions.

**Files:**
- Modify: `src/tui.rs`

- [ ] **Step 1: Update `open_turn_detail` to dispatch by source**

Find `open_turn_detail` (around line 639). The current function unconditionally calls `crate::turn_detail::extract_turn`. Replace its body so it switches on `s.source`:

```rust
fn open_turn_detail(app: &mut AppState) {
    let Some(turn_idx) = app.glyph_cursor else {
        return;
    };
    let Some(s) = app.selected_session() else {
        return;
    };
    let Some(jsonl) = s.jsonl_path.clone() else {
        return;
    };
    let source = s.source;
    let path = std::path::Path::new(&jsonl);
    let extracted = match source {
        SessionSource::ClaudeCode => crate::turn_detail::extract_turn(path, turn_idx as u32),
        SessionSource::Copilot => {
            crate::copilot_turn_detail::extract_turn(path, turn_idx as u32)
        }
        SessionSource::OpenCode => {
            app.status_msg = Some((
                "turn detail not supported for OpenCode sessions yet".to_string(),
                Instant::now(),
            ));
            return;
        }
    };
    match extracted {
        Ok(td) => {
            app.turn_detail_expanded = default_expansion(&td.blocks);
            app.turn_detail_focused = 0;
            app.turn_detail = Some(td);
            app.turn_detail_scroll = 0;
            app.mode = AppMode::TurnDetail;
        }
        Err(e) => {
            app.status_msg = Some((format!("turn detail: {e}"), Instant::now()));
        }
    }
}
```

- [ ] **Step 2: Update the Enter "want_detail" gate**

Find the block around line 1500 that decides whether Enter opens turn detail. The current condition checks `s.source == SessionSource::ClaudeCode`. Change it to allow Copilot too:

```rust
let want_detail = app.mode == AppMode::Normal
    && app.insights_visible
    && app.glyph_cursor.is_some()
    && app
        .selected_session()
        .map(|s| matches!(s.source, SessionSource::ClaudeCode | SessionSource::Copilot))
        .unwrap_or(false);
```

- [ ] **Step 3: Build and run the existing test suite**

Run: `cargo build && cargo test`
Expected: all tests still pass; no new compile errors.

- [ ] **Step 4: Commit**

```bash
git add src/tui.rs
git commit -m "feat(tui): dispatch open_turn_detail to copilot parser"
```

---

## Task 12: TUI dispatch — `maybe_spawn_insights_load` + `show_insights`

Allow Insights parsing to fire for Copilot sessions, and let the right-pane Insights panel render for them.

**Files:**
- Modify: `src/tui.rs`

- [ ] **Step 1: Update `maybe_spawn_insights_load`**

Find it around line 729. Today it early-returns when `s.source != SessionSource::ClaudeCode`, then calls `crate::insights::parse_insights`. Replace the early return and the parse call so Copilot is supported:

```rust
fn maybe_spawn_insights_load(app: &AppState) {
    let Some(s) = app.selected_session() else {
        return;
    };
    let source = s.source;
    if matches!(source, SessionSource::OpenCode) {
        return;
    }
    let Some(jsonl) = s.jsonl_path.clone() else {
        return;
    };
    let session_id = s.session_id.clone();

    let live_mtime = match std::fs::metadata(&jsonl).and_then(|m| m.modified()) {
        Ok(t) => t
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
        Err(_) => return,
    };

    {
        let cache = app.insights_cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(c) = cache.get(&session_id) {
            let turns_ok = c.insights.assistant_turns == 0 || !c.insights.turns.is_empty();
            if c.source_mtime >= live_mtime && turns_ok {
                return;
            }
        }
    }
    {
        let mut loading = app
            .insights_loading
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if !loading.insert(session_id.clone()) {
            return;
        }
    }

    let cache = app.insights_cache.clone();
    let loading = app.insights_loading.clone();
    let db = app.db.clone();
    let source_label = match source {
        SessionSource::ClaudeCode => "cc",
        SessionSource::Copilot => "copilot",
        SessionSource::OpenCode => unreachable!("filtered above"),
    };

    tokio::task::spawn_blocking(move || {
        let parsed = match source {
            SessionSource::ClaudeCode => {
                crate::insights::parse_insights(std::path::Path::new(&jsonl))
            }
            SessionSource::Copilot => {
                crate::copilot_insights::parse_insights(std::path::Path::new(&jsonl))
            }
            SessionSource::OpenCode => unreachable!(),
        };
        if let Ok(insights) = parsed {
            if let Ok(conn) = db.lock() {
                let _ = crate::store::save_insights(
                    &conn,
                    &session_id,
                    source_label,
                    live_mtime,
                    &insights,
                );
            }
            cache.lock().unwrap_or_else(|e| e.into_inner()).insert(
                session_id.clone(),
                crate::store::CachedInsights {
                    source_mtime: live_mtime,
                    insights,
                },
            );
        }
        loading
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&session_id);
    });
}
```

- [ ] **Step 2: Update the `show_insights` predicate**

Find it around line 2406. Replace:

```rust
let show_insights = app.insights_visible
    && app
        .selected_session()
        .map(|s| s.source == SessionSource::ClaudeCode)
        .unwrap_or(false);
```

with:

```rust
let show_insights = app.insights_visible
    && app
        .selected_session()
        .map(|s| !matches!(s.source, SessionSource::OpenCode))
        .unwrap_or(false);
```

- [ ] **Step 3: Build, lint, and run the full test suite**

Run: `cargo build && cargo clippy --no-deps && cargo test`
Expected: clean build, no new clippy warnings, all tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/tui.rs
git commit -m "feat(tui): render Insights panel + load parser for copilot sessions"
```

---

## Task 13: Manual smoke test on a real Copilot session

The unit tests cover the parsers; this step validates the end-to-end UX. No commit required if everything works.

**Files:** none

- [ ] **Step 1: Run the TUI**

```bash
cargo run
```

- [ ] **Step 2: Verify the Insights panel renders for a Copilot session**

In the session list:
1. Press `3` to filter to Copilot only.
2. Move the highlight to a session with at least a few turns. The right pane should show an Insights panel with model name, turn count, output tokens (input/cache will be 0), tool histogram (no `report_intent`), and a glyph timeline.

Expected: panel populates within ~1 second (initial parse), or instantly if previously cached.

- [ ] **Step 3: Verify the turn-detail modal**

1. Press `i` if needed to ensure Insights is visible.
2. Use `[` / `]` to move the glyph cursor across the timeline.
3. Press `Enter`. The modal should open with the user prompt at the top, the assistant blocks (Thinking with visible reasoningText, Text, Tool calls with their results), and footer stats.
4. Try `[` and `]` while the modal is open to navigate between adjacent turns.
5. Press `Esc` to close.

Expected: modal renders correctly. Multi-round turns within the same user prompt all show the same `user_msg`. Failed tools show as errors. Sub-agent rounds are not navigated to.

- [ ] **Step 4: Verify OpenCode is still gated**

1. Press `2` to filter to OpenCode only.
2. The Insights panel should NOT render for OpenCode sessions.
3. Pressing Enter should resume the session in tmux as before, not open the modal.

Expected: OpenCode behavior unchanged.

- [ ] **Step 5: If anything misbehaves, file follow-ups; otherwise the feature is shipped**

Nothing to commit. End of plan.

---

## Self-review notes

Spec coverage cross-check:

- Module layout (Section 1) → Tasks 1, 2, 7
- Block derivation rules (Section 2) → Task 2 implementation; Tasks 3–6 verify behavior
- Tool-result pairing (Section 2) → Task 2 (HashMap ID matching); Task 4 verifies under reordered completions
- `user_msg` attribution (Section 2) → Task 2 (`span_user_msg`); Task 3 verifies multi-round
- `usage` and `model` (Section 2) → Task 2; Task 9 verifies model from tool_complete
- Sub-agent filter (Section 2) → Task 2 (`parentToolCallId` skip); Task 6 verifies index isn't claimed
- Insights field map (Section 3) → Task 7; Tasks 8–10 verify each field
- Glyph picker priority (Section 3) → Task 7; Task 10 verifies
- `report_intent` suppression (Section 3) → Task 7; Task 8 verifies
- Tool-name → glyph map (Section 3) → Task 7 (`tool_to_glyph`); Task 10 verifies
- TUI dispatch (Section 4) → Tasks 11, 12
- Cache compatibility (Section 4) → Task 12 (`source_label` "copilot")
- Edge cases (large result, aborted, out-of-range) → Task 6
- OpenCode placeholder retained → Tasks 11, 12 (explicit OpenCode gates)

No placeholders. All function names referenced in later tasks (`SUPPRESSED_TOOL`, `extract_turn_from_str`, `parse_insights_from_str`, `tool_to_glyph`, `pick_turn_glyph`) are defined in their original tasks. Visibility changes in Task 1 enable the Task 7 imports of `increment_tool` and `bump_error`.
