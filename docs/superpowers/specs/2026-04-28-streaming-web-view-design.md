# Streaming Localhost Web View — Design

**Date:** 2026-04-28
**Status:** Draft, awaiting user review

## Problem

The TUI's per-turn detail modal (`Enter` on the insights timeline) is the richest view of a conversation, but it lives inside a terminal: monospaced font, limited width, no copy-paste of multi-line code blocks with formatting, no syntax highlighting, no scrollback in the way a browser provides. There's also no way to follow a still-running session in real time — the user has to manually re-open the modal to see new turns.

## Goal

Spawn a local HTTP server inside cc-speedy that exposes a browser-based dashboard and per-session conversation view, with live updates streamed via SSE for sessions that are currently `Live` (per the in-progress indicator's classification). Bind to `127.0.0.1` only.

The web view is a **read-only** companion to the TUI — it does not mutate sessions, summaries, learnings, or any other DB state.

## Non-goals

- Authentication, multi-user access, or remote (LAN/WAN) access. Localhost-only.
- Editing, archiving, tagging, linking, or any other state-changing actions from the browser.
- Mobile-first design. Desktop browser is the target. Mobile-friendly CSS comes for free with sensible defaults but is not tested.
- Markdown / code rendering with full IDE features (LSP, syntax-aware folding). We render plain `<pre>` for code blocks and `<details>` for tool results, with CSS-only basic prettification.
- A persistent server (always-on across cc-speedy sessions). Server lives in the cc-speedy process; quitting the TUI kills the server.

## Dependencies

This feature depends on the in-progress indicator (#2) for liveness detection. Specifically, it reuses:
- `liveness::detect(session) -> Liveness` for the dashboard list.
- The existing `liveness_cache` for read-only state lookup.
- Adds a per-session "tailer" abstraction that produces turn-added events (described below).

## User-facing behavior

### Lifecycle

- Server is **off by default** when cc-speedy launches.
- Pressing `W` in the TUI toggles the server on or off.
- When the server starts:
  - It binds `127.0.0.1:7457`. If 7457 is taken, falls back to OS-assigned (port 0) and displays the actual port.
  - Status line shows: `web: http://127.0.0.1:<port> (W: stop, o: open, y: yank)`.
- Pressing `W` again stops the server, drops all in-flight SSE connections, and clears the status line slot.
- On TUI quit, the server is shut down gracefully (drop sender → tasks unwind).

### TUI helpers (only active while server is running)

- `o` — opens the dashboard URL in the default browser (`xdg-open` on Linux, `open` on macOS, `start` on Windows). Best-effort; if the helper isn't available, shows a status-line error.
- `y` — copies the dashboard URL to system clipboard via the existing `arboard` dep (or shells out to `wl-copy` / `xclip` / `pbcopy` if not present — investigate during implementation).

These two keys are no-ops when the server is off.

### Routes

| Route | Method | Returns |
|-------|--------|---------|
| `/` | GET | HTML — dashboard listing all sessions, grouped by source, sorted by recency, liveness glyph next to each |
| `/session/:id` | GET | HTML — full per-turn conversation view for one session |
| `/session/:id/stream` | GET | text/event-stream — SSE stream of `turn-added` events while the session is live |
| `/session/:id/turn/:idx` | GET | HTML — single-turn detail, same data as the TUI modal |
| `/api/sessions` | GET | JSON — array of session summary objects |
| `/api/session/:id/turns/:idx` | GET | JSON — single `TurnDetail` payload |
| `/static/app.css` | GET | CSS — embedded at build time |
| `/static/app.js` | GET | JS — embedded at build time |
| any other path | GET | 404 |

All routes are GET-only. There are no POST / PUT / DELETE endpoints. CSRF is therefore a non-concern.

### Dashboard page (`/`)

- Three columns (or stacked sections, responsive): Claude Code, OpenCode, Copilot.
- Each row: liveness glyph (`▶` / `◦` / `·`), source badge, project path, age, first-message preview, link to `/session/:id`.
- Refresh-on-demand: a top-right "↻" button reloads the dashboard via `fetch('/api/sessions')` + DOM diff. No auto-poll on the dashboard for v1.
- Live sessions are visually emphasized (subtle pulsing dot or background tint).

### Session page (`/session/:id`)

- Title bar: source badge, project, session id, liveness state with timestamp.
- Body: vertically stacked turns. Each turn renders user message + assistant response + tool-use blocks + tool-result blocks (collapsed `<details>` for tool results to keep the page scannable).
- If the session is `Live`, the page opens an SSE connection to `/session/:id/stream` and appends `turn-added` events to the bottom of the page in real time. If the session is `Recent` or `Idle`, no SSE connection is opened.
- A small "▶ live" badge in the title bar reflects the SSE connection status. Reconnects automatically on transient drops.
- Token totals and tool histogram (same data as the insights panel) shown in a sticky right-side or top sidebar.

### SSE event format

```
event: turn-added
data: {"idx": 42, "url": "/api/session/abc-123/turns/42"}

event: turn-updated
data: {"idx": 42, "url": "/api/session/abc-123/turns/42"}

event: liveness
data: {"state": "recent"}
```

The browser fetches the JSON for each new turn via the URL in the event payload and renders. The minimal SSE payload size keeps the wire-format trivial; rendering logic lives in `app.js`.

`turn-updated` fires when the trailing turn (which was previously open) gains its tool_result and becomes "complete." `liveness` fires whenever the session's classification changes (e.g., goes from `Live` to `Recent`).

## Architecture

### New modules

```
src/
├── web/
│   ├── mod.rs          // server lifecycle, axum router, shutdown
│   ├── handlers.rs     // route handlers
│   ├── tailer.rs       // per-session shared tailer + broadcast
│   └── assets.rs       // include_str! / include_bytes! for static assets
└── web/
    └── static/
        ├── app.css
        ├── app.js
        └── index.html  // template scaffolding (server inserts payload as JSON inline)
```

### Lifecycle

`AppState` gains:

```rust
web_handle: Option<WebServerHandle>,
```

`WebServerHandle` holds the `tokio::task::JoinHandle` for the running server, the bound address, and a `oneshot::Sender<()>` for graceful shutdown.

Toggling `W`:

```rust
match self.web_handle.take() {
    Some(handle) => {
        handle.shutdown();   // sends on the oneshot
        // status line clears in the next frame
    }
    None => {
        let handle = web::start(self.shared_state())?;
        self.web_handle = Some(handle);
    }
}
```

`web::start` boots an `axum::serve(...)` future, returns the handle. `shared_state()` gives the web layer a `WebState` struct with `Arc` clones of the session list, liveness cache, and tailer registry — never `&mut AppState`, so the TUI thread never blocks on the server.

### Per-session shared tailer

When the first SSE client connects to `/session/:id/stream`, we lazily start a tailer task for that session:

```rust
struct Tailer {
    broadcast: tokio::sync::broadcast::Sender<TailEvent>,
    refcount: Arc<AtomicU32>,
    handle: tokio::task::JoinHandle<()>,
}

enum TailEvent {
    TurnAdded { idx: u32 },
    TurnUpdated { idx: u32 },
    LivenessChanged(Liveness),
}
```

Tailer logic:
1. Open the JSONL file.
2. Stat the file periodically (1s tick) — when size grows, read the new bytes from where we left off, parse complete lines, decide whether each new line opens or closes a turn, emit `TurnAdded` or `TurnUpdated`.
3. Also re-evaluate `liveness::detect` after each batch; if the state changes, emit `LivenessChanged`.
4. When refcount drops to 0 (last subscriber disconnected), the tailer task exits and de-registers itself.

Multiple browser tabs subscribing to the same session share one tailer via the broadcast channel — no per-tab file polling.

A `TailerRegistry: HashMap<SessionId, Tailer>` lives in `WebState`, behind a single `tokio::sync::Mutex`.

### Static assets

CSS and JS are embedded at build time with `include_str!`. No filesystem reads at runtime. ~20 KB total, served with `Cache-Control: no-cache` for now (no versioning needed since they ship with the binary).

### HTML rendering

We use a tiny template approach — `format!`-based string interpolation inside handlers. No template engine dependency. The dashboard and session pages are thin shells; most rendering is driven by JS fetches against `/api/...` for clean separation between static markup and dynamic data.

### JSON serialization

Existing structs that are emitted to the browser:
- `UnifiedSession` (already `Serialize`d via serde, used by Obsidian export).
- `TurnDetail` (currently used internally; needs to be `#[derive(Serialize)]` if not already).
- `SessionInsights` (same).
- A new `WireSession` summary (subset of `UnifiedSession` + current liveness state) for the dashboard payload.

### Shutdown semantics

- `W` again, or process exit: shutdown signal goes to the axum server. In-flight SSE responses get a final `event: bye\ndata: {}\n\n` then the connection closes.
- All running tailers receive a broadcast close; tasks unwind.
- `web_handle` is dropped, all `Arc`s decremented.

## Security & privacy

- **Bind address:** `127.0.0.1` is hardcoded. There is no setting to bind to `0.0.0.0` in v1. (Future setting `web_bind` could expose this with a clear warning, but not yet.)
- **No auth:** the server trusts anyone on `127.0.0.1`. On a single-user machine this is fine; on a shared host (jump server) any user could read another user's sessions. Documented in the spec; no v1 mitigation.
- **Conversation contents:** sessions can contain API keys, secrets, file contents the user pasted, etc. The web view exposes the same data the TUI does. We do not redact. The `W` opt-in lifecycle plus `127.0.0.1` are the privacy boundary.
- **CSRF:** no mutating endpoints, so non-issue.
- **Path traversal:** session IDs are validated against the in-memory session list (must match an existing `UnifiedSession.id`) before file paths are resolved. Untrusted input never reaches `Path::new`.

## Cargo.toml additions

```toml
axum = "0.8"
tokio-stream = "0.1"  # for SSE response stream
arboard = "3"         # only if not already present (for `y` clipboard yank)
```

(`tower-http` only added if we need its `ServeDir` / compression; for v1 we don't.)

## Testing

New suite at `tests/web_test.rs`. Each test boots the server on a random port and uses `reqwest` (already a dev-dep, or add) to drive it.

| Test | Setup | Assertion |
|------|-------|-----------|
| `boots_on_random_port_when_default_taken` | Pre-bind 7457, then `web::start` | Server bound to a different port; reported in handle |
| `dashboard_renders_html` | Fixture sessions list | `GET /` returns 200, `text/html`, contains expected session ids |
| `api_sessions_returns_json` | Fixture sessions list | `GET /api/sessions` returns 200, valid JSON, all sources represented |
| `session_page_404_for_unknown_id` | | `GET /session/nonexistent` returns 404 |
| `sse_stream_emits_turn_added_when_jsonl_grows` | Build live session, subscribe to `/session/:id/stream`, append a turn | First event is `turn-added` with `idx=N`, observed within 2s |
| `sse_stream_closes_on_shutdown` | Subscribe, send shutdown | Connection closes cleanly, no panic |
| `tailer_shared_between_subscribers` | Two SSE subscribers to same session, append a turn | Both receive the event from a single tailer (verify by counting tailer tasks via test hook) |
| `tailer_stops_when_refcount_zero` | Subscribe + disconnect, wait | Tailer task exits, registry no longer contains the session id |
| `path_traversal_rejected` | `GET /session/../../etc/passwd` | 404, no file read |
| `bind_is_localhost_only` | `web::start` | Bound address is `127.0.0.1`, not `0.0.0.0` |

## Risks

- **axum dep weight:** axum + its tower-tower-http transitive tree adds ~30 crates and ~10s to a clean build. Not catastrophic for cc-speedy, but biggest single dep we'd add. Mitigation: gate behind a Cargo feature (`web`) so users who don't want it can build without. Default-on so the user-facing behavior matches docs.
- **SSE flakiness through proxies:** not relevant on localhost.
- **Tailer parser drift:** the per-source "turn open/close" logic is the same problem #2 has. We share `liveness::detect` for state, and the new tailer code adds the *event-emitting* layer. If a source changes its log shape, the tailer may stop emitting `TurnAdded` correctly. Tests pin schemas; integration tests use representative fixtures.
- **Browser-side state on long sessions:** appending DOM nodes for hundreds of turns can degrade performance. Mitigation: virtualize / lazy-render later. For v1, document the limitation; cap initial render at the most recent 50 turns with a "load older" link.
- **Clipboard yank portability:** `arboard` works on most desktop Linux, macOS, Windows. WSL2 needs `wl-copy` / `clip.exe` fallback — investigate during implementation.

## Out-of-scope follow-ups

- Configurable bind address / port via Settings.
- Auth (token in URL, basic auth, etc.) for non-localhost binds.
- Push API for inviting other devices over LAN.
- Markdown rendering / syntax highlighting.
- Inline diff rendering for tool_use `edit_file` calls.
- Mobile-friendly layout pass.
- Persistent server across cc-speedy launches (systemd unit, launchd, etc.).
- WebSocket upgrade for browser→server interactions (e.g., archive-from-browser).
