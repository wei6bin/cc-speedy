# Cross-Session Grep — Design Spec

**Date:** 2026-04-23
**Status:** Approved
**Part of:** [Productivity Roadmap](./2026-04-23-productivity-roadmap-design.md) — feature #2

---

## Overview

The existing `/` filter matches only the session title (rendered `summary` field). Users frequently want to recall "where did I solve that auth bug?" — but the answer lives in the summary body or learning points, which the filter can't reach today.

Add a `?` key that opens a parallel grep mode. Substring-matches across the session title, project path, git branch, summary body, and accumulated learning points. Narrows the main list in place and highlights hits in the preview pane.

`/` stays unchanged. Users keep the cheap title-only filter and opt into deep search explicitly.

---

## 1. Mode & Keybinding

| Key | Mode | Effect |
|-----|------|--------|
| `/` | `AppMode::Filter` (existing) | Title-only substring |
| `?` | `AppMode::Grep` (new) | Deep substring across all text fields |
| `Esc` | — | Exits the current mode, restores unfiltered view |
| `Backspace`, printable chars | within mode | Edits the query; filter re-applies live |
| `Enter` | within Grep mode | No-op (live filter, nothing to submit) |

The two modes are mutually exclusive — entering grep while in filter mode discards the filter query, and vice versa. This avoids a three-axis filter state that would be hard to reason about.

## 2. Query Semantics

- Case-insensitive substring match. No regex, no tokenization, no escaping.
- Empty query shows all sessions (grep mode with empty query is equivalent to Normal mode for list contents, but the status bar still says "grep:").
- Whitespace is preserved — `foo bar` matches the literal substring, not two independent tokens.

## 3. Search Scope

Per session, the haystack is built by concatenating:

1. `s.summary` — session title (what's shown in the list).
2. `s.project_path` — full absolute path.
3. `s.git_branch` — empty string if none.
4. `app.summaries[session_id]` — cached combined summary string (factual + learnings), loaded at startup.

The concatenation uses newlines as separators (they don't affect substring matching).

**Not searched:**
- Session ID, timestamps, jsonl paths, summary generation timestamps — not user-meaningful text.
- Raw session transcripts (the `.jsonl` bodies) — would require file I/O per keystroke; summaries are the curated proxy.

## 4. Haystack Caching

When grep mode is entered, `AppState` builds `grep_haystacks: Vec<String>` — one lowercased haystack per session in the same index order as `sessions`. This cache is reused for every keystroke while grep mode is active; rebuilt only on mode re-entry.

For N sessions, one keystroke is O(N × avg_haystack_len) — with the haystack pre-lowercased, this is a plain substring search. At N = 10,000 sessions × 2 KB average haystack = 20 MB scanned per keystroke — well under a millisecond on modern hardware.

No SQLite FTS5 virtual table is needed at v1. If latency ever becomes visible, revisit.

## 5. Composition with Existing Filters

Grep applies *after* the source filter (`1`/`2`/`3`/`0`) and the active-vs-archived tab selection. So the pipeline for a displayed session is:

```
all sessions
  → active OR archived (tab)
  → source filter (CC / OC / Copilot / all)
  → grep query (if Grep mode active)
  → recency sort (pinned float to top)
```

Source filter keys (`1`/`2`/`3`/`0`) are treated as printable characters *inside* grep mode by default — they'd be typed into the query. To change source while in grep, the user presses `Esc` first (exits grep), then `1`/`2`/`3`/`0`, then `?` to re-enter grep.

**Rejected alternative:** routing `1`/`2`/`3`/`0` as source-filter shortcuts even inside grep mode. Rejected because grep queries legitimately contain digits (`auth2`, `v3`, `404`) and shadowing them would be surprising.

## 6. Ordering

Recency — the default sort. Pinned sessions still float to the top. No re-ranking by match count or match position.

Rationale: when searching "auth bug", users typically want the recent one. Ranking by match frequency would push a session that mentions "auth" 20 times above the actual work session that mentions it twice.

## 7. Preview Pane Highlighting

When grep mode is active and the query is non-empty:

- The preview pane splits each line into `ratatui::text::Span` chunks. Occurrences of the query (case-insensitive) are wrapped in a `theme::grep_match_style()` (new — e.g. background accent color, matching the existing `pin_popup_style` pattern).
- On selected-session change while in grep mode, `preview_scroll` is set so the first match line is at the top of the visible preview area. If there are no matches in the preview (shouldn't happen — if the session matched, its haystack contained the query somewhere), scroll resets to 0.
- On exiting grep mode, preview returns to plain text rendering.

## 8. Status / Footer Bar

While grep mode is active:
- Top bar shows `grep: <query>  (N matches)` where N is the filtered list length.
- Footer help switches to a minimal grep-mode legend: `? grep · Esc: exit`.

While grep mode is inactive, status/footer render as today.

## 9. Data Model

No schema change. No new persisted state. Grep query and haystack cache are in-memory, lost on TUI exit.

## 10. Files Changed

- `src/tui.rs`
  - `AppMode` variant `Grep`.
  - `AppState` fields: `grep_query: String`, `grep_haystacks: Vec<String>`.
  - New match arms for `AppMode::Grep` in the event loop (Esc, Backspace, char insertion).
  - New match arm for `(AppMode::Normal, _, KeyCode::Char('?'))` to enter grep mode.
  - Extend `apply_filter()` to honor grep query when mode is `Grep`.
  - Build `grep_haystacks` on mode entry.
  - Update top-bar and footer rendering.
- `src/theme.rs`
  - Add `grep_match_style()` returning a `Style` with a dim accent background (match btop palette).
- `src/tui.rs::build_preview_content` (or a new helper)
  - Refactor to return `Vec<Line>` with highlighted spans when grep mode is active; plain text otherwise.
  - The refactor is bounded — only the preview rendering path changes; list row rendering is untouched.
- `tests/tui_grep_test.rs` (new)
  - Unit test for `filter_sessions_by_grep()` returning expected subset and order.
  - Unit test for the span-splitting highlight helper.

## 11. Testing

**Unit:**
- `filter_sessions_by_grep` with haystacks containing: match in title only / match in branch only / match in summary / match in learnings / no match / multiple matches in one session. Expect: sessions returned in their original index order (preserving recency sort), no duplicates.
- Highlight helper: input `"authenticate the auth flow"`, query `"auth"`, expect 2 matched spans and 3 literal spans in the correct order.

**Manual TUI:**
- `?` enters grep; live match count updates on each keystroke.
- `/` and `?` are mutually exclusive (entering one clears the other).
- Source filter composition: `2` then `?` shows only OC sessions matching the query.
- Match highlight visible in preview; first match scrolled into view on selection change.
- Pinned sessions with matches float to top; pinned without matches are hidden.
- `Esc` clears grep and restores full unfiltered list.

## 12. Risks / Open Questions

- **Multi-line preview highlighting edge cases.** If the query spans a line break (e.g. `foo\nbar`), the current design won't highlight it since matching happens per-line in the preview renderer. Rare in practice; not solving in v1.
- **Grep haystack memory.** ~2 KB per session × N sessions. At 10,000 sessions = 20 MB peak, only while in grep mode. Acceptable.
- **Query containing filter-mode escape chars.** No regex, so no escaping needed. `?`, `/`, `#` are all literal when inside grep mode.

## 13. Non-Goals

- No regex / glob / fuzzy matching. Substring only.
- No result ranking by match quality or count.
- No search of raw `.jsonl` transcripts — summaries are the indexed proxy.
- No persistent grep history (no "recently searched" list).
- No dedicated results panel showing one row per match — stays as an inline filter over the main list.
