# cc-speedy — Live View Release

A focused release that makes the session list feel live: refresh without
restarting, see which agents are still running at a glance, and open a
browser companion that streams an in-progress conversation as it happens.
Plus a perf overhaul so refresh stays snappy on 400+ session corpora.

## ✨ New Features

### 🔄 Refresh — press `R` or `F5`
Re-scan all session sources without exiting and re-launching the TUI. New
sessions surface, updated sessions float to the top, deleted sessions
disappear; the user's current selection is preserved when the target
still exists, falls back to row 0 otherwise. Toast at the bottom reports
`Refreshed: N sessions (+K new, M updated)` or `(no changes)`.

- A `↻` indicator appears in the status hint while a scan is in flight.
- Mashing `R` does **not** stack scans: a global atomic in-flight guard
  is a no-op on re-entry, with an RAII drop guard so a panicking scan
  task can't leave the flag stuck-set.
- Failures surface explicitly: `Refresh failed: <error>` instead of
  silently swallowing I/O errors.
- Both the active list and the archived list have their selection
  restored independently, regardless of focus.
- Startup now uses the same async path — the TUI is interactive
  immediately while the first scan completes in the background; the
  list shows `Loading sessions…` until results land.

Bound in `Normal`, `Library`, and `Projects` modes.

### ▶ In-progress indicator — tri-state liveness glyph
Each row in the session list shows whether its agent is currently
running:

| Glyph | Color | Meaning |
|-------|-------|---------|
| `▶`   | green | **Live** — log was written within the last 5 s and the tail shows an unmatched `tool_use` / open assistant turn |
| `◦`   | cyan  | **Recent** — modified within the last 5 minutes but no open turn |
| `·`   | dim   | **Idle** — anything older |

Live detection requires more than a fresh `mtime` — the tail of the
jsonl/event log must show an actually-open turn, otherwise idle agents
that just had their disk caches synced wouldn't decay. CC parses
unmatched `tool_use` blocks; Copilot parses open assistant turns.

A 5 s polling task re-checks visible sessions only (selected row ± 25
rows on each list). Scrolling to a previously-unpolled row triggers a
one-shot detect, so glyphs appear within ~1 s of scrolling instead of
waiting for the next 5 s tick.

### 🌐 Streaming web companion — press `W`
Toggle a local read-only web server bound to `127.0.0.1:7457` (with
OS-assigned port fallback). Status bar shows `· web: http://127.0.0.1:7457`
while running. The dashboard groups sessions by source (CC / OC /
Copilot); each row is a clickable link to a session detail page that
loads turns top-to-bottom and streams new ones live via Server-Sent
Events while the underlying agent is `live`.

- `Ctrl+B` opens the URL in your default system browser
  (`xdg-open` / `open` / `cmd start`).
- `y` yanks the URL to the clipboard via `arboard`.
- The session detail page supports the same per-turn rendering as the
  TUI's turn-detail modal: text, collapsible thinking, tool-use
  invocations, collapsible tool results.
- SSE events: `turn-added`, `turn-updated`, `liveness`. The browser
  closes the stream automatically when the session goes idle.
- Path traversal protected at the route handler — unknown session ids
  are rejected before any `Path::new` resolution.
- All static assets (HTML, CSS, JS) are embedded at compile time via
  `include_str!`. No build step, no asset shipping.

### ⚡ Incremental refresh — skip what hasn't changed
Refresh on a 400+ session corpus used to be dominated by per-session
metadata parsing: `parse_messages` on Claude Code jsonls,
`query_first_user_text` per OpenCode row, `count_messages_and_first`
reading the entire `events.jsonl` per Copilot session.

The new `list_all_sessions_incremental(prior)` path consults the prior
session list before each expensive parse. When the cheap mtime signal
matches — `file_mtime` from CC's index, `time_updated` from OC, the
`updated_at` field in Copilot's `workspace.yaml` — the prior
`UnifiedSession` is cloned as-is and the parse is skipped.

Initial load still does a full scan. From there, refresh is a stat-only
walk in the common case. Tested with sentinel data: a Copilot session
whose `events.jsonl` is replaced with too-few-message garbage still
surfaces when prior matches (proving the events file was never read).

## 🐛 Fixes

- **`W` keybind matches Shift modifier.** Capital-letter keybinds in
  this codebase use `_` for the modifier so either `SHIFT` or `NONE`
  matches. The new `W` arm shipped initially as `NONE`-only and silently
  failed on terminals reporting Shift+W as `SHIFT`.

## 📋 Updated Keymap

| Key | Action |
|-----|--------|
| `R` / `F5` | Refresh — re-scan all session sources |
| `W` | Toggle local web companion server |
| `Ctrl+B` | Open web URL in default browser (when web is running) |
| `y` | Yank web URL to clipboard (when web is running) |
| `[` / `]` | Glyph timeline navigation in Insights panel |

(All other bindings unchanged.)

## 🔗 Web routes

| Path | Returns |
|------|---------|
| `GET /` | Dashboard HTML shell |
| `GET /session/{id}` | Session detail HTML shell |
| `GET /session/{id}/stream` | Server-Sent Events: `turn-added`, `turn-updated`, `liveness` |
| `GET /api/sessions` | JSON: projected session list with liveness state |
| `GET /api/session/{id}/turns/{idx}` | JSON: full content of one assistant turn |
| `GET /static/app.{css,js}` | Embedded static assets |
| `GET /health` | `ok` |

## 📦 New dependencies

- `axum = "0.8"` — web server
- `tokio-stream = "0.1"` (with `sync` feature) — `BroadcastStream` for SSE
- `arboard = "3"` — clipboard access for `y` keybind
- `futures = "0.3"` — `Stream` trait for SSE plumbing
- `reqwest` — adds the `stream` feature for SSE tests

## 📈 Stats

- **31 test suites**, **315 tests passing** (up from 21 / ~250).
- **+3,449 LOC** across 23 files.
- **4 design specs** committed under `docs/superpowers/specs/`:
  refresh, in-progress indicator, streaming web view, plus the
  ancillary plans.

## 📦 Install / upgrade

```bash
cargo install --path .
# or
cargo build --release && sudo install -m 755 target/release/cc-speedy /usr/local/bin/cc-speedy
```

The web companion is always-on (no feature flag) — pressing `W` is the
only thing that starts the server. No new SQLite migrations; the data
layer is unchanged.

## 🙏 Credits

Designed and implemented in collaboration with Claude Code (Opus 4.7),
each feature gated through brainstorming → spec → plan → subagent-driven
execution. Per-feature design specs in `docs/superpowers/specs/`
document behavior, tradeoffs, and non-goals.

---

# Previous release — Productivity Roadmap

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

## 🗄️ Data Model

Two additive, idempotent SQLite migrations run automatically on first launch:

```sql
CREATE TABLE IF NOT EXISTS tags  (session_id TEXT, tag TEXT, PRIMARY KEY (session_id, tag));
CREATE TABLE IF NOT EXISTS links (session_id TEXT PRIMARY KEY, parent_session_id TEXT, linked_at INTEGER);
```

No existing tables are modified. No data migration required.
