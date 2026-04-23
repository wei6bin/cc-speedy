# Tags — Design Spec

**Date:** 2026-04-23
**Status:** Approved
**Part of:** [Productivity Roadmap](./2026-04-23-productivity-roadmap-design.md) — feature #4

---

## Overview

Pinned is a binary categorization. Users want richer grouping: `wip`, `blocked`, `needs-review`, `side-project`. Add arbitrary free-form tags per session, with inline edit, preview-pane display, and filter-bar support for `#tag` tokens.

MVP scope: edit + storage + preview display + `#tag` filter. Deferred to a follow-up: in-row chip rendering and a dedicated tag browser view (`T` key).

---

## 1. Data Model

```sql
CREATE TABLE IF NOT EXISTS tags (
    session_id TEXT NOT NULL,
    tag        TEXT NOT NULL,
    PRIMARY KEY (session_id, tag)
);
CREATE INDEX IF NOT EXISTS idx_tags_tag ON tags (tag);
```

Migration is additive and idempotent; runs in `open_db()` alongside existing table creations.

Tags are normalized on write:
- Trimmed of whitespace.
- Lowercased.
- Characters restricted to `[a-z0-9-_]`; any other character is skipped during parsing so users can type `foo, bar-baz` freely.
- Empty strings after normalization are dropped.

## 2. Store API (src/store.rs)

```rust
pub fn load_tags(conn: &Connection, session_id: &str) -> Result<Vec<String>>;
pub fn set_tags(conn: &Connection, session_id: &str, tags: &[String]) -> Result<()>;
pub fn load_all_tags(conn: &Connection) -> Result<HashMap<String, Vec<String>>>;
```

`set_tags` replaces the full set (DELETE + INSERT in one transaction) — matches the comma-separated editing UX where the user re-types the whole list.

## 3. Edit Flow (`t` key)

- Normal + `t` on a selected session → opens `AppMode::TagEdit` popup.
- The popup input is pre-filled with the session's current tags joined by `, `.
- Typing / backspace edits; `Enter` commits (parses, normalizes, DELETE + INSERT), closes the popup, and refreshes the preview.
- `Esc` cancels without saving.

Popup layout (centered, ~60 × 5):
```
 Tags (comma-separated)
 ─────────────────────────────────────────
 wip, blocked, needs-review_
 [Enter] save   [Esc] cancel
```

## 4. Preview Display

Preview pane adds a line below `BRANCH:`:
```
TAGS:     wip, blocked, needs-review
```
Omitted when the session has no tags.

Tag set is read from an in-memory cache populated on startup (`load_all_tags`) and updated on every `set_tags` write.

## 5. Filter Bar `#tag` Support

The existing `/` filter query accepts tokens. Each whitespace-separated token:
- `#tag` → session must be tagged `tag` (exact match, case-insensitive, normalization applied).
- anything else → substring matched against title (current behavior).

A session passes the filter iff it satisfies **every** token.

Examples:
- `/#blocked` → sessions tagged `blocked`.
- `/#blocked auth` → tagged `blocked` AND title contains "auth".
- `/foo #wip` → title contains "foo" AND tagged `wip`.

## 6. Files Changed

- `src/store.rs` — migration, three new fns.
- `src/tui.rs`
  - `AppMode::TagEdit` variant.
  - `AppState.tags_by_session: HashMap<String, Vec<String>>` — loaded at startup, mutated on save.
  - `AppState.tag_edit_input: String`.
  - `t` key handler (Normal) — open editor.
  - `TagEdit` handlers (Enter/Esc/Backspace/Char).
  - `apply_filter()` extended — token-based filter parser.
  - Preview pane branch block adds `TAGS:` line.
  - New `draw_tag_edit_popup()`.
- `tests/tags_test.rs` — parsing + normalization; filter token splitter.

## 7. Testing

**Unit:**
- `normalize_tag("  WIP  ")` → `"wip"`.
- `normalize_tag("needs review")` → `"needsreview"` (space dropped because space is not in the allowed set).
- `parse_tags("wip, blocked, , WIP")` → `["wip", "blocked"]` (dedup, empties skipped).
- `filter_matches(session, tokens)` for various token combinations.

## 8. Non-Goals (deferred)

- **In-row tag chips.** Adds lots of rendering surface; wait for signal that users want it.
- **Tag browser view (`T`).** A future follow-up that lists all tags + counts, like the Learning Library structure.
- **Tag rename / merge.** Users can re-edit per session; bulk rename deferred.
- **Tag autocomplete.** Typing the tag is fast; autocomplete later if needed.
