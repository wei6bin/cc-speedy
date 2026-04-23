# Git Status Column — Design Spec

**Date:** 2026-04-23
**Status:** Approved
**Part of:** [Productivity Roadmap](./2026-04-23-productivity-roadmap-design.md) — feature #5

---

## Overview

When triaging sessions, users can't tell at a glance which project has uncommitted work. They have to open tmux and run `git status` to check. Add a live per-row git indicator so the list itself becomes a triage surface.

The existing `git_branch` field on `UnifiedSession` is *historical* (the branch recorded in the session's JSONL metadata when the session ran). This feature adds *current* live state, orthogonal to the historical data.

---

## 1. Row Indicator

A single glyph rendered between the knowledge-base check (`kb_span`) and the session title, occupying 2 columns (glyph + trailing space).

| Glyph | Color | Meaning |
|-------|-------|---------|
| `●` | red | Dirty — `git status --porcelain` produced any output |
| `○` | green | Clean — porcelain empty |
| `·` | dim | Not a git repo, or path missing |
| `◦` | yellow | Stale / timed out / check errored |
| `  ` (two spaces) | — | Check still pending (pre-first-result only) |

Colors come from theme constants: `Color::Red`, `Color::Green`, `theme::FG_DIM`, `Color::Yellow`. Matches the existing palette ethos (glyph carries secondary signal, color carries primary signal).

Updated row layout:
```
[pin *] [date] [CC] [kb✓] [git●] [title 22] [msgs 4] [folder]
```

## 2. Refresh Model

**Startup batch.** After the main list is built, spawn one `tokio::task::spawn_blocking` per unique `project_path`. Each task runs `git -C <path> status --porcelain --branch` with a 500ms timeout and writes the result into the shared cache. The TUI renders with `  ` (pending) for rows whose entry hasn't landed yet; subsequent redraws pick up completed entries.

**Selection-change refresh.** When the user selects a row, if its cached entry is older than 30 seconds, enqueue a background refresh for that single project path. Non-blocking; the next redraw picks up the new value. Entries less than 30s old are not re-checked.

**Manual refresh (`g`).** The top-level `g` key (currently unbound) force-refreshes *all* cached entries in parallel, regardless of age. Status bar flashes `refreshing git…` for ~1s.

No periodic background sweep, no on-focus refresh — `g` is the escape hatch when automatic refresh isn't aggressive enough.

## 3. Preview Pane Branch Line

The preview pane currently shows `BRANCH: <git_branch>` (historical). Replace with live-aware rendering:

- Live branch and historical match, clean: `BRANCH: feat/auth`
- Live branch and historical match, dirty: `BRANCH: feat/auth  (dirty)`
- Live branch differs from historical, clean: `BRANCH: feat/auth  (ran on feat/bugfix)`
- Live branch differs from historical, dirty: `BRANCH: feat/auth  (ran on feat/bugfix)  (dirty)`
- Not a git repo: line omitted.
- Check pending / errored: fall back to historical only — `BRANCH: feat/bugfix` (no live annotation).

## 4. Data Model

**New module** `src/git_status.rs`:

```rust
#[derive(Clone, Debug, PartialEq)]
pub enum GitStatus {
    Clean  { branch: String },
    Dirty  { branch: String },
    NoGit,
    Error,      // timeout, command failed, etc.
}

/// Run `git -C <path> status --porcelain --branch` with a timeout.
/// Returns Clean/Dirty based on whether any file-state lines follow the
/// first `## <branch>...` line. Returns NoGit for non-repo paths,
/// Error for timeouts or any other failure mode.
pub fn check(path: &str, timeout_ms: u64) -> GitStatus;

/// Parse raw stdout of `git status --porcelain --branch` into a status.
/// Exposed for unit testing.
pub fn parse_porcelain(stdout: &str) -> GitStatus;
```

**Cache on `AppState`:**
```rust
git_status: Arc<Mutex<HashMap<String, (GitStatus, Instant)>>>,  // key = project_path
```

Instant is the timestamp the entry was populated; used for the 30s staleness check on selection change.

## 5. Concurrency

- Startup batch: bounded by number of unique project paths. Each runs in `tokio::task::spawn_blocking` (git is sync-IO-bound). No semaphore — N paths typically < 50, and each task holds a 500ms timeout.
- Selection-change refresh: fire-and-forget `tokio::spawn` of one `spawn_blocking` task. Multiple rapid selection changes may enqueue multiple refreshes for the same path; benign (last write wins; cache updated idempotently).
- `g` key: same pattern as startup batch — one `spawn_blocking` per unique path.

## 6. Git Command Choice

Single invocation: `git -C <path> status --porcelain --branch`

Output format:
```
## feat/auth...origin/feat/auth [ahead 2]
 M src/main.rs
?? new-file.txt
```
- Line 1: always `## <branch>...` — gives us the branch name.
- Lines 2+: any change indicates dirty. No lines 2+ = clean.
- Non-repo: returns non-zero exit + stderr "not a git repository".

Rejected alternatives:
- `git branch --show-current` + `git status --porcelain` — two subprocesses per path.
- `git rev-parse --abbrev-ref HEAD` — fails for detached HEAD; `--branch` handles it uniformly.

## 7. Keybinding

`g` at top level (`AppMode::Normal` and `AppMode::Grep`) — force-refresh all git status entries.

`g` is currently unbound. No conflict with existing bindings.

## 8. Files Changed

- `src/git_status.rs` — new, ~80 LOC.
- `src/tui.rs`
  - `AppState` field: `git_status: Arc<Mutex<HashMap<String, (GitStatus, Instant)>>>`.
  - Startup: unique-paths batch spawn.
  - Row rendering: new glyph span between kb_span and title.
  - Preview rendering: new branch line logic replacing current `branch_line`.
  - Key handler for `g` (Normal + Grep modes).
  - Selection-change: 30s staleness check, enqueue refresh.
- `src/theme.rs` — add `git_status_style(GitStatus) -> Style` helper (keeps the match-on-variant out of tui.rs).
- `tests/git_status_test.rs` — new, ~60 LOC.

## 9. Testing

**Unit (`git_status_test.rs`):**
- `parse_porcelain("## feat/x\n")` → `Clean { branch: "feat/x" }`.
- `parse_porcelain("## feat/x\n M src/foo.rs\n")` → `Dirty { branch: "feat/x" }`.
- `parse_porcelain("## feat/x\n?? untracked.txt\n")` → `Dirty { branch: "feat/x" }` (untracked counts).
- `parse_porcelain("## HEAD (no branch)\n")` → `Clean { branch: "HEAD (no branch)" }` (detached HEAD).
- `parse_porcelain("")` → `Error` (no branch line).
- End-to-end: `check(".", 500)` on this repo → `Dirty { branch: "master" }` during development (not asserted — environmental).

**Manual TUI:**
- Launch in a dir with mixed repos: dirty repo → red ●, clean → green ○, /tmp → dim ·.
- `g` triggers refresh; status bar flashes.
- Selection change > 30s apart updates the glyph.
- Branch mismatch preview shows `(ran on X)` annotation.

## 10. Risks / Open Questions

- **Startup lag.** If a user has 100 unique project paths and 5 of them are on a slow NFS mount, the slow ones hit the 500ms timeout and show `◦`. Acceptable — user sees most indicators instantly, slow outliers degrade gracefully.
- **Submodules / worktrees.** `git -C <path>` handles both correctly; no special casing needed.
- **Git not installed.** `check()` returns `Error` for every path; the column becomes a field of `◦`. Not actively handled; cost is low (one try per path) and users installing cc-speedy almost certainly have git.

## 11. Non-Goals

- No ahead/behind indicator. `git status --branch` reports it but we don't surface it in v1.
- No inline diff count. Glyph is binary dirty/clean.
- No auto-fetch or stash-count awareness.
- No persistence — cache is in-memory, discarded on TUI exit.
