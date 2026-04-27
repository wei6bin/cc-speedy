# TUI two-tier project / session navigation

Status: design approved 2026-04-26 — pending implementation plan.

## Goal

Promote the existing `AppMode::Projects` ("P:") view from a one-shot modal picker to the **default landing screen** of the TUI, and demote the flat session list (`AppMode::Normal`) to a per-project drill-in view. Boot lands on Projects; `→` / `Enter` enters a project; `←` exits back to Projects.

The "show every session across every project as one flat list" surface is removed. Cross-project search continues to live in the Library (`L`) and Digest (`D`) views.

## Motivation

The session list grows linearly with the user's history. As soon as a user has worked across more than ~3 projects, the flat list becomes a scroll-fest where two adjacent rows can be from unrelated codebases. Project context is the most important grouping for a user trying to recall "what was I last doing on X". The existing Projects modal already encodes that grouping but is a transient picker — most users never discover it. Making it the front door means:

- The expensive grouping (`build_project_rows`) is computed once on startup and reused.
- The source filter (`1/2/3/0`) becomes meaningful at the project tier — "show only projects I've used Copilot on" is a real workflow.
- Per-project counts (sessions, learnings, pending summaries) surface state that's invisible in the flat list.

## Non-goals

Out of scope for this change. Kept here so the implementation plan doesn't drift:

- Project pinning (today only sessions are pinnable; project ordering still reflects pinned-session presence via `pinned_count` and the existing sort modes).
- A project summary card / preview pane on the Projects view. It's full-width list only.
- Per-source split badges on rows (e.g. `[CC:5 OC:2]`).
- Archived-count column on project rows. Archived sessions stay in the Archived tab inside a project.
- Token / cost rollups on project rows.
- New session creation from the Projects view.

## Architecture

### Tier 1 — Projects (default landing)

- `AppState::new()` initializes `mode = AppMode::Projects` (was `Normal`).
- After the initial sessions load (and after SQLite hydration of pinned / learnings / summaries caches), `rebuild_projects()` runs once.
- The Projects view renders **full-width** — no right preview pane. The existing two-pane split is replaced by a single list when `mode == AppMode::Projects`.
- Title bar: `cc-speedy — Projects (N) [src: <CC|OC|CO|all>]`. Filter hint line: `/ search · s sort · → enter · q quit`.

### Tier 2 — Session list (drill-in)

- `→` or `Enter` on a project row sets `project_filter = Some(project_path)` and switches `mode = AppMode::Normal`.
- `Normal`'s existing rendering (left list + right preview), sub-modes (`/`, `g`, `r`, `t`, `l`, action menu, settings, etc.), and per-session actions (archive, pin, link, rename, resume) are unchanged. They simply now always run with `project_filter` set.
- Title bar inside a project: `cc-speedy — <project name> · N sessions [src: <CC|OC|CO|all>]`. Hint line: `← projects · / search · Esc clear`.

### Cross-project surfaces

`L` (Library) and `D` (Digest) remain reachable from both tiers. `Esc` from Library / Digest returns to whichever tier opened them — i.e. the dispatch into these modes records `mode` before transitioning, and the Esc handler restores it.

## Project row content

`ProjectRow` gains two fields:

```rust
pub struct ProjectRow {
    pub project_path: String,
    pub name: String,
    pub session_count: usize,
    pub pinned_count: usize,
    pub learnings_count: usize,   // NEW — sum across project's sessions
    pub pending_count: usize,     // NEW — sessions with no stored summary
    pub last_active: SystemTime,
}
```

Rendered row goes from:

```
● main                   cc-speedy                       12 last: 2h ago   *3
```

to:

```
● main                   cc-speedy                       12 📝8 ⏳1 last: 2h ago   *3
```

Emoji glyphs `📝` (learnings) and `⏳` (pending) match the existing style elsewhere in the codebase. If a terminal can't render them the existing column-truncation logic still keeps the row aligned; we don't fall back to ASCII unless this proves a portability problem in practice.

When the source filter is active, `session_count`, `learnings_count`, and `pending_count` reflect **only** the active-source subset. Projects with zero sessions of the active source disappear from the list entirely.

## Source filter behavior

The single `source_filter: Option<SessionSource>` field in `AppState` drives both tiers — there is no separate "project source filter" or "session source filter". `1` / `2` / `3` / `0` toggle this state from either view; the result depends only on which tier is currently rendering.

- **At Projects:** `rebuild_projects()` is re-invoked. Sessions are filtered by `source_filter` *before* grouping, so the project list collapses to projects with at least one matching session, and per-row counts are scoped to that source.
- **At session list:** existing `apply_filter()` behavior — already source-aware today.
- **Persistence across drill-in / drill-out:** `source_filter` is preserved. Drill into a project under `src: OC`, exit back, the filter is still `OC`. This is consistent because `source_filter` is shared state.

## `build_project_rows()` signature

```rust
fn build_project_rows(
    sessions: &[UnifiedSession],
    pinned: &HashSet<String>,
    has_learnings: &HashSet<String>,    // NEW — drives learnings_count
    summaries: &HashMap<String, String>, // NEW — drives pending_count (missing key → pending)
    source_filter: Option<SessionSource>, // NEW — pre-grouping filter
) -> Vec<ProjectRow>
```

Implementation:

1. Iterate `sessions`. If `source_filter.is_some()` and the session's source doesn't match, skip.
2. Group by `project_path`.
3. Per group, compute:
   - `session_count = group.len()`
   - `pinned_count = group.iter().filter(|s| pinned.contains(&s.id)).count()`
   - `learnings_count = group.iter().filter(|s| has_learnings.contains(&s.id)).count()`
   - `pending_count = group.iter().filter(|s| !summaries.contains_key(&s.id)).count()`
   - `last_active = group.iter().map(|s| s.modified).max().unwrap()`
4. Return `Vec<ProjectRow>` (sort applied separately by `sort_projects()`).

The `has_learnings` / `summaries` caches are already `Arc<Mutex<…>>` on `AppState`. `rebuild_projects()` snapshots them under their locks before calling `build_project_rows` to avoid holding locks across the grouping pass.

## `rebuild_projects()` triggers

| Trigger | Reason |
| --- | --- |
| Startup, after sessions are loaded and the SQLite-backed `pinned`, `has_learnings`, and `summaries` caches are populated | Initial population. |
| `1` / `2` / `3` / `0` while `mode == Projects` | Source filter scoping. |
| `←` exit from session list | Refresh counts to reflect any pin / archive / summary change made inside the project. |

It is **not** called every frame, every selection change, or on every keystroke. Computation is O(sessions); for typical users (<2k sessions) this is sub-millisecond, so we don't bother caching beyond the lifetime of the Projects view.

## Navigation keys

| Key | On Projects | On session list |
| --- | --- | --- |
| `→` / `Enter` | Drill into selected project | Open selected session (existing) |
| `←` | no-op | Exit to Projects (clears `project_filter`, `mode = Projects`, `rebuild_projects`) |
| `Esc` (top-level) | no-op | Clear `filter` query if non-empty; else no-op |
| `Esc` (sub-mode) | n/a | Exit sub-mode to session list (existing) |
| `1` / `2` / `3` / `0` | Re-scope project list by source | Re-scope session list by source (existing) |
| `P` | no-op | Alias for `←` (muscle memory) |
| `q` | Quit | Quit |
| `s` | Cycle project sort (existing) | Existing binding (Settings or other — unchanged from today) |
| `/` | Filter project list by name (existing) | Filter sessions (existing) |
| `D` / `L` | Open Digest / Library | Open Digest / Library |

### Esc precedence on session list (concrete)

In order:
1. If a sub-mode is active (`Filter`, `Grep`, `Rename`, `TagEdit`, `LinkPicker`, `Settings`, `Help`, `Digest`, `TurnDetail`, `ActionMenu`) → exit sub-mode to session list. Existing behavior.
2. Else if `filter` is non-empty → clear it.
3. Else → no-op. (Today this branch clears `project_filter`; it is **removed**.)

## What's removed / changed

- The "Esc on Normal with `project_filter.is_some()` clears the filter" branch (`tui.rs:884`).
- The `(AppMode::Normal, _, KeyCode::Char('P'))` entry path that *enters* the Projects modal (`tui.rs:1095`). `P` becomes the alias for `←` instead.
- The current `(AppMode::Projects, _, KeyCode::Esc)` handler at `tui.rs:1100` — today it pops back to `Normal` and tears down the projects vec. After this change Projects is the home view, so Esc is a no-op there and the projects vec is owned for the lifetime of the app, not torn down. (`q` still quits.)
- The user-facing concept of "all sessions across all projects, ungrouped". The flat list still exists internally as `Normal` rendering with `project_filter = Some(...)`, but no key path leads to it without a project scope.

## Migration risks

- **Muscle memory.** Existing users press `Esc` to back out. The Esc-clears-`project_filter` branch is gone. Mitigation: the title bar hint line on the session list explicitly reads `← projects` so the new exit key is visible at all times. Help screen (`?`) is updated.
- **Cache hydration ordering.** `rebuild_projects()` depends on `pinned`, `has_learnings`, and `summaries`. If the initial rebuild runs before SQLite hydration completes, `learnings_count` and `pending_count` show zero. Mitigation: gate the initial `rebuild_projects()` call behind the existing hydration completion signal (the same one that today populates the session list pre-render).
- **Empty Projects view on first launch.** If `sessions` is empty, the Projects view shows an empty list. Render an empty-state line: `No projects yet. Run a coding agent and your sessions will appear here.`
- **Help screen** (`?` mode) needs updating to document the two-tier model. This is a doc-only change inside `tui.rs` but easy to forget.

## Out of scope (restated to prevent scope creep)

- Project pinning.
- Project preview pane.
- Per-source split badges.
- Archived-count column.
- Token / cost rollups.
- New-session-from-project workflow.

## Testing

### Unit (`tests/projects_test.rs`, new or extend existing)

- `build_project_rows` with no source filter — counts accurate.
- `build_project_rows` with source filter — projects without matching sessions are absent; counts on remaining projects reflect filtered subset.
- `learnings_count` correctness against a synthetic `has_learnings` set.
- `pending_count` correctness against a synthetic `summaries` map.

### Integration (`tests/tui_test.rs` style)

- Boot lands on `AppMode::Projects` (not `Normal`).
- `→` on a project sets `project_filter` and `mode = Normal`; the session list shows only that project's sessions.
- `←` from the session list clears `project_filter`, returns `mode = Projects`, and `rebuild_projects()` ran (counts visible).
- `Esc` on a session list with a non-empty `filter` clears the filter and stays on session list. With empty filter it's a no-op.
- `1` on Projects collapses the list to projects with ≥1 CC session; drill-in preserves the filter.
- `P` on session list behaves identically to `←`.

### Snapshot

If a row-format snapshot test exists for project rows, update its golden output to include the new `📝` and `⏳` columns.

## Implementation order

A suggested phasing for the plan author:

1. Extend `ProjectRow` and `build_project_rows()` signature; update existing callers.
2. Wire `rebuild_projects()` into startup post-hydration. Verify counts populate.
3. Switch `AppState::new()` default `mode` to `Projects`; render full-width when in Projects.
4. Move the `→` / `Enter` drill-in path; rewire `←` and `P` as exit; remove the old Esc-clears-filter branch.
5. Make `1` / `2` / `3` / `0` re-invoke `rebuild_projects()` when `mode == Projects`.
6. Title-bar / hint-line copy. Help screen update.
7. Empty-state copy on Projects.
8. Tests — unit then integration.
