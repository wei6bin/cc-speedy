# OpenCode Integration Design

Date: 2026-03-10

## Overview

Extend cc-speedy to be a **unified session browser** for both Claude Code (existing) and OpenCode
(new), presenting sessions from both tools in a single TUI with a consistent experience.

---

## Background: Storage Comparison

| Dimension          | Claude Code (existing)                              | OpenCode (new)                                     |
|--------------------|-----------------------------------------------------|----------------------------------------------------|
| Storage format     | JSONL files per session                             | SQLite (`~/.local/share/opencode/opencode.db`)     |
| Session path       | `~/.claude/projects/<enc-dir>/<uuid>.jsonl`         | `session` table, FK to `project` table             |
| Session ID format  | UUID (`fab61238-0f5d-41fa-b53b-a61a002f66d5`)       | `ses_` + 26-char base62 (`ses_3282094f…`)          |
| Project ID         | Directory name (SHA1-path encoded with `-`)         | SHA1 hash of worktree path                         |
| Session title      | `{"type":"summary","summary":"..."}` in JSONL       | `session.title` column                             |
| Message content    | JSONL lines with `type: user\|assistant`            | `message` + `part` tables; `part.data` JSON        |
| Rename history     | `~/.claude/history.jsonl` `/rename` entries         | `session.title` updated directly in DB             |
| Summaries          | `~/.claude/summaries/<session-id>.md` files         | No built-in; we will store at `~/.local/share/opencode/summaries/<id>.md` |
| Resume command     | `claude --resume <uuid>`                            | `opencode --session <ses_id>` (TBD — see §Resume) |
| Git branch         | `sessions-index.json gitBranch` field               | `session.summary_diffs` JSON / snapshot refs       |
| Hook mechanism     | `~/.claude/settings.json` `SessionEnd` hook         | OpenCode hooks (TBD — see §Hook)                   |
| Sub-sessions       | Sidechains (`isSidechain: true`)                    | `session.parent_id` non-null                       |
| Dependency         | None (flat files)                                   | `rusqlite` crate (SQLite bindings)                 |

---

## Goals

1. **Unified list** — a single `cc-speedy` TUI shows sessions from both tools, sorted by
   recency, with a source badge (`[CC]` / `[OC]`).
2. **OpenCode session reading** — query `opencode.db` directly using `rusqlite`; no parsing of
   intermediate files.
3. **OpenCode summaries** — on-demand generation via `claude --print` (same as Claude Code path),
   stored in `~/.local/share/opencode/summaries/<session-id>.md`.
4. **OpenCode tmux resume** — derive session name from project worktree path; run
   `opencode` in the correct working directory with the session pre-selected.
5. **Source filter** — `/cc` and `/oc` filter shortcuts to view only one source.
6. **No breaking changes** — all existing Claude Code behaviour is preserved.

---

## Architecture

### Module Changes

```
src/
├── main.rs          unchanged (dispatch only)
├── lib.rs           add pub mod opencode_sessions
├── sessions.rs      unchanged (Claude Code source)
├── opencode_sessions.rs   NEW — reads OpenCode SQLite
├── unified.rs       NEW — merges both sources into Vec<UnifiedSession>
├── summary.rs       extend: opencode summary path + generation
├── tmux.rs          extend: resume_opencode_in_tmux()
├── tui.rs           extend: source badge, /cc /oc filter shortcuts
└── install.rs       extend: opencode hook registration (future)
```

### New `UnifiedSession` type

Replace the current `Session` type in the TUI with a `UnifiedSession` that wraps either source:

```rust
pub enum SessionSource {
    ClaudeCode,
    OpenCode,
}

pub struct UnifiedSession {
    // shared fields (same semantics for both sources)
    pub session_id:     String,        // UUID for CC; ses_xxx for OC
    pub project_name:   String,        // last 2 path segments
    pub project_path:   String,        // absolute worktree path
    pub modified:       SystemTime,
    pub message_count:  usize,
    pub first_user_msg: String,        // first user text (≤80 chars)
    pub summary:        String,        // rename title / DB title
    pub git_branch:     String,
    pub source:         SessionSource, // CC or OC
    // source-specific handles (needed for resume / summary generation)
    pub jsonl_path:     Option<String>, // CC only
}
```

`UnifiedSession` replaces `Session` in the TUI. The existing `Session` struct in `sessions.rs` is
kept as-is; conversion `Session → UnifiedSession` is a thin `From` impl.

---

## New Module: `opencode_sessions.rs`

### Dependency

Add to `Cargo.toml`:
```toml
rusqlite = { version = "0.31", features = ["bundled"] }
```

The `bundled` feature statically links SQLite — no system library required.

### DB Path

```rust
pub fn opencode_db_path() -> Option<PathBuf> {
    dirs::data_local_dir()   // ~/.local/share
        .map(|d| d.join("opencode").join("opencode.db"))
}
```

`dirs::data_local_dir()` returns `~/.local/share` on Linux, `~/Library/Application Support` on
macOS — matches the XDG layout OpenCode uses on both platforms.

### Query

```sql
SELECT
    s.id,
    s.title,
    s.time_updated,
    p.worktree,
    COUNT(DISTINCT m.id)     AS message_count,
    s.summary_diffs,
    s.parent_id
FROM session s
JOIN project p ON p.id = s.project_id
LEFT JOIN message m ON m.session_id = s.id
WHERE s.time_archived IS NULL          -- exclude archived
  AND s.parent_id IS NULL              -- exclude sub-agent sessions
GROUP BY s.id
ORDER BY s.time_updated DESC;
```

**Why `parent_id IS NULL`**: sub-agent sessions (spawned by the `task` tool) are implementation
details of the parent session, not standalone conversations. They clutter the list exactly like
Claude Code sidechains (`isSidechain: true`). Both are filtered out.

**First user message**: retrieved in a second query per session (or a subquery) from the `part`
table:
```sql
SELECT p.data
FROM part p
JOIN message m ON m.id = p.message_id
WHERE m.session_id = ?
  AND p.data LIKE '{"type":"text"%'
ORDER BY p.time_created ASC
LIMIT 1;
```
Extract `data.text` from the JSON, truncate to 80 chars.

### `list_opencode_sessions()` return type

```rust
pub fn list_opencode_sessions() -> Result<Vec<UnifiedSession>>
```

Returns `Ok(vec![])` (not an error) if `opencode.db` does not exist — OpenCode may not be
installed on the machine.

---

## Summary Storage for OpenCode

Summaries for OpenCode sessions are stored in:
```
~/.local/share/opencode/summaries/<session-id>.md
```

Add an `opencode_summary_path(session_id: &str) -> PathBuf` function to `summary.rs`.  
`generate_summary()` already works with `Vec<Message>` and `claude --print`; it is reused as-is.

The TUI summary loading / generation logic in `tui.rs` uses the session's `source` to call the
right path function.

---

## Tmux Resume for OpenCode

### Determining the Resume Command

OpenCode's CLI flags are not yet documented for `--session`-style resume.  
**Approach**: inspect the installed `opencode` binary's help output at runtime:

```rust
fn opencode_resume_args(session_id: &str, project_path: &str) -> Vec<String> {
    // Try `opencode --help` to detect supported flags; fall back to just launching opencode
    // in the project directory (OpenCode will show the session browser on its own).
    // This is intentionally conservative — update when the flag is confirmed.
    vec!["--session".to_string(), session_id.to_string()]
}
```

For now, `resume_opencode_in_tmux()` in `tmux.rs`:
1. Derives session name from `project_path` (same `session_name_from_path()` logic, but prefixed
   with `oc-` to avoid colliding with Claude Code tmux sessions using the same project).
2. `cd`s to `project_path` and runs `opencode` (without explicit session flag if the flag is
   unsupported — OpenCode will default to the most recent session for that directory).

### Collision Avoidance

Claude Code and OpenCode sessions for the **same project** would previously collide on the tmux
session name (e.g. both generate `ai-cc-speedy`). Fix: prefix tmux session names with the source:
- Claude Code: `cc-<name>` (breaking change — existing tmux sessions will not be found under old names; document in changelog)
- OpenCode:    `oc-<name>`

---

## TUI Changes

### Source Badge in List

```
03-10 15:42  [OC] stellar-comet     42  ai/cc-speedy
03-10 14:10  [CC] fix auth bug       28  api/backend
```

`[OC]` rendered in cyan, `[CC]` in green.

### Source Filter Shortcuts

In addition to the existing `/` filter, two quick-filter keys:
- `1` — show Claude Code only
- `2` — show OpenCode only
- `0` — show all (clear source filter)

These complement (not replace) the text filter — both can be active simultaneously.

### Status Bar

Add the source-filter state to the status bar hint:
```
1:CC  2:OC  0:all  /: text filter  Enter: resume  ...
```

---

## Hook Integration (Future / Phase 2)

OpenCode does not currently expose a `SessionEnd`-style hook to external binaries.

**Plan**: when OpenCode adds lifecycle hooks, register via:
```
cc-speedy install-opencode
```
which would write to `~/.config/opencode/opencode.json`:
```json
{
  "hooks": {
    "session:end": [{ "command": "/path/to/cc-speedy summarize-oc" }]
  }
}
```

The `summarize-oc` subcommand would read `OPENCODE_SESSION_ID` from the environment
(analogous to `CLAUDE_SESSION_ID`), query `opencode.db` for the session's messages, generate
the summary, and write to the OpenCode summaries path.

For now, summaries are generated **on-demand** in the TUI (same as Claude Code's on-demand path),
which is fully functional without a hook.

---

## Data Flow (Updated)

```
Claude Code:
  ~/.claude/projects/**/*.jsonl  ──► sessions.rs::list_sessions()     ──┐
  ~/.claude/history.jsonl        ──► sessions.rs::read_rename_history()  ├──► unified.rs::list_all()
                                                                          │
OpenCode:                                                                 │
  ~/.local/share/opencode/       ──► opencode_sessions.rs::             ──┘
    opencode.db                        list_opencode_sessions()
                                                                          │
                                                                          ▼
                                                                    Vec<UnifiedSession>
                                                                          │
                                                                          ▼
                                                                     tui.rs::run()
                                                                          │
                                                           ┌──────────────┴────────────────┐
                                                           │                               │
                                               Enter (CC)  │              Enter (OC)       │
                                      claude --resume <id> │    opencode (in project dir)  │
                                                           │                               │
                                                         tmux                            tmux
                                                       cc-<name>                       oc-<name>

Summaries (both sources):
  claude --print <prompt>  ──►  ~/.claude/summaries/<cc-id>.md
                                ~/.local/share/opencode/summaries/<oc-id>.md
```

---

## Implementation Plan

### Phase 1 — OpenCode read-only (no resume, no summary hook)

| Step | File(s) | Description |
|------|---------|-------------|
| 1 | `Cargo.toml` | Add `rusqlite = { version = "0.31", features = ["bundled"] }` |
| 2 | `src/unified.rs` | Define `SessionSource`, `UnifiedSession`; `From<Session>` impl |
| 3 | `src/lib.rs` | Add `pub mod opencode_sessions; pub mod unified;` |
| 4 | `src/opencode_sessions.rs` | `opencode_db_path()`, `list_opencode_sessions()` |
| 5 | `src/summary.rs` | Add `opencode_summary_path()` |
| 6 | `src/tui.rs` | Migrate to `UnifiedSession`; add `[OC]`/`[CC]` badges; add `1`/`2`/`0` source filter |
| 7 | Tests | `tests/opencode_sessions_test.rs` with an in-memory SQLite fixture |

### Phase 2 — OpenCode resume

| Step | File(s) | Description |
|------|---------|-------------|
| 8 | `src/tmux.rs` | `resume_opencode_in_tmux()` with `oc-` prefix naming |
| 9 | `src/tui.rs` | Wire `Enter` to the right resume function based on `source` |
| 10 | `src/tmux.rs` | Rename CC tmux sessions to `cc-<name>` prefix (with changelog note) |

### Phase 3 — OpenCode hook (when OpenCode exposes it)

| Step | File(s) | Description |
|------|---------|-------------|
| 11 | `src/main.rs` | Add `summarize-oc` subcommand dispatch |
| 12 | `src/summary.rs` | `run_opencode_hook()` reading `OPENCODE_SESSION_ID` |
| 13 | `src/install.rs` | `install_opencode_hook()` writing to `opencode.json` |
| 14 | `src/main.rs` | Add `install-opencode` subcommand dispatch |

---

## Key Design Decisions

### Why rusqlite (not raw file parsing)?

OpenCode's SQLite schema is stable and versioned via Drizzle migrations. Querying it with
`rusqlite` is simpler, faster, and more correct than parsing WAL-flushed binary files. The
`bundled` feature avoids any system library dependency, keeping the "zero runtime deps" property.

### Why separate `UnifiedSession` instead of extending `Session`?

The two session types have genuinely different identity (UUID vs `ses_xxx`), different path to
messages (JSONL file vs DB query), and different resume mechanics. A unified struct with an enum
discriminant is cleaner than an optional `Option<SqliteRow>` bolted onto `Session`.

### Why `oc-` / `cc-` tmux prefix?

Without prefixes, a Claude Code session and an OpenCode session in the same project directory
generate the same tmux session name, causing silent attach-to-wrong-session bugs. The prefix cost
is one rename for existing users' tmux sessions — acceptable given the functional correctness
gain. It is documented as a breaking change in the changelog.

### Why on-demand summaries only (no hook) for now?

On-demand generation in the TUI is immediately functional and requires zero changes to OpenCode
itself. It is the same model already validated for Claude Code. The hook path adds value
(summaries generated even if the TUI is not open) but depends on OpenCode implementing the
feature — deferred to Phase 3.

### Why filter with `1`/`2`/`0` instead of a toggle?

Toggling between CC/OC/all would require tracking state and pressing a key multiple times to
cycle. Dedicated `1`/`2`/`0` keys are O(1) and self-documenting in the status bar.
