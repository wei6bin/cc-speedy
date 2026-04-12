# Session KB & Obsidian Export — Design Spec

**Date:** 2026-04-12
**Status:** Approved

## Overview

Extend cc-speedy so that Ctrl+R (summary generation) also extracts structured knowledge from the session — decision points, lessons, and tool discoveries — and automatically exports a Markdown note to an Obsidian vault. Learning points accumulate across re-generations; factual summary sections are overwritten.

---

## 1. Enriched Prompt & Sections

`generate_summary()` in `summary.rs` is extended to produce all sections in a **single `claude --print` call**. The prompt instructs Claude to output two groups separated by `<!-- LEARNINGS -->`:

### Factual group (overwritten on re-generate)
```
## What was done
- bullet

## Files changed
- file (or "none")

## Status
Completed / In progress

## Problem context
What problem was being solved and why

## Approach taken
How it was solved — key decisions and steps
```

### Learning group (accumulated, never overwritten)
```
## Decision points
- Technical design choice + brief rationale

## Lessons & gotchas
- Surprise, pitfall, or thing to do differently next time

## Tools & commands discovered
- CLI flag / library / API found (or "none")
```

The delimiter `<!-- LEARNINGS -->` lets the code split the output into two strings:
- **Factual part** → stored in the existing `summaries` table, overwritten each re-gen
- **Learning part** → parsed into bullets, appended to the new `learnings` table

On re-generation, existing learning bullets for the session are passed into the prompt as context so Claude only returns **new** points not already captured.

---

## 2. Database Schema

### Existing: `summaries` table — unchanged
Stores the factual part only (overwritten on re-gen).

### New: `learnings` table

```sql
CREATE TABLE IF NOT EXISTS learnings (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id  TEXT    NOT NULL,
    category    TEXT    NOT NULL,  -- "decision_points" | "lessons_gotchas" | "tools_commands"
    point       TEXT    NOT NULL,
    captured_at INTEGER NOT NULL   -- unix seconds
);
CREATE INDEX IF NOT EXISTS learnings_session ON learnings (session_id);
```

Each bullet point is one row. Re-generation appends new rows; existing rows are never deleted or modified.

### New: `settings` table

```sql
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

Initial keys:
- `obsidian_kb_path` — absolute path to the Obsidian folder

---

## 3. Code Modules

### `summary.rs` — changes
- `generate_summary(messages, existing_learnings) -> Result<(String, Vec<LearningPoint>)>`
  - Takes `existing_learnings: &[LearningPoint]` so Claude can avoid duplicates
  - Returns `(factual_markdown, new_learning_points)`
- `LearningPoint { category: String, point: String }` — new struct

### `store.rs` — changes
- `migrate()` creates the `learnings` and `settings` tables if absent
- `save_learnings(conn, session_id, points: &[LearningPoint]) -> Result<()>` — appends rows
- `load_learnings(conn, session_id) -> Result<Vec<LearningPoint>>` — fetches all rows for session
- `get_setting(conn, key) -> Option<String>`
- `set_setting(conn, key, value) -> Result<()>`

### `obsidian.rs` — new module
- `export_to_obsidian(session: &UnifiedSession, factual: &str, learnings: &[LearningPoint], vault_path: &str) -> Result<()>`
- Writes `YYYY-MM-DD-<project_name>-<session_id[:8]>.md` (overwrites if exists)
- Skips if `session.message_count < 5`
- File format:
  ```markdown
  ---
  date: 2026-04-12
  project: ai/cc-speedy
  session_id: abc12345
  tags: [agent-session]
  ---

  ## What was done
  ...

  ## Problem context
  ...

  ## Approach taken
  ...

  ## Decision points
  ...

  ## Lessons & gotchas
  ...

  ## Tools & commands discovered
  ...
  ```
- `project_name` is derived from `session.project_path` using the existing `path_last_n` util (last 2 segments, `/` replaced with `-`)

### `settings.rs` — new module
- `AppSettings { obsidian_kb_path: Option<String> }` struct
- `load(conn) -> AppSettings` — reads from DB
- `save_obsidian_path(conn, path: &str) -> Result<()>` — validates + writes

---

## 4. TUI Changes

### Ctrl+R flow (updated)
```
1. Clear cached factual summary for session
2. Load existing LearningPoints from DB for session
3. spawn_summary_generation(... existing_learnings ...)
   a. Parse messages
   b. Call generate_summary(messages, existing_learnings)
      → returns (factual_str, new_learning_points)
   c. Overwrite summaries table with factual_str
   d. Append new_learning_points to learnings table
   e. Load ALL learnings for session from DB
   f. Render full preview = factual_str + all learnings formatted
   g. If obsidian_kb_path is set → call obsidian::export_to_obsidian()
   h. Show status: "Summary + KB saved" or "Summary saved (no Obsidian path set)"
```

### TUI preview panel
Renders factual markdown first, then a divider, then all accumulated learning rows grouped by category. Learning section has a distinct header style to visually separate it.

The in-memory `app.summaries` cache stores the **combined display string** (factual + all learnings rendered), not just the factual part. This keeps the rendering path unchanged — the preview widget just reads from the cache as before.

### `s` key — Settings panel

New `AppMode::Settings` with a `SettingsField` enum (one variant now: `ObsidianPath`).

**Layout:** Modal panel centred over the TUI, 60% width:
```
┌─ Settings ──────────────────────────────────────────┐
│                                                     │
│  Obsidian KB path                                   │
│  ▶ /mnt/c/DIM/obsidian-vault/nas/work/agent-...    │
│                                                     │
│  [Enter] Edit   [Esc] Close                         │
└─────────────────────────────────────────────────────┘
```

When editing a field, the row enters inline edit mode (text cursor, same pattern as `AppMode::Rename`). On Enter:
- Validate: `std::fs::metadata(path).map(|m| m.is_dir())` — show red inline error if invalid
- On success: save to DB via `store::set_setting()`, update `AppState.settings`

---

## 5. Key Bindings Update

| Key | Action |
|-----|--------|
| `s` | Open Settings panel (Normal mode) |
| Ctrl+R | Regenerate summary + append new learning points + export to Obsidian |

Help bar updated to include `s: settings`.

---

## 6. Error Handling

- Obsidian export failure is non-fatal: logged as a TUI status message, does not block summary display
- If `claude --print` returns output without the `<!-- LEARNINGS -->` delimiter, treat entire output as factual (graceful degradation)
- Path validation in settings shows inline error without closing the panel

---

## 7. Out of Scope

- Searching or browsing learning points across sessions (future feature)
- Tagging or categorising notes beyond the three fixed categories
- Syncing deletions back from Obsidian to the DB
