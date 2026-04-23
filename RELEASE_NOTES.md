# cc-speedy — Productivity Roadmap Release

A multi-feature release that turns cc-speedy from a session list + resume tool into an active productivity surface. Seven new cross-session capabilities, plus a critical fix for environments where `ANTHROPIC_*` env vars leak in from wrapper shells.

## ✨ New Features

### 🔍 Cross-session grep — press `?`
Case-insensitive substring search across every session's **title, project path, git branch, summary body, and learning points**. Narrows the main list in place; preview pane highlights matches and auto-scrolls to the first hit. Composes with the source filter and archive tab.

### 📊 Live git status column — glyphs + `g` refresh
Each row shows the current git state of its project:

| Glyph | Meaning |
|-------|---------|
| `●` red | Dirty (uncommitted changes) |
| `○` green | Clean |
| `·` dim | Not a git repo |
| `◦` yellow | Check timed out / errored |

Startup batch populates every unique project path in parallel (500ms timeout each); 30s auto-refresh on selection change; `g` force-refreshes all entries. Preview pane adds live branch with `(dirty)` and `(ran on <original branch>)` annotations when the current branch differs from the session's historical branch.

### 📚 Learning Library — press `L`
Full-screen cross-session view of every learning point cc-speedy has captured: decision points, lessons & gotchas, and tools/commands discovered. Filter by category (`0`/`1`/`2`/`3`), live-search with `/`, `Enter` to jump back to the source session. Turns passively-collected knowledge into an active reference.

### 📁 Project Dashboard — press `P`
Full-screen list of unique projects with per-project stats: git glyph, live branch, session count, last-active date, pinned count. `s` cycles sort (last-active / session-count / alphabetical). `Enter` drills the main list into that project via a new `project_filter`. `Esc` in Normal mode clears an active project filter.

### 🏷️ Tags — press `t`, filter with `#tag`
Free-form comma-separated tags per session. `t` opens a top-bar editor; `Enter` saves, `Esc` cancels. Tags are normalized (trimmed, lowercased, `[a-z0-9-_]` only) and deduplicated. The filter bar now accepts `#tag` tokens AND-composed with text tokens:
- `/#blocked` → sessions tagged `blocked`
- `/#blocked auth` → tagged `blocked` AND title contains `auth`
- `/foo #wip` → title contains `foo` AND tagged `wip`

Tags appear in the preview pane as `TAGS: wip, blocked, needs-review`.

### 🔗 Session linking — press `l`, unlink with `u`
Link sessions that span multiple work periods into explicit parent/child chains. `l` opens a full-screen picker of candidate parent sessions (recency-sorted, filterable with `/`). Preview pane shows `PARENT:` and `CHILDREN: (N)` blocks so chains are visible at a glance.

### 📝 Weekly Digest — press `D`
Pure-aggregation 7-day view: session count, project count, learning count, per-project breakdown with session titles, and all learning points captured in the window. No LLM — instant open, no API failure surface. Press `e` to export to `<vault>/cc-speedy/digests/YYYY-Www.md`.

### 🛠️ Actions menu (replaces pin popup) — press `x`
The `x` popup now offers five actions on the selected session:
- `p` — pin / unpin (existing)
- `n` — new session in the selected folder, same agent
- `N` — same, in yolo mode
- `s` — **new session with prior summary pre-pasted as context** (via tmux bracketed-paste)
- `S` — same, in yolo mode

The top-level `n` and `Ctrl+N` bindings have been removed — their behavior lives only in the menu now.

## 🐛 Fixes

- **Strip `ANTHROPIC_*` env vars globally.** When cc-speedy is launched from a wrapper that exports `ANTHROPIC_BASE_URL` / `AUTH_TOKEN` / `MODEL` / `API_KEY` (e.g. openclaw's local proxy), spawned children previously inherited those vars and routed through the proxy — timing out or misbehaving. cc-speedy now unsets them at startup in `main()`, so every child process (tmux, `claude --print`, the agents themselves) inherits a clean environment and uses the user's default Claude subscription.
- **Bump `claude --print` timeout from 60s to 180s.** Manual Ctrl+R on large sessions legitimately runs close to the prior limit.
- **Grep mode keybindings.** `Enter`, `Tab`, `Ctrl+Y`, `Ctrl+R` now fire correctly while grep mode is active, without requiring an `Esc` first.

## 📋 Updated Keymap

| Key | Action |
|-----|--------|
| `/` | Filter (supports `#tag` tokens) |
| `?` | Cross-session grep |
| `L` | Learning library |
| `P` | Project dashboard |
| `D` | Weekly digest |
| `t` | Edit tags |
| `l` / `u` | Link parent / unlink |
| `g` | Refresh git status |
| `x` | Actions menu (pin / new / new-with-summary) |
| `a` | Archive |
| `r` | Rename |
| `c` | Copy summary |
| `Enter` | Resume |
| `Ctrl+Y` | Resume in yolo mode |
| `Ctrl+R` | Regenerate summary |
| `s` | Settings |
| `1`/`2`/`3`/`0` | Source filter: CC / OC / Copilot / all |
| `Tab` | Toggle focus |
| `q` | Quit |

## 🗄️ Data Model

Two additive, idempotent SQLite migrations run automatically on first launch:

```sql
CREATE TABLE IF NOT EXISTS tags  (session_id TEXT, tag TEXT, PRIMARY KEY (session_id, tag));
CREATE TABLE IF NOT EXISTS links (session_id TEXT PRIMARY KEY, parent_session_id TEXT, linked_at INTEGER);
```

No existing tables are modified. No data migration required.

## 📈 Stats

- **21 test suites** (up from 14), 100+ passing tests.
- **~2,000 LOC** added across 7 feature modules.
- **9 design specs** committed under `docs/superpowers/specs/`.

## 📦 Install

```bash
cargo install --path .                      # from source
# or
cargo build --release && sudo install -m 755 target/release/cc-speedy /usr/local/bin/cc-speedy
```

Run `cc-speedy install` once to register the SessionEnd hook that auto-summarizes every ended session.

## 🙏 Credits

Designed and implemented in collaboration with Claude Code (Opus 4.7). Each of the seven features ships with a per-feature design spec in `docs/superpowers/specs/` documenting behavior, data model, tradeoffs, and non-goals.
