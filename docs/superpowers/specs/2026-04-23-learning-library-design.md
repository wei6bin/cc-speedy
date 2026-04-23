# Learning Library — Design Spec

**Date:** 2026-04-23
**Status:** Approved
**Part of:** [Productivity Roadmap](./2026-04-23-productivity-roadmap-design.md) — feature #1

---

## Overview

cc-speedy already collects structured learning points from every session: decision rationales, lessons & gotchas, and tool/command discoveries. Today that data is locked inside individual session rows — you can only read it in the preview pane of the session that produced it.

Add a `L` key that opens a full-screen Learning Library: a flat, cross-session list of every learning point, filterable by category and by text. Each row shows the point, its category, and the source session. Enter on a row jumps back to that source session in the main list.

This turns cc-speedy's accumulated learning data into a browsable personal knowledge base.

---

## 1. Mode & Keybinding

- `L` (Shift+l) at top level enters `AppMode::Library`. `l` (lowercase) stays free.
- `Esc` exits back to `AppMode::Normal` with the previously-focused session selected.
- Inside Library mode:
  - `j` / `k` / `Up` / `Down` — navigate entries.
  - `0` / `1` / `2` / `3` — category filter (all / decisions / lessons / tools).
  - `/` — enter `AppMode::LibraryFilter` (sub-mode for live text filter).
  - `Esc` (from LibraryFilter) — cancel and return to Library with filter cleared.
  - `Enter` — exit Library, scroll main list to the source session and select it.

## 2. Row Layout

```
[DEC] point text — session title                    · 2026-04-20
[LSN] another point …                               · 2026-04-19
[TOL] cli flag discovered …                         · 2026-04-18
```

Columns:
- Category tag (3-letter): `DEC` (decision_points), `LSN` (lessons_gotchas), `TOL` (tools_commands). Color-coded: DEC blue, LSN yellow, TOL green.
- Point text — truncated to fit available width minus trailing date.
- Source session short title (falls back to project name if summary empty).
- Right-aligned date of the source session (from `sessions.modified`).

## 3. Category Filter

`0` / `1` / `2` / `3` cycle the category filter:
- `0` — all categories
- `1` — decision_points
- `2` — lessons_gotchas
- `3` — tools_commands

Inside Library mode these keys are category filters (shadow the main-mode source filter). Users return to main mode via `Esc` to regain source filtering.

## 4. Text Filter

Within Library mode, `/` enters `AppMode::LibraryFilter`. Query matches case-insensitive substrings over the point text only (not over session title — keeps scope obvious). Live filter while typing, same UX as main-mode `/` filter.

Composition with category filter: if a category is active, text filter narrows within that category.

## 5. Ordering

By source session `modified` timestamp, most recent first. Ties within a session keep whatever order the rows were inserted into the `learnings` table (insertion order from `save_learnings`).

## 6. Enter / Jump to Source

On Enter:
- `app.mode = AppMode::Normal`.
- Find the selected learning entry's `session_id`, look it up in `app.filtered_active` (or `filtered_archived`), select that index in the appropriate list.
- `preview_scroll = 0`.
- If the session isn't in the currently filtered list (e.g. archived or filtered out by source filter), surface a status message: "Session not in current view — press 0 to unfilter" and do not change selection.

## 7. Data Loading

**New store helper** `load_all_learnings_with_context(&conn)`:

```rust
pub struct LearningEntry {
    pub session_id: String,
    pub category:   String,
    pub point:      String,
    pub session_title: String,
    pub session_modified: i64,
}

pub fn load_all_learnings_with_context(conn: &Connection) -> Result<Vec<LearningEntry>>;
```

Single SQL:
```sql
SELECT l.session_id, l.category, l.point, s.content, COALESCE(s.generated_at, 0)
FROM learnings l
LEFT JOIN summaries s ON l.session_id = s.session_id
ORDER BY s.generated_at DESC, l.rowid ASC
```

The title used for display is the session's `sessions_index` title (from `UnifiedSession.summary`), not the full summary content. Resolved at render time via a HashMap keyed by session_id (populated from `app.sessions`), rather than baking it into the SQL result.

Library entries load once on Library-mode entry (not at TUI startup), so re-entering picks up newly generated learnings without restart.

## 8. Files Changed

- `src/store.rs` — add `LearningEntry` struct + `load_all_learnings_with_context()` (~30 LOC).
- `src/tui.rs`
  - New `AppMode::Library` and `AppMode::LibraryFilter` variants.
  - `AppState` fields: `library_entries: Vec<LibraryRow>`, `library_filter: String`, `library_category: Option<String>`, `library_list_state: ListState`, `library_filtered: Vec<usize>`.
  - `L` key handler: load entries, reset filter state, switch to Library mode.
  - Esc/j/k/0/1/2/3//Enter handlers for Library mode.
  - Dedicated `draw_library(f, app, area)` function; `draw()` dispatches to it when mode is Library.
  - Library filter mode handlers (char/backspace/esc).
  - `apply_library_filter()` helper that rebuilds `library_filtered` from category + text filter.
- `tests/learning_library_test.rs` — new, ~50 LOC: filter / category logic.

## 9. Testing

**Unit:**
- `filter_library_entries(entries, category=Some("decision_points"), "auth")` returns expected subset.
- `filter_library_entries` with empty filters returns all entries.
- Category filter respects string equality exactly.

**Manual TUI:**
- `L` opens library; all categories visible.
- `1` narrows to decisions; `2` to lessons; `3` to tools; `0` restores all.
- `/auth` narrows live; `Esc` inside filter returns to library list.
- Enter jumps back to source session, main list repositioned and selected.
- Archived source shows appropriate status message on Enter.

## 10. Non-Goals

- No editing of learning points from the library.
- No deletion.
- No export to Obsidian from the library (that's already per-session).
- No ranking — pure chronological by source session.
- No pre-loading at TUI startup (entries may be hundreds; delayed load on `L` is cheap).
