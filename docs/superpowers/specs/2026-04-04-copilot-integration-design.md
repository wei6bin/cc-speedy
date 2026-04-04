# Copilot CLI Session Integration — Design Spec

**Date:** 2026-04-04  
**Status:** Approved

## Overview

Add GitHub Copilot CLI (`copilot`) as a third session source in cc-speedy, alongside Claude Code (CC) and OpenCode (OC). Users will be able to browse, filter, summarize, pin, and resume Copilot sessions from the same TUI.

---

## Session Storage Format

Only the **new directory-based format** is supported (sessions created by copilot CLI v1.x+).

```
~/.copilot/session-state/
  <uuid>/
    workspace.yaml      ← primary metadata
    events.jsonl        ← conversation events
    checkpoints/
    files/
    research/
```

Sessions lacking `workspace.yaml` (legacy VS Code–originated sessions) are silently skipped.

### workspace.yaml fields used

| Field | Use |
|---|---|
| `id` | `session_id` |
| `cwd` | `project_path`; last 2 segments → `project_name` |
| `name` | Display title (priority 1) |
| `summary` | Display title (priority 2, fallback to `name`) |
| `updated_at` | `modified` timestamp |
| `branch` | `git_branch` |

`workspace.yaml` is flat key:value YAML — parsed line-by-line with a `key: value` splitter (no `serde_yaml` dependency).

### events.jsonl message types used

| Type | Role mapping | Field |
|---|---|---|
| `user.message` | `"user"` | `data.content` |
| `assistant.message` | `"assistant"` | `data.content` (skip if empty) |

Sessions with fewer than 4 messages are filtered out (consistent with CC threshold).

---

## New Module: `src/copilot_sessions.rs`

```
list_copilot_sessions() -> Result<Vec<UnifiedSession>>
  - Scan ~/.copilot/session-state/ for directories
  - Skip dirs without workspace.yaml
  - Parse workspace.yaml (line-by-line key:value)
  - Read events.jsonl: count messages, extract first user message text
  - Filter sessions with < 4 messages
  - Return UnifiedSession { source: SessionSource::Copilot, jsonl_path: None, ... }

parse_copilot_messages(session_id: &str) -> Result<Vec<Message>>
  - Read ~/.copilot/session-state/<session_id>/events.jsonl
  - Extract user.message and assistant.message entries into Vec<Message>
  - Used by TUI summary generation task

parse_workspace_yaml(path: &Path) -> Option<WorkspaceYaml>   [private]
  - Line-by-line: split on first ':' to get key/value pairs
  - Returns struct with id, cwd, summary, name, updated_at, branch
```

Returns `Ok(vec![])` if `~/.copilot/session-state/` does not exist (Copilot not installed).

---

## Modified Files

### `src/unified.rs`
- Add `SessionSource::Copilot` variant to enum
- `list_all_sessions()` merges CC + OC + Copilot, sorted by recency

### `src/tmux.rs`

| Function | Command |
|---|---|
| `copilot_session_name(path)` | `"co-<last-2-path-segments>"`, max 50 chars |
| `new_copilot_session_name(path)` | `"co-new-<base>-<ts>"` |
| `resume_copilot_in_tmux(name, path, id, yolo, title)` | `copilot --resume=<id>` or `copilot --allow-all --resume=<id>` |
| `new_copilot_in_tmux(name, path, title)` | `copilot` |

Yolo maps to `--allow-all` (Copilot's native equivalent of `--dangerously-skip-permissions`).

### `src/theme.rs`
- Add `CO_BADGE: Color = Color::Rgb(255, 140, 0)` — orange, distinct from CC green (`#0d8300`) and OC blue (`#1e90ff`)

### `src/tui.rs`

| Area | Change |
|---|---|
| Source filter key `'3'` | Filter to Copilot sessions only |
| Badge | `[CO]` in `theme::CO_BADGE` (orange) |
| Resume (Enter) | `resume_copilot_in_tmux(..., yolo: false)` |
| Yolo (Ctrl+Y) | `resume_copilot_in_tmux(..., yolo: true)` |
| New session (n / Ctrl+N) | `new_copilot_in_tmux(...)` |
| Summary generation | `copilot_sessions::parse_copilot_messages(id)`, source string `"co"` |

### `src/store.rs`
- Source string `"co"` used when saving Copilot summaries (joins `"cc"` and `"oc"`)

### `src/lib.rs`
- Add `pub mod copilot_sessions;`

---

## Tests: `tests/copilot_sessions_test.rs`

- `workspace.yaml` parsing: valid full fields, missing optional fields, malformed lines
- `events.jsonl` message extraction: correct role mapping, empty assistant content skipped
- Message count filter: sessions with < 4 messages excluded
- `list_copilot_sessions()` with a `tempfile`-based directory: detects sessions, skips legacy dirs

---

## Key Bindings Update

| Key | Action |
|---|---|
| `0` | Show all sources |
| `1` | CC only |
| `2` | OC only |
| `3` | CO (Copilot) only |
