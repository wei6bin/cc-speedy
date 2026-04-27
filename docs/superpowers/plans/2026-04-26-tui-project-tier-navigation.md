# TUI Two-Tier Project / Session Navigation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Promote `AppMode::Projects` to the default landing view; demote the flat session list to a per-project drill-in. Boot lands on Projects; `→`/`Enter` enters a project; `←` exits.

**Architecture:** Reuse the existing `Projects` mode + `ProjectRow` model. Extend `ProjectRow` with `learnings_count` and `pending_count`. Extend `build_project_rows()` to accept the learnings cache, summaries map, and source filter. Source filter (`1/2/3/0`) is shared state and re-invokes `rebuild_projects()` while on Projects. Remove the "Esc clears `project_filter`" branch on `Normal`; `←`/`P` become the only exit keys. Drop the user-facing flat list.

**Tech Stack:** Rust, ratatui, crossterm, rusqlite (existing). No new deps.

**Spec:** `docs/superpowers/specs/2026-04-26-tui-project-tier-navigation-design.md`

---

## File Map

| File | Change |
| --- | --- |
| `src/tui.rs` | Extend `ProjectRow`; change `build_project_rows()` signature; update `rebuild_projects()`; switch default `mode`; add `←`/`P` exit handlers in Normal; remove Esc-clears-`project_filter` branch; add source filter handlers in Projects; update title/hint/help/empty-state copy; update single existing call site of `build_project_rows`. |
| `tests/project_dashboard_test.rs` | Update existing 5 tests to use the new signature; add tests for `learnings_count`, `pending_count`, source-filter scoping. |

No new files. No new modules.

---

## Task 1: Extend `ProjectRow` struct

**Files:**
- Modify: `src/tui.rs:63-69`

- [ ] **Step 1: Extend the struct definition**

Replace the existing `ProjectRow` (`src/tui.rs:63-69`):

```rust
pub struct ProjectRow {
    pub project_path: String,
    pub name: String,
    pub session_count: usize,
    pub pinned_count: usize,
    pub last_active: std::time::SystemTime,
}
```

with:

```rust
pub struct ProjectRow {
    pub project_path: String,
    pub name: String,
    pub session_count: usize,
    pub pinned_count: usize,
    pub learnings_count: usize,
    pub pending_count: usize,
    pub last_active: std::time::SystemTime,
}
```

- [ ] **Step 2: Verify compilation breaks**

Run: `cargo build 2>&1 | head -40`
Expected: errors at the `or_insert_with` block in `build_project_rows` (struct literal missing fields) and possibly at the row renderer in `draw_projects`. We'll fix both in the next tasks.

- [ ] **Step 3: Commit (yet — wait for Task 2 to land together)**

Hold the commit until Task 2 finishes so the tree builds at every commit boundary.

---

## Task 2: Extend `build_project_rows()` signature and tests (TDD)

**Files:**
- Modify: `src/tui.rs:559-585`
- Modify: `tests/project_dashboard_test.rs`

The existing fn signature (`src/tui.rs:559`):

```rust
pub fn build_project_rows(
    sessions: &[UnifiedSession],
    pinned: &std::collections::HashSet<String>,
) -> Vec<ProjectRow>
```

becomes:

```rust
pub fn build_project_rows(
    sessions: &[UnifiedSession],
    pinned: &std::collections::HashSet<String>,
    has_learnings: &std::collections::HashSet<String>,
    summaries: &std::collections::HashMap<String, String>,
    source_filter: Option<crate::unified::SessionSource>,
) -> Vec<ProjectRow>
```

- [ ] **Step 1: Update existing tests to compile against the new signature**

Open `tests/project_dashboard_test.rs`. At the top, add imports:

```rust
use cc_speedy::tui::build_project_rows;
use cc_speedy::unified::{SessionSource, UnifiedSession};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, UNIX_EPOCH};
```

Replace every existing call of the form `build_project_rows(&sessions, &pinned)` and `build_project_rows(&sessions, &HashSet::new())` with `build_project_rows(&sessions, &pinned, &HashSet::new(), &HashMap::new(), None)` (preserving each test's existing `pinned` value). The test cases themselves don't change semantics — they verify the new signature still produces the old behavior when the new args are empty / `None`.

- [ ] **Step 2: Add a test for `learnings_count`**

Append to `tests/project_dashboard_test.rs`:

```rust
#[test]
fn test_learnings_count() {
    let sessions = vec![
        mk("s1", "/repo/alpha", 100),
        mk("s2", "/repo/alpha", 200),
        mk("s3", "/repo/alpha", 300),
        mk("s4", "/repo/beta", 100),
    ];
    let has_learnings: HashSet<String> = ["s1".into(), "s3".into(), "s4".into()]
        .into_iter()
        .collect();
    let rows = build_project_rows(
        &sessions,
        &HashSet::new(),
        &has_learnings,
        &HashMap::new(),
        None,
    );
    let alpha = rows.iter().find(|r| r.project_path == "/repo/alpha").unwrap();
    let beta = rows.iter().find(|r| r.project_path == "/repo/beta").unwrap();
    assert_eq!(alpha.learnings_count, 2);
    assert_eq!(beta.learnings_count, 1);
}
```

- [ ] **Step 3: Add a test for `pending_count`**

Append:

```rust
#[test]
fn test_pending_count_counts_sessions_without_summary() {
    let sessions = vec![
        mk("s1", "/repo/alpha", 100),
        mk("s2", "/repo/alpha", 200),
        mk("s3", "/repo/alpha", 300),
    ];
    let mut summaries: HashMap<String, String> = HashMap::new();
    summaries.insert("s1".into(), "summary text".into());
    // s2 and s3 have no stored summary => pending
    let rows = build_project_rows(
        &sessions,
        &HashSet::new(),
        &HashSet::new(),
        &summaries,
        None,
    );
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].pending_count, 2);
}
```

- [ ] **Step 4: Add a test for source-filter scoping**

Append:

```rust
fn mk_with_source(id: &str, path: &str, secs: u64, source: SessionSource) -> UnifiedSession {
    let mut s = mk(id, path, secs);
    s.source = source;
    s
}

#[test]
fn test_source_filter_drops_projects_with_no_matching_sessions() {
    let sessions = vec![
        mk_with_source("a1", "/repo/alpha", 100, SessionSource::ClaudeCode),
        mk_with_source("a2", "/repo/alpha", 200, SessionSource::OpenCode),
        mk_with_source("b1", "/repo/beta", 100, SessionSource::OpenCode),
        mk_with_source("c1", "/repo/gamma", 100, SessionSource::Copilot),
    ];
    let rows = build_project_rows(
        &sessions,
        &HashSet::new(),
        &HashSet::new(),
        &HashMap::new(),
        Some(SessionSource::ClaudeCode),
    );
    // only alpha has a CC session
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].project_path, "/repo/alpha");
    assert_eq!(rows[0].session_count, 1); // only a1, not a2
}

#[test]
fn test_source_filter_scopes_counts() {
    let sessions = vec![
        mk_with_source("a1", "/repo/alpha", 100, SessionSource::ClaudeCode),
        mk_with_source("a2", "/repo/alpha", 200, SessionSource::ClaudeCode),
        mk_with_source("a3", "/repo/alpha", 300, SessionSource::OpenCode),
    ];
    let pinned: HashSet<String> = ["a1".into(), "a3".into()].into_iter().collect();
    let has_learnings: HashSet<String> = ["a2".into(), "a3".into()].into_iter().collect();
    let rows = build_project_rows(
        &sessions,
        &pinned,
        &has_learnings,
        &HashMap::new(),
        Some(SessionSource::ClaudeCode),
    );
    assert_eq!(rows.len(), 1);
    let r = &rows[0];
    assert_eq!(r.session_count, 2);   // a1 + a2 only
    assert_eq!(r.pinned_count, 1);    // a1 (not a3 — wrong source)
    assert_eq!(r.learnings_count, 1); // a2 (not a3 — wrong source)
    assert_eq!(r.pending_count, 2);   // both a1 and a2 have no summary entry
}
```

- [ ] **Step 5: Run tests — they should fail to compile**

Run: `cargo test --test project_dashboard_test 2>&1 | head -30`
Expected: compile error — `build_project_rows` arg count mismatch (or struct literal field missing in `tui.rs` from Task 1).

- [ ] **Step 6: Implement the new `build_project_rows`**

Replace `src/tui.rs:559-585`:

```rust
/// Group sessions by `project_path` into Project Dashboard rows.
/// Archived sessions are included in counts. Last-active is the max of
/// session.modified across the group. Pinned count is the number of
/// sessions in the group whose id is in the pinned set.
///
/// When `source_filter` is `Some`, sessions whose source doesn't match are
/// skipped before grouping; projects with no matching sessions disappear
/// from the result, and per-row counts reflect only the filtered subset.
pub fn build_project_rows(
    sessions: &[UnifiedSession],
    pinned: &std::collections::HashSet<String>,
    has_learnings: &std::collections::HashSet<String>,
    summaries: &std::collections::HashMap<String, String>,
    source_filter: Option<crate::unified::SessionSource>,
) -> Vec<ProjectRow> {
    use std::collections::HashMap;
    let mut acc: HashMap<String, ProjectRow> = HashMap::new();
    for s in sessions {
        if let Some(sf) = source_filter {
            if s.source != sf {
                continue;
            }
        }
        let row = acc
            .entry(s.project_path.clone())
            .or_insert_with(|| ProjectRow {
                project_path: s.project_path.clone(),
                name: crate::util::path_last_n(&s.project_path, 2),
                session_count: 0,
                pinned_count: 0,
                learnings_count: 0,
                pending_count: 0,
                last_active: std::time::UNIX_EPOCH,
            });
        row.session_count += 1;
        if pinned.contains(&s.session_id) {
            row.pinned_count += 1;
        }
        if has_learnings.contains(&s.session_id) {
            row.learnings_count += 1;
        }
        if !summaries.contains_key(&s.session_id) {
            row.pending_count += 1;
        }
        if s.modified > row.last_active {
            row.last_active = s.modified;
        }
    }
    acc.into_values().collect()
}
```

Note: `SessionSource` derives `Debug, Clone, PartialEq` in `src/unified.rs` (not `Copy`). The `if let Some(ref sf) = source_filter` + `&s.source != sf` form avoids a move.

- [ ] **Step 7: Update the single in-tree caller**

`src/tui.rs:308` currently reads:

```rust
self.projects = build_project_rows(&self.sessions, &self.pinned);
```

Replace with:

```rust
let has_learnings = self
    .has_learnings
    .lock()
    .unwrap_or_else(|e| e.into_inner())
    .clone();
let summaries = self
    .summaries
    .lock()
    .unwrap_or_else(|e| e.into_inner())
    .clone();
self.projects = build_project_rows(
    &self.sessions,
    &self.pinned,
    &has_learnings,
    &summaries,
    self.source_filter.clone(),
);
```

The `.clone()` on the locked guards snapshots the cache contents and releases the lock immediately — `build_project_rows` runs on the snapshot. This avoids holding the mutex across the grouping pass. `unwrap_or_else(|e| e.into_inner())` matches the codebase convention elsewhere in `tui.rs` for recovering from a poisoned mutex (the alternative `unwrap_or_default()` would silently drop cache data on poisoning, making counts wrong). `SessionSource` derives `Clone` (not `Copy`), so `self.source_filter.clone()` is needed to pass by value.

- [ ] **Step 8: Run the test suite**

Run: `cargo test --test project_dashboard_test 2>&1 | tail -20`
Expected: all tests pass (5 existing + 4 new).

Run: `cargo build 2>&1 | tail -10`
Expected: build succeeds.

- [ ] **Step 9: Commit Tasks 1 + 2 together**

```bash
git add src/tui.rs tests/project_dashboard_test.rs
git commit -m "$(cat <<'EOF'
feat(tui): extend ProjectRow with learnings_count and pending_count

build_project_rows now takes has_learnings, summaries, and source_filter.
Projects with no sessions matching the active source are dropped; per-row
counts (sessions, pinned, learnings, pending) reflect only the filtered
subset. Existing call site updated; unit tests cover scoping behavior.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Render new columns on project rows

**Files:**
- Modify: `src/tui.rs:2620-2650` (the `Line::from(vec![…])` block inside `draw_projects`)

- [ ] **Step 1: Replace the row spans**

Find the block at `src/tui.rs:2620` (the existing row rendering inside `.map(|p| { … Line::from(…) })`):

```rust
Line::from(vec![
    Span::styled(format!("{} ", glyph), Style::default().fg(gcolor)),
    Span::styled(
        format!("{:<20} ", truncate(&branch_str, 20)),
        theme::dim_style(),
    ),
    Span::styled(
        format!("{:<28}", truncate(&p.name, 28)),
        Style::default().fg(theme::FG),
    ),
    Span::styled(format!("{:>4} ", p.session_count), theme::dim_style()),
    Span::styled(
        format!("last: {}", format_time(p.last_active)),
        theme::dim_style(),
    ),
    Span::styled(pin_str, theme::pin_style()),
])
```

with:

```rust
let learnings_str = if p.learnings_count > 0 {
    format!("📝{} ", p.learnings_count)
} else {
    "    ".to_string()
};
let pending_str = if p.pending_count > 0 {
    format!("⏳{} ", p.pending_count)
} else {
    "    ".to_string()
};
Line::from(vec![
    Span::styled(format!("{} ", glyph), Style::default().fg(gcolor)),
    Span::styled(
        format!("{:<20} ", truncate(&branch_str, 20)),
        theme::dim_style(),
    ),
    Span::styled(
        format!("{:<28}", truncate(&p.name, 28)),
        Style::default().fg(theme::FG),
    ),
    Span::styled(format!("{:>4} ", p.session_count), theme::dim_style()),
    Span::styled(learnings_str, theme::dim_style()),
    Span::styled(pending_str, theme::dim_style()),
    Span::styled(
        format!("last: {}", format_time(p.last_active)),
        theme::dim_style(),
    ),
    Span::styled(pin_str, theme::pin_style()),
])
```

The fixed-width fallback strings (`"    "`) keep columns aligned when a project has zero learnings or pending summaries.

- [ ] **Step 2: Run the build**

Run: `cargo build 2>&1 | tail -10`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add src/tui.rs
git commit -m "$(cat <<'EOF'
feat(tui): render learnings and pending counts on project rows

Adds 📝N and ⏳N columns between session_count and last-active. Empty
slots padded so columns stay aligned.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Switch default mode to Projects + initial rebuild

**Files:**
- Modify: `src/tui.rs:198` (default `mode` in `AppState::new`)
- Modify: `src/tui.rs` around `AppState::new`'s tail (insert post-construct rebuild)

- [ ] **Step 1: Change the default mode**

In `src/tui.rs`, around line 198, change:

```rust
mode: AppMode::Normal,
```

to:

```rust
mode: AppMode::Projects,
```

- [ ] **Step 2: Trigger initial `rebuild_projects()` after construction**

The current tail of `AppState::new` (`src/tui.rs:248-251`) reads:

```rust
        // Split archived out of the active list on startup so the "all" view
        // correctly shows archived sessions in the bottom-left panel.
        state.apply_filter();
        Ok(state)
```

`state` is already declared `let mut state = Self { … };` so no `mut` change is needed. Insert one line before `Ok(state)`:

```rust
        // Split archived out of the active list on startup so the "all" view
        // correctly shows archived sessions in the bottom-left panel.
        state.apply_filter();
        state.rebuild_projects();
        Ok(state)
```

This runs **after** `summaries_map`, `pinned`, and `has_learnings` have been hydrated from SQLite (lines 171, 174-175 of the same fn), so the new `learnings_count` and `pending_count` fields populate correctly on the first paint.

- [ ] **Step 3: Run the build**

Run: `cargo build 2>&1 | tail -10`
Expected: success.

- [ ] **Step 4: Smoke-test by launching**

Run: `cargo run 2>&1 | head -1` then immediately `q` to quit.
Expected: no crash on startup. (Manual visual verification: Projects view appears full-width on launch.)

- [ ] **Step 5: Commit**

```bash
git add src/tui.rs
git commit -m "$(cat <<'EOF'
feat(tui): boot into Projects view; populate rows on startup

Default AppMode is now Projects (was Normal). rebuild_projects() runs
once at the tail of AppState::new so counts are populated before the
first render.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Source filter handlers in Projects mode

**Files:**
- Modify: `src/tui.rs:1508-1523` (existing source-filter key handlers)

The current handlers are scoped to `(AppMode::Normal, …, KeyCode::Char('1'|'2'|'3'|'0'))`. Add parallel handlers for `AppMode::Projects` that update `source_filter` and re-invoke `rebuild_projects()` + `apply_projects_filter()`.

- [ ] **Step 1: Add Projects-mode source filter handlers**

Immediately after the existing `(AppMode::Normal, _, KeyCode::Char('0'))` arm at `src/tui.rs:1521-1524`:

```rust
(AppMode::Normal, KeyModifiers::NONE, KeyCode::Char('0')) => {
    app.source_filter = None;
    app.apply_filter();
}
```

insert four new arms:

```rust
(AppMode::Projects, KeyModifiers::NONE, KeyCode::Char('1')) => {
    app.source_filter = Some(SessionSource::ClaudeCode);
    app.rebuild_projects();
}
(AppMode::Projects, KeyModifiers::NONE, KeyCode::Char('2')) => {
    app.source_filter = Some(SessionSource::OpenCode);
    app.rebuild_projects();
}
(AppMode::Projects, KeyModifiers::NONE, KeyCode::Char('3')) => {
    app.source_filter = Some(SessionSource::Copilot);
    app.rebuild_projects();
}
(AppMode::Projects, KeyModifiers::NONE, KeyCode::Char('0')) => {
    app.source_filter = None;
    app.rebuild_projects();
}
```

`rebuild_projects()` already calls `sort_projects()` and `apply_projects_filter()` internally (per `src/tui.rs:307-311`), so no extra calls are needed.

- [ ] **Step 2: Run the build**

Run: `cargo build 2>&1 | tail -10`
Expected: success.

- [ ] **Step 3: Manual smoke test**

Launch: `cargo run`. On the Projects view, press `2` then `0`. Expected: pressing `2` collapses the list to projects with ≥1 OpenCode session (or shows empty if none); `0` restores the full list. Quit with `q`.

- [ ] **Step 4: Commit**

```bash
git add src/tui.rs
git commit -m "$(cat <<'EOF'
feat(tui): source filter (1/2/3/0) at Projects tier

Re-invokes rebuild_projects so the project list collapses to projects
with at least one session of the selected source, with all per-row
counts scoped accordingly. State persists across drill-in/drill-out.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: `←` and `P` exit handlers on Normal

**Files:**
- Modify: `src/tui.rs` Normal-mode key arms (around `tui.rs:881-895` and surrounding)

- [ ] **Step 1: Add a `KeyCode::Left` arm on Normal that exits to Projects**

Locate the block of `(AppMode::Normal, …)` arms near `src/tui.rs:881-895`. After the existing `(AppMode::Normal, _, KeyCode::Char('q'))` quit arm, add:

```rust
(AppMode::Normal, _, KeyCode::Left) => {
    app.project_filter = None;
    app.mode = AppMode::Projects;
    app.rebuild_projects();
}
```

- [ ] **Step 2: Repurpose `P` on Normal to alias `Left`**

The current `P` handler is at `src/tui.rs:1095-1099`:

```rust
(AppMode::Normal, _, KeyCode::Char('P')) => {
    app.projects_filter.clear();
    app.rebuild_projects();
    app.mode = AppMode::Projects;
}
```

Replace with:

```rust
(AppMode::Normal, _, KeyCode::Char('P')) => {
    app.project_filter = None;
    app.projects_filter.clear();
    app.mode = AppMode::Projects;
    app.rebuild_projects();
}
```

(Functional change: also clears `project_filter` like `Left` does. The existing `projects_filter.clear()` keeps the project search box empty when re-entering Projects, which we preserve.)

- [ ] **Step 3: Build**

Run: `cargo build 2>&1 | tail -10`
Expected: success.

- [ ] **Step 4: Manual smoke test**

Launch `cargo run`. On Projects view, `→` (or `Enter`) into a project. Press `←`. Expected: returns to Projects view, full project list visible. Repeat with `P`. Same outcome. Quit with `q`.

- [ ] **Step 5: Commit**

```bash
git add src/tui.rs
git commit -m "$(cat <<'EOF'
feat(tui): ← / P exit session list back to Projects

Both keys clear project_filter, switch mode to Projects, and rebuild
the project rows so any pin/archive/summary changes inside the project
are reflected in the count columns.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: Remove Esc-clears-`project_filter` on Normal; no-op Esc on Projects

**Files:**
- Modify: `src/tui.rs:884-890` (the `Esc` arm that clears `project_filter`)
- Modify: `src/tui.rs:1100-1105` (the `Esc` arm on Projects)

- [ ] **Step 1: Replace the Esc-clears-project_filter branch with a search-clear branch**

Today, `tui.rs:884-890` is the **only** `KeyCode::Esc` arm on `AppMode::Normal`:

```rust
(AppMode::Normal, _, KeyCode::Esc) if app.project_filter.is_some() => {
    app.project_filter = None;
    app.apply_filter();
    app.set_status("project filter cleared".to_string());
}
```

Replace it with:

```rust
(AppMode::Normal, _, KeyCode::Esc) if !app.filter.is_empty() => {
    app.filter.clear();
    app.apply_filter();
}
```

After this change, Esc on Normal:
- with an active in-view search filter → clears the search and re-applies (new behavior).
- with no search → no arm matches → falls through to the catch-all (no-op).
- with a committed search and `project_filter` set → still clears just the search; `project_filter` stays. Use `←` / `P` to leave the project.

(There is a separate `(AppMode::Filter, _, KeyCode::Esc)` arm at `tui.rs:1308-1312` for the live-typing filter mode — it is unaffected.)

- [ ] **Step 2: Change Esc on Projects to a no-op (preserve state)**

The current arm at `src/tui.rs:1100-1105`:

```rust
(AppMode::Projects, _, KeyCode::Esc) => {
    app.mode = AppMode::Normal;
    app.projects.clear();
    app.projects_filtered.clear();
    app.projects_filter.clear();
}
```

Replace with:

```rust
(AppMode::Projects, _, KeyCode::Esc) if !app.projects_filter.is_empty() => {
    app.projects_filter.clear();
    app.apply_projects_filter();
}
```

The new arm only fires when there's an active project search; it clears the search and stays on Projects. If no search is active, Esc is a no-op (no arm matches → falls through to the catch-all). The state-teardown of the projects vec is removed because Projects is now persistent.

- [ ] **Step 3: Build**

Run: `cargo build 2>&1 | tail -10`
Expected: success.

- [ ] **Step 4: Manual smoke test**

Launch `cargo run`. On Projects, press `/`, type "x", `Enter` to commit search. Press `Esc`. Expected: search clears, projects re-listed. Press `Esc` again. Expected: no-op. `→` into a project. Press `Esc`. Expected: no-op (or, if you have an active session search via `/`, it clears that). Press `←`. Expected: back to Projects.

- [ ] **Step 5: Commit**

```bash
git add src/tui.rs
git commit -m "$(cat <<'EOF'
feat(tui): drop Esc-clears-project_filter; Projects Esc clears search only

Esc on session list no longer clears project_filter — ← / P are the
explicit exit keys. Esc on Projects clears the project search if
non-empty, else no-op. The projects vec is no longer torn down on Esc
since Projects is now the persistent home view.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: Title and hint copy

**Files:**
- Modify: `src/tui.rs:2330-2347` (the per-mode `bar_text` / `bar_title` block)

- [ ] **Step 1: Update the Normal hint to mention `← projects`**

Locate the `AppMode::Normal => {` arm in the `match app.mode` for `bar_text` (around `src/tui.rs:2333-2345`). The current block is:

```rust
AppMode::Normal => {
    let hint = if let Some(ref pp) = app.project_filter {
        format!(
            "  project: {}  (Esc to clear)",
            crate::util::path_last_n(pp, 2)
        )
    } else if app.filter.is_empty() {
        "  (F1: help  /: filter  ?: grep  L: library  P: projects)".to_string()
    } else {
        format!("  filter: {}", app.filter)
    };
    (hint, " cc-speedy ")
}
```

Replace with:

```rust
AppMode::Normal => {
    let src_tag = match &app.source_filter {
        None => "all",
        Some(crate::unified::SessionSource::ClaudeCode) => "CC",
        Some(crate::unified::SessionSource::OpenCode) => "OC",
        Some(crate::unified::SessionSource::Copilot) => "CO",
    };
    let hint = if let Some(ref pp) = app.project_filter {
        if app.filter.is_empty() {
            format!(
                "  project: {}  [src: {}]  (← projects · / search)",
                crate::util::path_last_n(pp, 2),
                src_tag
            )
        } else {
            format!("  filter: {}  (Esc clear)", app.filter)
        }
    } else if app.filter.is_empty() {
        "  (F1: help  /: filter  ?: grep  L: library)".to_string()
    } else {
        format!("  filter: {}", app.filter)
    };
    (hint, " cc-speedy ")
}
```

(Removes the stale `P: projects` hint from the no-filter branch since `P` is no longer a Normal-entry shortcut for that purpose; it's now an exit alias. Adds the source-tag and explicit `← projects` cue when scoped to a project.)

- [ ] **Step 2: Update the Projects hint**

The current `AppMode::Projects` arm (`src/tui.rs:2291-2306`) reads:

```rust
        AppMode::Projects => {
            let sort_label = match app.projects_sort {
                ProjectSort::LastActive => "last active",
                ProjectSort::SessionCount => "session count",
                ProjectSort::Alphabetical => "alphabetical",
            };
            let n = app.projects_filtered.len();
            (
                format!(
                    "  sort: {}  ·  {} project{}  (/: filter  s: sort  Enter: drill  Esc: exit)",
                    sort_label,
                    n,
                    if n == 1 { "" } else { "s" }
                ),
                " Project Dashboard ",
            )
        }
```

Replace with:

```rust
        AppMode::Projects => {
            let sort_label = match app.projects_sort {
                ProjectSort::LastActive => "last active",
                ProjectSort::SessionCount => "session count",
                ProjectSort::Alphabetical => "alphabetical",
            };
            let src_tag = match app.source_filter {
                None => "all",
                Some(crate::unified::SessionSource::ClaudeCode) => "CC",
                Some(crate::unified::SessionSource::OpenCode) => "OC",
                Some(crate::unified::SessionSource::Copilot) => "CO",
            };
            let n = app.projects_filtered.len();
            (
                format!(
                    "  sort: {}  ·  src: {}  ·  {} project{}  (/: search  s: sort  →: enter  q: quit)",
                    sort_label,
                    src_tag,
                    n,
                    if n == 1 { "" } else { "s" }
                ),
                " Project Dashboard ",
            )
        }
```

The `AppMode::ProjectsFilter` arm at `src/tui.rs:2308-2311` is fine as-is — leave it.

- [ ] **Step 3: Build**

Run: `cargo build 2>&1 | tail -10`
Expected: success.

- [ ] **Step 4: Manual visual check**

Launch `cargo run`. Verify Projects view title bar shows `Projects (N) [src: all]` etc., and after drill-in the bar shows `project: <name> [src: …] (← projects · / search)`.

- [ ] **Step 5: Commit**

```bash
git add src/tui.rs
git commit -m "$(cat <<'EOF'
feat(tui): refresh title-bar hints for two-tier navigation

Projects view shows project count and source tag. Session list shows
'← projects' cue so the new exit key is always visible.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: Update help popup

**Files:**
- Modify: `src/tui.rs:3697-3760` (the `lines` vec inside `draw_help_popup`)

- [ ] **Step 1: Add a Projects section to the help screen**

Find the existing `lines` vec inside `draw_help_popup` (`src/tui.rs:3697`). Insert a new section immediately after the "Source filter" block (before "Sessions"):

```rust
Line::from(""),
Line::from(vec![Span::styled("  Projects view (default)", theme::title_style())]),
Line::from("    →  / Enter   drill into selected project"),
Line::from("    /            search projects by name"),
Line::from("    s            cycle sort (last active / count / a-z)"),
Line::from("    1/2/3/0      filter project list by source"),
Line::from(""),
Line::from(vec![Span::styled("  Inside a project", theme::title_style())]),
Line::from("    ←  / P       back to Projects"),
Line::from("    Esc          clear in-view search (if any)"),
```

Locate the existing single line that says `Line::from("    /            filter (project + title)"),` under the "Sessions" heading and leave it alone — that's still the in-project search.

- [ ] **Step 2: Build**

Run: `cargo build 2>&1 | tail -10`
Expected: success.

- [ ] **Step 3: Manual visual check**

Launch `cargo run`, press `F1`, scroll the popup, confirm the new sections render.

- [ ] **Step 4: Commit**

```bash
git add src/tui.rs
git commit -m "$(cat <<'EOF'
docs(tui): document two-tier navigation in help popup

Adds Projects view and 'inside a project' sections covering drill-in,
exit, and search semantics.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 10: Empty-state copy on Projects

**Files:**
- Modify: `src/tui.rs:2598-2670` (top of `draw_projects`)

- [ ] **Step 1: Render an empty-state line when `projects_filtered` is empty**

Inside `draw_projects` at `src/tui.rs:2598`, find where `items` is built (the `.map(ListItem::new).collect()` chain). Just after that — and before the `let title = …` line — insert:

```rust
let items: Vec<ListItem> = if items.is_empty() {
    let msg = if app.projects.is_empty() {
        "  No projects yet. Start a coding agent and your sessions will appear here."
    } else {
        "  No projects match the current filter."
    };
    vec![ListItem::new(Line::from(Span::styled(msg, theme::dim_style())))]
} else {
    items
};
```

This wraps the existing `items` so an empty result renders one explanatory line without changing the surrounding list/widget code.

- [ ] **Step 2: Build**

Run: `cargo build 2>&1 | tail -10`
Expected: success.

- [ ] **Step 3: Manual smoke test (best-effort)**

Launch `cargo run`. On Projects, press `2` (OpenCode filter). If you have no OC sessions, expect: "No projects match the current filter." Press `0` to restore.

- [ ] **Step 4: Commit**

```bash
git add src/tui.rs
git commit -m "$(cat <<'EOF'
feat(tui): empty-state line on Projects view

Shows 'No projects yet…' on first launch and 'No projects match the
current filter.' when source/search filter excludes everything.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 11: Final verification

**Files:** none — verification only.

- [ ] **Step 1: Run the full test suite**

Run: `cargo test 2>&1 | tail -30`
Expected: every suite passes. `project_dashboard_test` is the new ground truth for the row builder; everything else should be unaffected.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --all-targets 2>&1 | tail -20`
Expected: no new warnings. (Pre-existing warnings, if any, are out of scope.)

- [ ] **Step 3: Format**

Run: `cargo fmt`
Expected: no diff (or a small whitespace-only diff that is then committed).

If `cargo fmt` produced changes:

```bash
git add -A
git commit -m "$(cat <<'EOF'
style: cargo fmt

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 4: Manual end-to-end walk-through**

Launch `cargo run` and walk through the user flows. The expected behavior at each step:

1. App boots straight into the Projects view (full-width list, title bar reads `Projects (N) [src: all] …`).
2. Each project row shows: glyph, branch, name, session count, 📝learnings, ⏳pending, last-active, ★pins (with empty cells where counts are zero).
3. `j/k` or `↑/↓` moves selection. `/` opens search; typing filters; `Esc` or empty search restores.
4. `s` cycles project sort. `1` shows only CC projects with counts scoped to CC; `0` restores all.
5. `→` (or `Enter`) drills into the selected project. The session list appears with the title bar reading `project: <name> [src: …] (← projects · / search)`.
6. Inside the project: every existing key (`a`, `x`, `t`, `r`, `l`, `?`, `i`, etc.) works as before.
7. `←` returns to Projects. The 📝/⏳ counts reflect any pin/archive/summarize done inside the project (since `rebuild_projects` ran on exit).
8. `P` from the session list does the same as `←`.
9. `Esc` on the session list with an active `/` search clears the search; with no search, it does nothing.
10. `q` quits from any view.

If any step diverges from the description, reopen the relevant task before declaring done.

- [ ] **Step 5: Push**

```bash
git push
```

(Skip if working in a worktree that will be merged later.)

---

## Spec coverage check

| Spec section | Task |
| --- | --- |
| Tier 1 — Projects default landing | Task 4 |
| Tier 2 — drill-in via `→`/`Enter` | Existing code (verified in Task 11) |
| Cross-project surfaces unaffected (L/D) | No change required (Task 11 verifies) |
| ProjectRow new fields | Task 1 |
| `build_project_rows` signature change | Task 2 |
| Render new columns | Task 3 |
| Source filter at both tiers | Task 5 |
| `←` / `P` exit | Task 6 |
| Esc precedence (drop project_filter clear; no-op Projects Esc; preserve search-clear) | Task 7 |
| Title / hint copy | Task 8 |
| Help screen update | Task 9 |
| Empty-state copy | Task 10 |
| Cache hydration ordering (initial rebuild after caches load) | Task 4 (rebuild called at tail of `AppState::new`, post-DB-hydration) |
| Unit tests for build_project_rows | Task 2 |
