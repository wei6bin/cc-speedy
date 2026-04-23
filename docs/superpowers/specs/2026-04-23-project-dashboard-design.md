# Project Dashboard — Design Spec

**Date:** 2026-04-23
**Status:** Approved
**Part of:** [Productivity Roadmap](./2026-04-23-productivity-roadmap-design.md) — feature #8

---

## Overview

The main list shows sessions. But users think in *projects*: "let me see what I've been doing on cc-speedy lately". Today a user has to `/`-filter by project name and mentally aggregate. Add a `P` key that opens a full-screen Project Dashboard — one row per unique `project_path` with per-project stats and one-keystroke drill-in.

---

## 1. Mode & Keybinding

- `P` (Shift+p) at top level enters `AppMode::Projects`. `p` (lowercase) stays free.
- `Esc` exits to `AppMode::Normal`.
- Inside Projects mode:
  - `j` / `k` / `Up` / `Down` — navigate.
  - `/` — enter `AppMode::ProjectsFilter` (substring filter over project name).
  - `s` — cycle sort: last-active → session-count → alphabetical → last-active.
  - `Enter` — exit dashboard and narrow the main list to that project (sets `project_filter`).
  - `Esc` — exit.

## 2. Row Layout

```
● feat/auth (dirty)     cc-speedy          17  last: 2026-04-22   *2
○ master                 ec-hip-tools       42  last: 2026-04-19
· (no git)               old-scripts         3  last: 2026-03-10
```

Columns:
- Git status glyph (reused from feature #5) + live branch.
- Project name — last 2 path segments.
- Session count.
- Last active timestamp (formatted).
- Pinned-session count (prefixed with `*`), omitted when zero.

## 3. Aggregation

Compute once on mode entry:

```rust
pub struct ProjectRow {
    pub project_path: String,      // full absolute path — identity
    pub name: String,              // display — last 2 segments
    pub session_count: usize,
    pub pinned_count: usize,
    pub last_active: SystemTime,   // max of session.modified across the group
}
```

Source data: `app.sessions` (already loaded). Group by `project_path` (string equality). Archived sessions are *included* in counts (they're still work on that project) but archived-only projects still show.

Sort modes cycle via `s`:
- **LastActive** (default) — `last_active` desc.
- **SessionCount** — `session_count` desc, tiebreak last_active desc.
- **Alphabetical** — `name` asc, case-insensitive.

## 4. Text Filter

`/` enters `AppMode::ProjectsFilter`. Query matches substring against the display `name` (not the full path). Live narrowing. `Esc` clears and returns to Projects mode.

## 5. Enter → Drill Into Main List

On Enter, set `app.project_filter = Some(project_path)` and return to Normal mode. `apply_filter()` is extended to respect this field alongside `source_filter` and text filter.

A new top-level indicator shows the active project filter in the top bar hint: `  project: cc-speedy  (Esc to clear)`.

`Esc` in Normal mode with an active project filter clears the filter (does not quit). `q` still quits.

## 6. Files Changed

- `src/tui.rs`
  - `AppMode::Projects`, `AppMode::ProjectsFilter`.
  - `AppState` fields: `projects: Vec<ProjectRow>`, `projects_filtered: Vec<usize>`, `projects_filter: String`, `projects_sort: ProjectSort`, `projects_list_state: ListState`, `project_filter: Option<String>`.
  - `P` handler: builds `projects` via `build_project_rows()`, applies filter/sort, enters mode.
  - `s` sort-cycle handler.
  - `/` enters ProjectsFilter sub-mode.
  - `Enter` sets `project_filter`, calls `apply_filter()`, returns to Normal.
  - `apply_filter()` honors `project_filter`.
  - Top-bar Normal-mode hint shows active project filter when set; `Esc` in Normal clears it.
  - `draw_projects(f, app, area)` full-width renderer.
- `tests/project_dashboard_test.rs` — unit tests for aggregation and sort.

## 7. Testing

**Unit:**
- `build_project_rows()` groups correctly, counts pinned, computes max last_active.
- Sort: LastActive / SessionCount / Alphabetical return expected orders.

**Manual:**
- `P` opens dashboard; row count matches unique project count.
- `s` cycles sort.
- `/` narrows by name.
- Enter narrows main list to that project; top bar shows `project: X (Esc to clear)`.
- `Esc` in Normal clears project filter.

## 8. Non-Goals

- No tag chips in rows (feature #4 not yet shipped — add later when tags land).
- No project-level aggregation of summaries or learning points (those live in the Library/grep views).
- No rename/hide of projects.
- No persistence — mode state is ephemeral.
