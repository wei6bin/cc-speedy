# Copilot turn detail + insights timeline

Status: design approved 2026-04-26 — pending implementation plan.

## Goal

Bring the per-turn detail modal that exists for Claude Code sessions to GitHub Copilot CLI sessions. Today the modal is gated to `SessionSource::ClaudeCode`; for Copilot users the timeline never renders, so the existing entry point (`Enter` on the Insights glyph cursor) doesn't fire.

Scope (chosen): the timeline glyphs and the modal — both produced from Copilot's `events.jsonl`. The aggregate stats line above the timeline renders with whatever data we have (output tokens only); cells that need input/cache breakdowns stay zero. Histogram, tasks, and error count are computed normally.

OpenCode stays placeholder.

## Format reference

Copilot session lives at `~/.copilot/session-state/<id>/events.jsonl`. Relevant events:

| Event | Carries |
| --- | --- |
| `user.message` | `data.content` (raw user text), `data.transformedContent` (with system preamble — ignored), `data.attachments` |
| `assistant.turn_start` | `data.turnId`, `data.interactionId` — opens a logical turn |
| `assistant.message` | `data.content`, `data.toolRequests[]`, `data.reasoningText`, `data.reasoningOpaque`, `data.outputTokens`, optional `data.parentToolCallId` |
| `tool.execution_start` | `data.toolCallId`, `data.toolName`, `data.arguments` |
| `tool.execution_complete` | `data.toolCallId`, `data.success`, `data.result.{content, detailedContent}`, `data.model` |
| `assistant.turn_end` | `data.turnId` |
| `subagent.*`, `session.compaction_*`, `session.resume`, `session.model_change`, `abort` | metadata; not extracted in v1 |

Key differences from CC:

- Multiple `assistant.message` events can occur inside one `turn_start..turn_end` span (agentic loops).
- Reasoning is plaintext in `reasoningText` (CC's `thinking` is usually opaque).
- Token usage is asymmetric: only `outputTokens` per message — no input/cache breakdown.
- Tool calls execute in parallel; completions can arrive out of order.
- Sub-agents emit nested `assistant.message` events tagged with `parentToolCallId`.

## Architecture

Two new modules. Existing types stay where they are; the new parsers return the same `TurnDetail` and `SessionInsights` so the renderer is unchanged.

```
src/
  turn_detail.rs              (existing — CC parser, shared types)
  copilot_turn_detail.rs      (NEW — Copilot parser, returns TurnDetail)
  insights.rs                 (existing — CC parser, shared types)
  copilot_insights.rs         (NEW — Copilot parser, returns SessionInsights)
  tui.rs                      (4 dispatch points)
```

No trait, no enum dispatch inside parsing loops. `tui.rs` matches on `SessionSource` and calls the right parser.

## Turn-detail extraction (`copilot_turn_detail::extract_turn`)

Signature: `pub fn extract_turn(path: &Path, turn_idx: u32) -> Result<TurnDetail>`.

**Glyph grain.** One glyph = one `assistant.message` event whose `parentToolCallId` is **not** set. The Nth such event maps to `turn_idx == N`.

**Block derivation, per assistant message:**

1. **Thinking block** — from `data.reasoningText`. Non-empty → `Thinking { text, redacted: false }`. Empty + non-empty `reasoningOpaque` → `Thinking { text: "", redacted: true }`. Both empty → skip.
2. **Text block** — from `data.content`. Skip if empty.
3. **Tool blocks** — one per `data.toolRequests[]` entry, in array order:
   - `name = toolReq.name`
   - `input_pretty = serde_json::to_string_pretty(&toolReq.arguments)`
   - `result = None`

While capturing the message, build `HashMap<toolCallId, block_idx>`.

**Tool-result pairing.** Scan forward until the next `assistant.message`, `assistant.turn_end`, or `user.message`. For each `tool.execution_complete`:

- Look up `data.toolCallId` in the map. If absent, skip.
- `is_error = !data.success`
- `content = data.result.detailedContent` if non-empty, else `data.result.content`, else `""`
- Truncate at `RESULT_BYTE_CAP` (8 KB, same as CC), record `original_bytes` and `truncated`.

Exact-ID matching is required because Copilot tools run in parallel and completions can arrive out of order — CC's first-unfilled-wins would mis-pair.

**`user_msg` attribution.** Carry the most recent `user.message.data.content` across the entire `turn_start..turn_end` span. Every `assistant.message` inside that span (including agentic follow-up rounds) reports the same originating prompt. A new `user.message` resets it. Use the raw `content`, not `transformedContent`.

**`usage`.** `TurnUsage { input_tokens: 0, output_tokens: outputTokens, cache_creation: 0, cache_read: 0 }`. The renderer's `cache_hit_pct()` returns 0 when denom is 0, so it degrades cleanly.

**`model`.** Read from the next `tool.execution_complete.data.model` in the same scan window. Fall back to the most recent `session.model_change.data.model`. Default `""`.

**Out-of-range index** → `Err`, same shape as CC.

## Insights parser (`copilot_insights::parse_insights`)

Returns `SessionInsights`. Field map:

| Field | Source |
| --- | --- |
| `model` | last `tool.execution_complete.data.model` (or `session.model_change.data.model`) |
| `assistant_turns` | count of qualifying `assistant.message` events (no `parentToolCallId`, and at least one block-producing field is non-empty — see "Empty message" edge case) |
| `output_tokens` | sum of `data.outputTokens` over the same set |
| `input_tokens`, `cache_creation`, `cache_read` | `0` |
| `tool_counts` | `(toolName, count, errors)` for each non-suppressed tool. `errors += 1` whenever a paired `tool.execution_complete` has `success == false`. |
| `tool_errors` | count of `tool.execution_complete` with `success == false` |
| `tasks` | `arguments.agent_type` (or `arguments.name`) of every `task` tool call, first-occurrence order, deduplicated |
| `skills` | `vec![]` (no Copilot equivalent) |
| `sidechain_lines` | count of `assistant.message` events with `parentToolCallId` |
| `turns` | one `TurnGlyph` per qualifying `assistant.message` |

**Glyph picker — same priority as CC** (Task > Skill > Tool > Thinking > Text):

- `task` toolName → `Task` category, glyph `'A'`, label `Agent → <agent_type>`.
- Skill never matches (Copilot has none).
- Other tool calls → `Tool` category. Dominant tool wins glyph; label like CC's (`bash×3 + view`).
- No tools, `reasoningText` non-empty → `Thinking`, glyph `'t'`.
- Otherwise → `Text`, glyph `'·'`.
- `has_error` set if any tool in the message returned `success == false`.

**Tool-name → glyph map** (Copilot-specific, lives in `copilot_insights.rs`):

| Name | Glyph |
| --- | --- |
| `bash` | `B` |
| `view` | `R` |
| `edit`, `str_replace` | `E` |
| `create`, `write` | `W` |
| `task` | `A` |
| `web_fetch`, `fetch_*` | `F` |
| `report_intent` | (suppressed — see below) |
| anything else | `+` |

**`report_intent` suppression.** The `report_intent` tool fires on nearly every Copilot turn as protocol noise. It is excluded from `tool_counts`, from the dominant-glyph calculation, and from `has_error`. It still appears as a `Tool` block inside the modal if present, so users can see it; it just doesn't pollute the aggregate view.

## TUI integration

Four dispatch points in `src/tui.rs`:

| Location | Change |
| --- | --- |
| `open_turn_detail` (~line 639) | `match s.source` — call `copilot_turn_detail::extract_turn` for Copilot. OpenCode → status message. |
| `maybe_spawn_insights_load` (~line 729) | Skip OpenCode only. Call `copilot_insights::parse_insights` for Copilot. Persist with source `"copilot"`. |
| `show_insights` predicate (~line 2406) | Allow CC and Copilot, exclude OpenCode. |
| Enter "want_detail" gate (~line 1504) | Same — allow Copilot. |

**Cache.** The `insights` SQLite table has a `source` column already. Adding `"copilot"` rows is additive — no migration. Session IDs are UUIDs from `workspace.yaml`; no collision with CC IDs in practice.

## Edge cases

| Case | Behavior |
| --- | --- |
| Empty `assistant.message` (no content, no toolRequests, no reasoning) | Skipped from timeline — doesn't claim a glyph slot. |
| Aborted run (`abort` event mid-turn) | Tool blocks without matching `_complete` keep `result: None`. Renderer already handles None. |
| Out-of-order tool completions | Handled by exact-ID HashMap. |
| Compaction / resume | Transparent — we keep counting. |
| Sub-agent rounds (`parentToolCallId`) | Excluded from main timeline; counted in `sidechain_lines`. |
| Truncated final line | Skipped silently (same as CC parser). |
| `transformedContent` on user messages | Ignored; raw `content` only. |
| Missing/non-readable file | Returns `Err` from `extract_turn`; status line shows the error. |

## Out of scope (v1)

- Rendering sub-agent rounds as their own modal/timeline entries.
- Surfacing compaction/resume markers in the timeline.
- A separate "round X of turn Y" label decoration.
- OpenCode parity for either feature.
- Backfilling input/cache token estimates from compaction events.

## Testing

Fixtures under `tests/fixtures/copilot/`:

- `simple_turn.jsonl` — one round, one tool, success.
- `multi_round_turn.jsonl` — three rounds inside one turn span.
- `parallel_tools.jsonl` — three tools, completions arrive C/B/A.
- `failed_tool.jsonl` — `success: false`, empty result.
- `redacted_thinking.jsonl` — empty `reasoningText`, non-empty `reasoningOpaque`.
- `subagent_filter.jsonl` — one main message + two `parentToolCallId` messages.
- `report_intent_suppressed.jsonl` — `report_intent` plus a real tool.
- `aborted.jsonl` — `tool.execution_start` without `_complete`, then `abort`.
- `large_result.jsonl` — `detailedContent` larger than `RESULT_BYTE_CAP`.

Test files:

`tests/copilot_turn_detail_test.rs`:

- `turn_0_basic_extraction`
- `turn_1_carries_originating_user_msg`
- `parallel_tool_completions_pair_by_id`
- `failed_tool_marked_as_error`
- `redacted_thinking_block`
- `subagent_messages_excluded_from_index`
- `large_result_truncated_at_char_boundary`
- `out_of_range_returns_err`

`tests/copilot_insights_test.rs`:

- `assistant_turns_excludes_subagents`
- `output_tokens_summed_input_zero`
- `tool_histogram_excludes_report_intent`
- `tool_errors_count_matches_failed_completions`
- `tasks_populated_from_task_tool_agent_type`
- `skills_empty_for_copilot`
- `sidechain_lines_counts_subagent_messages`
- `glyph_category_priority_task_over_tool`
- `model_picked_up_from_tool_complete`

No new TUI integration test — dispatch changes are small and covered by manual smoke testing on a real Copilot session.
