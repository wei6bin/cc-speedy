# Refresh — Design

**Date:** 2026-04-28
**Status:** Draft, awaiting user review

## Problem

Today, the only way to pick up newly-created sessions in cc-speedy is to quit and relaunch the TUI. As users keep cc-speedy open while running coding agents in tmux, the session list grows stale — newly-created sessions and updated message counts don't appear until restart.

## Goal

Add a non-blocking refresh action that re-scans all session sources (Claude Code, OpenCode, Copilot) and updates the in-memory session list in place, preserving the user's current selection and view state.

## Non-goals

- Refreshing git status (already covered by `g`).
- Invalidating the insights cache (mtime-keyed in SQLite, self-invalidates).
- Regenerating session summaries (already covered by `Ctrl+R`).
- Incremental / delta scanning. We re-run the full unified merge each time.
- Refreshing aggregated views (Weekly Digest re-aggregation is out of scope).

## User-facing behavior

### Keybinds

- `R` (capital R) and `F5` both trigger refresh.
- Active in: `Normal`, `Library`, `Projects`.
- Ignored in all other modes (`Filter`, `Grep`, `Rename`, `ActionMenu`, `Settings`, `LibraryFilter`, `ProjectsFilter`, `TagEdit`, `LinkPicker`, `LinkPickerFilter`, `Digest`, `Help`, `TurnDetail`).

### Selection preservation

- Selection is tracked by `session_id`.
- After refresh, the same `session_id` is reselected if it's still visible under the current filters (source filter, search query, archived/active focus, tag filters).
- If the previous selection is no longer visible, selection falls back to the first row.
- Scroll position re-anchors to keep the selected row visible.

### Visual feedback

While the scan is in flight, a small `↻` glyph renders next to the source-filter badge (`[CC]` / `[OC]` / `[CO]` / `[All]`).

On completion, a status-line toast appears for ~2 seconds:

- `Refreshed: 142 sessions (+3 new, 5 updated)` — when something changed.
- `Refreshed: 142 sessions (no changes)` — when nothing changed.

Definitions:
- **new** = `session_id` was not in the prior list.
- **updated** = `session_id` existed and its `last_active` advanced.

### Concurrency

If a refresh is already in flight, additional `R` / `F5` presses are no-ops (silently dropped). Toast on completion of the in-flight scan is enough feedback.

## Architecture

### Refactor: startup uses the same code path as refresh

Today, `tui::run()` calls `unified::merge(...)` synchronously before entering the event loop. To keep refresh and startup as one logical operation, we refactor:

- Startup boots into the event loop with an empty session list and an `↻ Loading…` indicator.
- Startup immediately calls `AppState::refresh_sessions()` on the first frame.
- The first frame renders empty (or a "Loading sessions…" placeholder), and the list populates when the spawned scan completes.

This keeps "load sessions" as one path, eliminates duplication, and matches the TUI invariant from CLAUDE.md: *"the TUI never blocks. Anything I/O-bound goes through `tokio::spawn`."*

The current entry point is `unified::list_all_sessions()` at `src/tui.rs:866`. Both startup and refresh will call it through the same async wrapper.

### New AppState fields

The existing in-flight guards (`generating`, `insights_loading`) use `Arc<Mutex<HashSet<String>>>` because they are per-session. Refresh is a single global action, so a `AtomicBool` is sufficient and lighter.

```rust
// In-flight guard. Lighter than the per-session HashSet pattern because refresh is global.
refreshing: Arc<AtomicBool>,

// Channel for receiving refresh results into the event loop.
refresh_tx: tokio::sync::mpsc::UnboundedSender<RefreshResult>,
refresh_rx: tokio::sync::mpsc::UnboundedReceiver<RefreshResult>,

// Transient toast shown in the status line.
toast: Option<(String, Instant)>,  // text + display-until timestamp
```

### Public method

```rust
impl AppState {
    pub fn refresh_sessions(&self) {
        if self.refreshing.swap(true, Ordering::SeqCst) {
            return; // already refreshing
        }
        let prior_ids: HashSet<String> = self.sessions.iter().map(|s| s.id.clone()).collect();
        let prior_active: HashMap<String, DateTime<Utc>> =
            self.sessions.iter().map(|s| (s.id.clone(), s.last_active)).collect();
        let tx = self.refresh_tx.clone();
        let flag = self.refreshing.clone();

        tokio::spawn(async move {
            let new_sessions = tokio::task::spawn_blocking(|| {
                unified::list_all_sessions().unwrap_or_default()
            })
                .await
                .unwrap_or_default();

            let mut new_count = 0;
            let mut updated_count = 0;
            for s in &new_sessions {
                if !prior_ids.contains(&s.id) {
                    new_count += 1;
                } else if prior_active.get(&s.id).is_some_and(|t| s.last_active > *t) {
                    updated_count += 1;
                }
            }

            let _ = tx.send(RefreshResult {
                sessions: new_sessions,
                new_count,
                updated_count,
            });
            flag.store(false, Ordering::SeqCst);
        });
    }
}
```

### Result handling

The event loop drains `refresh_rx` each frame (alongside summary / insights / git-status receivers):

```rust
while let Ok(result) = self.refresh_rx.try_recv() {
    let total = result.sessions.len();
    self.sessions = result.sessions;
    self.recompute_filtered_indices();
    self.restore_selection_by_id();  // falls back to row 0 if missing
    self.toast = Some((
        if result.new_count == 0 && result.updated_count == 0 {
            format!("Refreshed: {total} sessions (no changes)")
        } else {
            format!(
                "Refreshed: {total} sessions (+{} new, {} updated)",
                result.new_count, result.updated_count
            )
        },
        Instant::now() + Duration::from_secs(2),
    ));
}
```

### Toast rendering

The status line at the bottom of the TUI has a slot for the toast. When `self.toast` is `Some` and not expired, render its text in dim style. When expired, clear it. Existing status-line content (counts, mode hint) compresses or hides to make room.

### Keybind dispatch

In the existing match-on-`KeyEvent`, add a clause for `Char('R')` and `F5` in the three target modes:

```rust
(AppMode::Normal | AppMode::Library | AppMode::Projects, KeyCode::Char('R'))
| (AppMode::Normal | AppMode::Library | AppMode::Projects, KeyCode::F(5)) => {
    self.refresh_sessions();
}
```

## Testing

New integration suite at `tests/refresh_test.rs`:

| Test | Setup | Assertion |
|------|-------|-----------|
| `picks_up_new_session` | Build temp project tree, init AppState, drop new JSONL, call `refresh_sessions`, drain channel | Session appears, `new_count == 1` |
| `preserves_selection` | Select session X, refresh | Still selected after |
| `falls_back_when_selection_gone` | Select X, delete X from disk, refresh | Selection moves to row 0, no panic |
| `counts_updated_sessions` | Append a message to existing JSONL, refresh | `updated_count == 1`, `new_count == 0` |
| `no_changes_message` | Refresh with no on-disk changes | Toast contains "no changes" |
| `inflight_guard_dedupes` | Call `refresh_sessions` twice in quick succession | Only one `unified::merge` runs (verify via instrumented test double) |
| `respects_filters` | Set source filter to CC-only, drop new OC session, refresh | New OC session present in `sessions` but not in filtered indices |

## Risks

- **Toast slot conflict:** the status line currently shows mode hints; we'll need to make sure the toast doesn't clip or hide important hints. Mitigation: dim the hints under the toast or shift them.
- **Selection ID mismatch:** if `unified::merge` ever produces a different `session_id` for the same session (e.g., normalization changes), selection-by-id will fail silently and fall back to row 0. Mitigation: assert id stability with the existing tests for `unified::merge`.
- **Startup refactor regression:** moving startup into the async path means the first frame is briefly empty. We will render a `Loading sessions…` placeholder so it doesn't look broken. Tests must still pass without modification (they don't depend on synchronous load order).

## Out-of-scope follow-ups

- A "refresh on focus" behavior (auto-refresh when the terminal regains focus) — possible, but out of scope here.
- Configurable auto-refresh interval — explicitly not building this; users can press `R`.
