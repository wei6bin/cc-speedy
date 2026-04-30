# Streaming Localhost Web View Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Spawn a local `axum` HTTP server on `127.0.0.1:7457` that serves a browser-based dashboard and per-session turn-by-turn view, with live SSE updates for active sessions, toggled by `W` in the TUI.

**Architecture:** A new `src/web/` module tree (`mod.rs` lifecycle, `handlers.rs` routes, `tailer.rs` per-session shared file watcher with broadcast, `assets.rs` for embedded HTML/CSS/JS). `AppState` gains a `web_handle: Option<WebServerHandle>`. The web layer holds `Arc` clones of read-only TUI state (sessions, liveness cache, tailer registry), so the TUI thread is never blocked by web traffic. Browser-side rendering is vanilla JS that fetches JSON from `/api/...` and listens to `/session/:id/stream` for live turn-added events.

**Tech Stack:** Rust 2021, `axum 0.8` (new), `tokio-stream 0.1` (new for SSE), `arboard 3` (new for clipboard), `tokio` (already), `serde_json` (already), `reqwest` (already as dep, used in tests). Frontend: server-rendered HTML + a single `app.js` (~250 lines vanilla JS, no build step) + `app.css`.

**Spec:** `docs/superpowers/specs/2026-04-28-streaming-web-view-design.md`

**Branch baseline:** This plan assumes the **in-progress indicator** feature (`feat/in-progress-indicator` branch — `src/liveness.rs` plus `liveness_cache: Arc<Mutex<HashMap<String, CachedLiveness>>>` on `AppState`) is in scope and accessible. The implementer should branch from `feat/in-progress-indicator`, not from `master`. If the indicator has been merged to `master` by the time this is executed, branch from `master`.

**Spec reconciliation (overrides):**
- The spec said `UnifiedSession` is "already `Serialize`d via serde, used by Obsidian export." This is incorrect — the Obsidian module hand-formats Markdown, not JSON. We instead project `UnifiedSession` into a small `WireSession` struct that owns `Serialize` and the dashboard wire format. This keeps the boundary explicit and avoids forcing serde derives onto the core domain types.
- The spec said "the existing `arboard` dep." `arboard` is not a current dependency; this plan adds it.
- `Tailer.handle: tokio::task::JoinHandle<()>` from the spec is internal to the `Tailer` module; the plan keeps it but does not expose it as a public field.
- The `LIVENESS_LIVE_RGB`, `LIVENESS_RECENT_RGB` colour values in this plan match the indicator feature's `liveness_span` choices (RGB 0xa6/0xe3/0xa1 for live, 0x89/0xdc/0xeb for recent).

---

## File Structure

| File | Role | Action |
|------|------|--------|
| `Cargo.toml` | Add axum, tokio-stream, arboard. | **Modify** |
| `src/lib.rs` | Add `pub mod web;`. | **Modify** |
| `src/web/mod.rs` | Server lifecycle: `start(WebState) -> WebServerHandle`, `WebServerHandle::shutdown()`, `WebState`, port-fallback logic. | **Create** |
| `src/web/handlers.rs` | Route handlers for `/`, `/session/:id`, `/api/sessions`, `/api/session/:id/turns/:idx`, `/static/...`, SSE. | **Create** |
| `src/web/tailer.rs` | `Tailer`, `TailerRegistry`, `TailEvent`, per-session shared file watcher with `tokio::sync::broadcast`. | **Create** |
| `src/web/assets.rs` | `include_str!` for `app.css` / `app.js`, the dashboard and session-page HTML templates. | **Create** |
| `src/web/static/app.css` | Browser styles. | **Create** |
| `src/web/static/app.js` | Browser logic: dashboard render, session-page render, SSE subscription. | **Create** |
| `src/web/wire.rs` | `WireSession` projection of `UnifiedSession` for JSON output. | **Create** |
| `src/turn_detail.rs` | Add `#[derive(Serialize)]` to `TurnDetail`, `DetailBlock`, `ToolResultDetail`, `TurnUsage`. | **Modify** |
| `src/tui.rs` | `web_handle` field, `W` / `o` / `y` keybinds, status-line URL display, help-popup entry. | **Modify** |
| `tests/web_test.rs` | Integration tests: server boots, routes return expected, SSE works, path-traversal rejected. | **Create** |

---

## Task Decomposition

### Task 1: Cargo dependencies + serde derives on existing types

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/turn_detail.rs`

- [ ] **Step 1: Add new deps to `Cargo.toml`**

In the `[dependencies]` section of `Cargo.toml`, append:

```toml
axum = "0.8"
tokio-stream = "0.1"
arboard = "3"
```

Place them alongside the existing `tokio = ...` line.

- [ ] **Step 2: Add `Serialize` derives to `TurnDetail` and friends**

Open `src/turn_detail.rs`. The current `use` block at the top includes (or should include) `serde`. If not, add:

```rust
use serde::Serialize;
```

For each of the four affected types (`TurnUsage` at line 19, `ToolResultDetail` at line 41, `DetailBlock` at line 51, `TurnDetail` at line 75), append `Serialize` to its `#[derive(...)]` macro:

- `#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]` for `TurnUsage`
- `#[derive(Debug, Clone, PartialEq, Eq, Serialize)]` for `ToolResultDetail`
- `#[derive(Debug, Clone, PartialEq, Eq, Serialize)]` for `DetailBlock`
- `#[derive(Debug, Clone, Serialize)]` for `TurnDetail`

For the `DetailBlock` enum specifically, also add a serde tag attribute so JSON consumers can tell variants apart:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DetailBlock {
    // ... existing variants ...
}
```

- [ ] **Step 3: Verify build**

```bash
cargo build
```

Expected: succeeds. If `serde` is not imported in the file, the compiler will tell you — add the `use serde::Serialize;` line.

- [ ] **Step 4: Verify with a quick serialization round-trip in a doctest or scratch test**

In a fresh terminal, run:

```bash
cargo test --lib turn_detail
```

Expected: existing tests still pass; no regression.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/turn_detail.rs
git commit -m "feat(web): add axum/tokio-stream/arboard deps; serialize TurnDetail"
```

---

### Task 2: Web module skeleton + minimal boot

**Files:**
- Create: `src/web/mod.rs`
- Create: `src/web/handlers.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Add `pub mod web;` to `src/lib.rs`**

Insert alphabetically in `src/lib.rs`. The current order has `pub mod util;` last. Add `pub mod web;` after it (or wherever alphabetical order dictates):

```rust
pub mod util;
pub mod web;
```

- [ ] **Step 2: Create `src/web/mod.rs`**

```rust
//! Local HTTP server: a read-only browser companion to the TUI. Bound
//! to `127.0.0.1` only; toggled via `W` in the TUI.
//!
//! Architecture:
//! - `WebState` carries `Arc` clones of session list, liveness cache,
//!   and tailer registry — the web layer never holds `&mut AppState`.
//! - `start(state)` boots an `axum` server on `127.0.0.1:7457`, falling
//!   back to an OS-assigned port if 7457 is taken.
//! - `WebServerHandle` exposes the bound address and a `shutdown()`
//!   method that triggers a graceful close.

use anyhow::Result;
use axum::Router;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

pub mod handlers;
pub mod tailer;
pub mod wire;
pub mod assets;

/// Shared, read-only handle on the bits of `AppState` the web layer
/// needs. The TUI builds this once when starting the server.
#[derive(Clone)]
pub struct WebState {
    pub sessions: Arc<std::sync::Mutex<Vec<crate::unified::UnifiedSession>>>,
    pub liveness_cache: Arc<
        std::sync::Mutex<std::collections::HashMap<String, crate::liveness::CachedLiveness>>,
    >,
    pub tailer_registry: tailer::TailerRegistry,
}

/// Handle returned by [`start`]. Holds the join handle, the bound
/// address, and a shutdown channel. Drop the handle to abandon the
/// server (it will not stop on its own); call [`Self::shutdown`] for
/// a graceful close.
pub struct WebServerHandle {
    pub addr: SocketAddr,
    join: JoinHandle<()>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl WebServerHandle {
    pub fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        // Drop join handle without awaiting — the runtime will reap it
        // when the future completes (after graceful shutdown unwinds).
        drop(self.join);
    }
}

const DEFAULT_PORT: u16 = 7457;

/// Boot the web server on `127.0.0.1:7457`, falling back to an OS-
/// assigned port if 7457 is busy. Returns a handle whose `addr` field
/// reports the port actually used.
pub async fn start(state: WebState) -> Result<WebServerHandle> {
    let app = build_router(state);

    // Try the default port first.
    let preferred: SocketAddr = ([127, 0, 0, 1], DEFAULT_PORT).into();
    let listener = match tokio::net::TcpListener::bind(preferred).await {
        Ok(l) => l,
        Err(_) => {
            // Fall back to any free port.
            let any: SocketAddr = ([127, 0, 0, 1], 0).into();
            tokio::net::TcpListener::bind(any).await?
        }
    };
    let addr = listener.local_addr()?;

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let join = tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await;
    });

    Ok(WebServerHandle {
        addr,
        join,
        shutdown_tx: Some(shutdown_tx),
    })
}

fn build_router(state: WebState) -> Router {
    use axum::routing::get;
    Router::new()
        .route("/health", get(handlers::health))
        .with_state(state)
}
```

- [ ] **Step 3: Create stub modules so the file compiles**

Create `src/web/handlers.rs`:

```rust
//! Route handlers. Each handler is a free async fn; axum extracts the
//! shared [`super::WebState`] from the router.

pub async fn health() -> &'static str {
    "ok"
}
```

Create `src/web/tailer.rs`:

```rust
//! Per-session shared file tailer. Stub — full implementation in a
//! later task.

use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Default)]
pub struct TailerRegistry {
    inner: Arc<tokio::sync::Mutex<HashMap<String, ()>>>,
}
```

Create `src/web/wire.rs`:

```rust
//! Wire types for browser ↔ server JSON. Stub — full implementation in
//! a later task.
```

Create `src/web/assets.rs`:

```rust
//! Static asset embedding. Stub — full implementation in a later task.
```

- [ ] **Step 4: Verify build**

```bash
cargo build
```

Expected: succeeds. The web module is plumbed but only serves `/health`.

- [ ] **Step 5: Add a quick smoke test that the server boots**

Append at the bottom of `src/web/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn empty_state() -> WebState {
        WebState {
            sessions: Arc::new(std::sync::Mutex::new(Vec::new())),
            liveness_cache: Arc::new(std::sync::Mutex::new(Default::default())),
            tailer_registry: tailer::TailerRegistry::default(),
        }
    }

    #[tokio::test]
    async fn boots_and_health_responds() {
        let handle = start(empty_state()).await.unwrap();
        let url = format!("http://{}/health", handle.addr);
        let body = reqwest::get(&url).await.unwrap().text().await.unwrap();
        assert_eq!(body, "ok");
        handle.shutdown();
    }

    #[tokio::test]
    async fn falls_back_when_default_port_busy() {
        // Bind 7457 to make it busy.
        let blocker = tokio::net::TcpListener::bind(([127, 0, 0, 1], DEFAULT_PORT))
            .await
            .ok();
        // Whether or not blocker succeeded (CI port may already be in use),
        // start() must always succeed by falling back to port 0.
        let handle = start(empty_state()).await.unwrap();
        // Verify we did NOT bind to default if the blocker held it.
        if blocker.is_some() {
            assert_ne!(handle.addr.port(), DEFAULT_PORT);
        }
        handle.shutdown();
        drop(blocker);
    }
}
```

- [ ] **Step 6: Run the tests**

```bash
cargo test --lib web::tests
```

Expected: 2 tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/lib.rs src/web/
git commit -m "feat(web): module skeleton with axum boot + health endpoint"
```

---

### Task 3: `WireSession` + `/api/sessions` JSON endpoint

**Files:**
- Modify: `src/web/wire.rs`
- Modify: `src/web/handlers.rs`
- Modify: `src/web/mod.rs` (router)

- [ ] **Step 1: Implement `WireSession` in `src/web/wire.rs`**

```rust
//! Wire types for browser ↔ server JSON. These project the internal
//! domain types (`UnifiedSession`, `Liveness`, …) into a stable JSON
//! shape that the browser code in `app.js` consumes.

use crate::liveness::Liveness;
use crate::unified::{SessionSource, UnifiedSession};
use serde::Serialize;
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize)]
pub struct WireSession {
    pub session_id: String,
    pub source: WireSource,
    pub project_path: String,
    pub project_name: String,
    pub modified_unix_secs: u64,
    pub message_count: usize,
    pub first_user_msg: String,
    pub summary: String,
    pub liveness: WireLiveness,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WireSource {
    Cc,
    Oc,
    Co,
}

impl From<SessionSource> for WireSource {
    fn from(s: SessionSource) -> Self {
        match s {
            SessionSource::ClaudeCode => WireSource::Cc,
            SessionSource::OpenCode => WireSource::Oc,
            SessionSource::Copilot => WireSource::Co,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WireLiveness {
    Idle,
    Recent,
    Live,
}

impl From<Liveness> for WireLiveness {
    fn from(l: Liveness) -> Self {
        match l {
            Liveness::Idle => WireLiveness::Idle,
            Liveness::Recent => WireLiveness::Recent,
            Liveness::Live => WireLiveness::Live,
        }
    }
}

/// Project a `UnifiedSession` plus its current liveness into a wire
/// session. Liveness defaults to `Idle` when not in the cache.
pub fn project(session: &UnifiedSession, liveness: Liveness) -> WireSession {
    WireSession {
        session_id: session.session_id.clone(),
        source: session.source.clone().into(),
        project_path: session.project_path.clone(),
        project_name: session.project_name.clone(),
        modified_unix_secs: session
            .modified
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        message_count: session.message_count,
        first_user_msg: session.first_user_msg.clone(),
        summary: session.summary.clone(),
        liveness: liveness.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    fn make_session(id: &str) -> UnifiedSession {
        UnifiedSession {
            session_id: id.to_string(),
            project_name: "p".to_string(),
            project_path: "/tmp/p".to_string(),
            modified: UNIX_EPOCH + Duration::from_secs(100),
            message_count: 5,
            first_user_msg: "hi".to_string(),
            summary: "test".to_string(),
            git_branch: String::new(),
            source: SessionSource::ClaudeCode,
            jsonl_path: None,
            archived: false,
        }
    }

    #[test]
    fn projects_unified_to_wire() {
        let s = make_session("a");
        let w = project(&s, Liveness::Live);
        assert_eq!(w.session_id, "a");
        assert!(matches!(w.source, WireSource::Cc));
        assert!(matches!(w.liveness, WireLiveness::Live));
        assert_eq!(w.modified_unix_secs, 100);
        assert_eq!(w.message_count, 5);
    }

    #[test]
    fn serializes_to_expected_json_shape() {
        let s = make_session("a");
        let w = project(&s, Liveness::Recent);
        let json = serde_json::to_value(&w).unwrap();
        assert_eq!(json["session_id"], "a");
        assert_eq!(json["source"], "cc");
        assert_eq!(json["liveness"], "recent");
        assert_eq!(json["modified_unix_secs"], 100);
    }
}
```

- [ ] **Step 2: Add the `/api/sessions` handler to `src/web/handlers.rs`**

Replace the contents of `src/web/handlers.rs` with:

```rust
//! Route handlers. Each handler is a free async fn; axum extracts the
//! shared [`super::WebState`] from the router.

use axum::extract::State;
use axum::Json;
use crate::liveness::Liveness;

pub async fn health() -> &'static str {
    "ok"
}

/// `GET /api/sessions` — array of `WireSession` for every session in
/// the in-memory list, sorted by recency (newest first).
pub async fn api_sessions(
    State(state): State<super::WebState>,
) -> Json<Vec<super::wire::WireSession>> {
    let sessions = state
        .sessions
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let cache = state
        .liveness_cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let mut out: Vec<super::wire::WireSession> = sessions
        .iter()
        .map(|s| {
            let liveness = cache
                .get(&s.session_id)
                .map(|c| c.state)
                .unwrap_or(Liveness::Idle);
            super::wire::project(s, liveness)
        })
        .collect();
    out.sort_by(|a, b| b.modified_unix_secs.cmp(&a.modified_unix_secs));
    Json(out)
}
```

- [ ] **Step 3: Wire the route into the router**

In `src/web/mod.rs`, replace `build_router` with:

```rust
fn build_router(state: WebState) -> Router {
    use axum::routing::get;
    Router::new()
        .route("/health", get(handlers::health))
        .route("/api/sessions", get(handlers::api_sessions))
        .with_state(state)
}
```

- [ ] **Step 4: Add a unit-level integration test**

Append to the `#[cfg(test)] mod tests` block in `src/web/mod.rs`:

```rust
    #[tokio::test]
    async fn api_sessions_returns_json_with_projected_shape() {
        use crate::unified::{SessionSource, UnifiedSession};
        use std::time::{Duration, UNIX_EPOCH};

        let sessions = vec![
            UnifiedSession {
                session_id: "abc".to_string(),
                project_name: "p".to_string(),
                project_path: "/tmp/p".to_string(),
                modified: UNIX_EPOCH + Duration::from_secs(100),
                message_count: 1,
                first_user_msg: "hi".to_string(),
                summary: "x".to_string(),
                git_branch: String::new(),
                source: SessionSource::ClaudeCode,
                jsonl_path: None,
                archived: false,
            },
        ];
        let state = WebState {
            sessions: Arc::new(std::sync::Mutex::new(sessions)),
            liveness_cache: Arc::new(std::sync::Mutex::new(Default::default())),
            tailer_registry: tailer::TailerRegistry::default(),
        };
        let handle = start(state).await.unwrap();
        let url = format!("http://{}/api/sessions", handle.addr);
        let resp = reqwest::get(&url).await.unwrap();
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        let arr = body.as_array().expect("array");
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["session_id"], "abc");
        assert_eq!(arr[0]["source"], "cc");
        assert_eq!(arr[0]["liveness"], "idle");
        handle.shutdown();
    }
```

- [ ] **Step 5: Run tests**

```bash
cargo test --lib web::tests
cargo test --lib web::wire::tests
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/web/wire.rs src/web/handlers.rs src/web/mod.rs
git commit -m "feat(web): WireSession projection + /api/sessions endpoint"
```

---

### Task 4: Static assets + dashboard HTML at `/`

**Files:**
- Create: `src/web/static/app.css`
- Create: `src/web/static/app.js`
- Create: `src/web/static/dashboard.html`
- Modify: `src/web/assets.rs`
- Modify: `src/web/handlers.rs`
- Modify: `src/web/mod.rs` (router)

- [ ] **Step 1: Create the static assets**

Create `src/web/static/app.css` with a minimum-viable stylesheet:

```css
* { box-sizing: border-box; }
body {
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif;
    background: #1e2124;
    color: #d8d8d8;
    margin: 0;
    padding: 1.5rem;
    line-height: 1.5;
}
h1 { color: #00b2ff; margin-top: 0; }
a { color: #1e90ff; text-decoration: none; }
a:hover { text-decoration: underline; }
.session-row {
    padding: 0.5rem 0.75rem;
    border-bottom: 1px solid #2a2d31;
    display: grid;
    grid-template-columns: auto auto 1fr auto;
    gap: 1rem;
    align-items: baseline;
}
.session-row:hover { background: #232629; }
.glyph-live { color: #a6e3a1; }
.glyph-recent { color: #89dcebcc; }
.glyph-idle { color: #595959; }
.badge {
    font-family: ui-monospace, monospace;
    padding: 0.05rem 0.4rem;
    border-radius: 3px;
}
.badge-cc { background: #0d8300; color: #fff; }
.badge-oc { background: #1e90ff; color: #fff; }
.badge-co { background: #ff8c00; color: #000; }
.summary { color: #d8d8d8; }
.path { color: #595959; font-size: 0.9rem; }
.refresh-btn {
    float: right;
    background: #232629;
    color: #00b2ff;
    border: 1px solid #2a6180;
    padding: 0.3rem 0.7rem;
    cursor: pointer;
    border-radius: 3px;
}
.refresh-btn:hover { background: #2a3035; }
.section { margin-top: 1.5rem; }
.section-header { color: #00b2ff; border-bottom: 1px solid #2a6180; padding-bottom: 0.3rem; }
.turn { border-bottom: 1px solid #2a2d31; padding: 0.75rem 0; }
.turn-user { color: #d8d8d8; }
.turn-assistant { color: #d8d8d8; margin-top: 0.5rem; }
.tool-use, details {
    background: #232629;
    border-left: 3px solid #2a6180;
    padding: 0.4rem 0.6rem;
    margin: 0.3rem 0;
    border-radius: 0 3px 3px 0;
    font-family: ui-monospace, monospace;
    font-size: 0.85rem;
    overflow-x: auto;
}
pre { white-space: pre-wrap; word-break: break-word; margin: 0; }
.live-badge {
    color: #a6e3a1;
    font-weight: bold;
    margin-left: 0.5rem;
}
.live-badge.disconnected { color: #595959; }
```

Create `src/web/static/app.js` (minimal placeholder; full logic comes in Task 9):

```javascript
// app.js — populated in later tasks. The dashboard.html / session.html
// templates load this script at runtime.
console.log("cc-speedy web view loaded");
```

Create `src/web/static/dashboard.html`:

```html
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>cc-speedy</title>
  <link rel="stylesheet" href="/static/app.css">
</head>
<body>
  <h1>cc-speedy
    <button class="refresh-btn" onclick="window.ccSpeedy.refreshDashboard()">↻ refresh</button>
  </h1>
  <div id="dashboard">Loading…</div>
  <script src="/static/app.js"></script>
  <script>window.ccSpeedy && window.ccSpeedy.initDashboard && window.ccSpeedy.initDashboard();</script>
</body>
</html>
```

- [ ] **Step 2: Implement `src/web/assets.rs`**

```rust
//! Static asset embedding. Files in `src/web/static/` are embedded at
//! build time via `include_str!`. The HTTP handlers serve these as
//! plain text/css/javascript with `Cache-Control: no-cache` headers.

pub const APP_CSS: &str = include_str!("static/app.css");
pub const APP_JS: &str = include_str!("static/app.js");
pub const DASHBOARD_HTML: &str = include_str!("static/dashboard.html");
```

- [ ] **Step 3: Add static-asset and dashboard handlers**

Append to `src/web/handlers.rs`:

```rust
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;

pub async fn dashboard() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        super::assets::DASHBOARD_HTML,
    )
}

pub async fn static_app_css() -> impl IntoResponse {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        super::assets::APP_CSS,
    )
}

pub async fn static_app_js() -> impl IntoResponse {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        super::assets::APP_JS,
    )
}
```

- [ ] **Step 4: Wire the routes**

In `src/web/mod.rs`, replace `build_router` with:

```rust
fn build_router(state: WebState) -> Router {
    use axum::routing::get;
    Router::new()
        .route("/", get(handlers::dashboard))
        .route("/health", get(handlers::health))
        .route("/api/sessions", get(handlers::api_sessions))
        .route("/static/app.css", get(handlers::static_app_css))
        .route("/static/app.js", get(handlers::static_app_js))
        .with_state(state)
}
```

- [ ] **Step 5: Add a test**

Append to the `#[cfg(test)] mod tests` block in `src/web/mod.rs`:

```rust
    #[tokio::test]
    async fn dashboard_returns_html() {
        let handle = start(empty_state()).await.unwrap();
        let url = format!("http://{}/", handle.addr);
        let resp = reqwest::get(&url).await.unwrap();
        assert_eq!(resp.status(), 200);
        let ct = resp.headers().get("content-type").unwrap().to_str().unwrap().to_string();
        let body = resp.text().await.unwrap();
        assert!(ct.starts_with("text/html"));
        assert!(body.contains("cc-speedy"));
        assert!(body.contains("/static/app.js"));
        handle.shutdown();
    }

    #[tokio::test]
    async fn static_assets_served() {
        let handle = start(empty_state()).await.unwrap();
        let css_url = format!("http://{}/static/app.css", handle.addr);
        let js_url = format!("http://{}/static/app.js", handle.addr);
        let css = reqwest::get(&css_url).await.unwrap();
        assert_eq!(css.status(), 200);
        assert!(css.headers().get("content-type").unwrap().to_str().unwrap().starts_with("text/css"));
        let js = reqwest::get(&js_url).await.unwrap();
        assert_eq!(js.status(), 200);
        assert!(js.headers().get("content-type").unwrap().to_str().unwrap().starts_with("application/javascript"));
        handle.shutdown();
    }
```

- [ ] **Step 6: Run tests**

```bash
cargo test --lib web
```

Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/web/static src/web/assets.rs src/web/handlers.rs src/web/mod.rs
git commit -m "feat(web): static asset embedding + dashboard HTML route"
```

---

### Task 5: Per-turn JSON endpoint

**Files:**
- Modify: `src/web/handlers.rs`
- Modify: `src/web/mod.rs`

- [ ] **Step 1: Add the handler**

Append to `src/web/handlers.rs`:

```rust
use axum::extract::Path;
use crate::turn_detail::TurnDetail;

/// `GET /api/session/:id/turns/:idx` — serialize the requested turn as
/// JSON. The session id is validated against the in-memory list before
/// any path is touched, so untrusted ids cannot reach `Path::new`.
pub async fn api_turn(
    State(state): State<super::WebState>,
    Path((session_id, idx)): Path<(String, u32)>,
) -> Result<Json<TurnDetail>, StatusCode> {
    let sessions = state
        .sessions
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let session = sessions
        .iter()
        .find(|s| s.session_id == session_id)
        .ok_or(StatusCode::NOT_FOUND)?;

    let jsonl = match session.source {
        crate::unified::SessionSource::ClaudeCode | crate::unified::SessionSource::Copilot => {
            session.jsonl_path.clone().ok_or(StatusCode::NOT_FOUND)?
        }
        // OpenCode is SQLite-backed; no JSONL to extract turns from in v1.
        crate::unified::SessionSource::OpenCode => return Err(StatusCode::NOT_IMPLEMENTED),
    };

    let detail = match session.source {
        crate::unified::SessionSource::ClaudeCode => {
            crate::turn_detail::extract_turn(std::path::Path::new(&jsonl), idx)
                .map_err(|_| StatusCode::NOT_FOUND)?
        }
        crate::unified::SessionSource::Copilot => {
            crate::copilot_turn_detail::extract_turn(std::path::Path::new(&jsonl), idx)
                .map_err(|_| StatusCode::NOT_FOUND)?
        }
        crate::unified::SessionSource::OpenCode => unreachable!(),
    };

    Ok(Json(detail))
}
```

- [ ] **Step 2: Wire the route**

In `src/web/mod.rs` `build_router`, add:

```rust
        .route(
            "/api/session/:id/turns/:idx",
            get(handlers::api_turn),
        )
```

- [ ] **Step 3: Add a test**

Append to the `#[cfg(test)] mod tests` block in `src/web/mod.rs`:

```rust
    #[tokio::test]
    async fn api_turn_404_for_unknown_session() {
        let handle = start(empty_state()).await.unwrap();
        let url = format!("http://{}/api/session/nonexistent/turns/0", handle.addr);
        let resp = reqwest::get(&url).await.unwrap();
        assert_eq!(resp.status(), 404);
        handle.shutdown();
    }

    #[tokio::test]
    async fn api_turn_path_traversal_rejected() {
        let handle = start(empty_state()).await.unwrap();
        // axum routing won't even match the `..` segment to a session id;
        // this still must be handled (404 or 4xx).
        let url = format!("http://{}/api/session/..%2F..%2Fetc%2Fpasswd/turns/0", handle.addr);
        let resp = reqwest::get(&url).await.unwrap();
        assert!(resp.status().is_client_error());
        handle.shutdown();
    }
```

- [ ] **Step 4: Run tests**

```bash
cargo test --lib web
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/web/handlers.rs src/web/mod.rs
git commit -m "feat(web): per-turn JSON endpoint with id validation"
```

---

### Task 6: Tailer module — pure parser + runtime

**Files:**
- Modify: `src/web/tailer.rs`

The tailer follows a session JSONL file by polling its size on a 1 s interval. Each time the file grows, the new bytes are parsed (forward, line by line) and `TailEvent::TurnAdded` / `TurnUpdated` events are broadcast to subscribers.

For testability, the parsing is a pure function `classify_new_lines(prev_state, new_text) -> (next_state, Vec<TailEvent>)`. The tailer task owns the file I/O and calls this function.

- [ ] **Step 1: Replace the stub with the real implementation**

Replace `src/web/tailer.rs` entirely with:

```rust
//! Per-session shared file tailer with broadcast. Multiple SSE
//! subscribers to the same session share one tailer; the tailer task
//! self-cleans when the last subscriber disconnects.

use crate::unified::SessionSource;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum TailEvent {
    TurnAdded { idx: u32 },
    TurnUpdated { idx: u32 },
    LivenessChanged { state: crate::liveness::Liveness },
}

const BROADCAST_CAPACITY: usize = 64;
const POLL_INTERVAL_MS: u64 = 1000;

#[derive(Default)]
struct TailerState {
    /// Number of complete user/assistant pairs observed so far. The
    /// "current" turn index is `pair_count - 1`; the next added turn
    /// index would be `pair_count`.
    pair_count: u32,
    /// True if the most recent assistant message had an unclosed
    /// `tool_use`. A subsequent `tool_result` turns this off and emits
    /// `TurnUpdated` for the same idx.
    open_turn: bool,
    /// Last reported open-turn idx (if any), so we can emit
    /// `TurnUpdated` against the right index.
    last_open_idx: Option<u32>,
}

/// Pure classifier: given the previous state and the new text appended
/// to the JSONL since last call, return the new state and the list of
/// events to broadcast. Source-specific (CC vs Copilot) parsing is
/// dispatched on `source`.
pub fn classify_new_lines(
    source: SessionSource,
    prev: TailerState,
    new_text: &str,
) -> (TailerState, Vec<TailEvent>) {
    let mut state = prev;
    let mut events = Vec::new();
    for line in new_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        match source {
            SessionSource::ClaudeCode => classify_cc_line(&v, &mut state, &mut events),
            SessionSource::Copilot => classify_copilot_line(&v, &mut state, &mut events),
            SessionSource::OpenCode => { /* no-op; OC has no JSONL */ }
        }
    }
    (state, events)
}

fn classify_cc_line(v: &serde_json::Value, state: &mut TailerState, events: &mut Vec<TailEvent>) {
    let ty = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
    let content = v
        .get("message")
        .and_then(|m| m.get("content"))
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let blocks: Vec<serde_json::Value> = match content {
        serde_json::Value::Array(a) => a,
        _ => Vec::new(),
    };
    match ty {
        "assistant" => {
            // New assistant message starts a new turn idx.
            let idx = state.pair_count;
            state.pair_count = state.pair_count.saturating_add(1);
            let opens = blocks
                .iter()
                .any(|b| b.get("type").and_then(|x| x.as_str()) == Some("tool_use"));
            state.open_turn = opens;
            state.last_open_idx = if opens { Some(idx) } else { None };
            events.push(TailEvent::TurnAdded { idx });
        }
        "user" => {
            // A `tool_result` user line closes the previously-open turn.
            let closes = blocks
                .iter()
                .any(|b| b.get("type").and_then(|x| x.as_str()) == Some("tool_result"));
            if closes && state.open_turn {
                state.open_turn = false;
                if let Some(idx) = state.last_open_idx.take() {
                    events.push(TailEvent::TurnUpdated { idx });
                }
            }
            // Fresh user prompts (no tool_result) advance no counters; the
            // next assistant line will start a new turn.
        }
        _ => {}
    }
}

fn classify_copilot_line(v: &serde_json::Value, state: &mut TailerState, events: &mut Vec<TailEvent>) {
    let ty = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
    match ty {
        "assistant.message" => {
            let idx = state.pair_count;
            state.pair_count = state.pair_count.saturating_add(1);
            state.open_turn = true;
            state.last_open_idx = Some(idx);
            events.push(TailEvent::TurnAdded { idx });
        }
        "tool.execution_complete" => {
            if state.open_turn {
                state.open_turn = false;
                if let Some(idx) = state.last_open_idx.take() {
                    events.push(TailEvent::TurnUpdated { idx });
                }
            }
        }
        _ => {}
    }
}

/// Per-session tailer — one task, multiple subscribers via broadcast.
pub struct Tailer {
    pub broadcast: broadcast::Sender<TailEvent>,
    pub refcount: Arc<AtomicU32>,
}

impl Tailer {
    pub fn subscribe(&self) -> broadcast::Receiver<TailEvent> {
        self.refcount.fetch_add(1, Ordering::SeqCst);
        self.broadcast.subscribe()
    }

    pub fn release(&self) {
        self.refcount.fetch_sub(1, Ordering::SeqCst);
    }
}

#[derive(Clone, Default)]
pub struct TailerRegistry {
    inner: Arc<tokio::sync::Mutex<HashMap<String, Arc<Tailer>>>>,
}

impl TailerRegistry {
    /// Get-or-create the tailer for `session_id`. If created, spawns
    /// the polling task. The caller MUST `subscribe()` immediately to
    /// bump the refcount before dropping the `Arc<Tailer>`.
    pub async fn ensure(
        &self,
        session_id: &str,
        source: SessionSource,
        jsonl: PathBuf,
    ) -> Arc<Tailer> {
        let mut map = self.inner.lock().await;
        if let Some(t) = map.get(session_id) {
            return t.clone();
        }
        let (tx, _) = broadcast::channel::<TailEvent>(BROADCAST_CAPACITY);
        let tailer = Arc::new(Tailer {
            broadcast: tx.clone(),
            refcount: Arc::new(AtomicU32::new(0)),
        });
        let registry = self.clone();
        let session_id_owned = session_id.to_string();
        let refcount = tailer.refcount.clone();
        tokio::spawn(async move {
            run_tailer_task(source, jsonl, tx, refcount, registry, session_id_owned).await
        });
        map.insert(session_id.to_string(), tailer.clone());
        tailer
    }

    pub async fn contains(&self, session_id: &str) -> bool {
        self.inner.lock().await.contains_key(session_id)
    }
}

async fn run_tailer_task(
    source: SessionSource,
    path: PathBuf,
    tx: broadcast::Sender<TailEvent>,
    refcount: Arc<AtomicU32>,
    registry: TailerRegistry,
    session_id: String,
) {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};
    let mut state = TailerState::default();
    let mut last_size: u64 = 0;

    // Bootstrap: read the entire file once and process it so subscribers
    // who join later see the *current* turn count rather than starting
    // from zero. We do not emit historical events on bootstrap — only
    // events that happen after the first poll.
    if let Ok(meta) = tokio::fs::metadata(&path).await {
        last_size = meta.len();
        if let Ok(content) = tokio::fs::read_to_string(&path).await {
            let (next, _events) = classify_new_lines(source.clone(), state, &content);
            state = next;
        }
    }

    let mut interval = tokio::time::interval(std::time::Duration::from_millis(POLL_INTERVAL_MS));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        interval.tick().await;

        // Self-shutdown when refcount hits zero.
        if refcount.load(Ordering::SeqCst) == 0 {
            let mut map = registry.inner.lock().await;
            map.remove(&session_id);
            return;
        }

        let meta = match tokio::fs::metadata(&path).await {
            Ok(m) => m,
            Err(_) => continue, // file disappeared transiently; try again next tick
        };
        let size = meta.len();
        if size <= last_size {
            continue;
        }
        // Read only the new bytes.
        let mut f = match tokio::fs::File::open(&path).await {
            Ok(f) => f,
            Err(_) => continue,
        };
        if f.seek(std::io::SeekFrom::Start(last_size)).await.is_err() {
            continue;
        }
        let mut buf = Vec::with_capacity((size - last_size) as usize);
        if f.read_to_end(&mut buf).await.is_err() {
            continue;
        }
        last_size = size;
        let new_text = String::from_utf8_lossy(&buf).into_owned();
        let (next_state, events) = classify_new_lines(source.clone(), state, &new_text);
        state = next_state;
        for ev in events {
            let _ = tx.send(ev);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cc_state() -> TailerState {
        TailerState::default()
    }

    #[test]
    fn cc_assistant_text_emits_turn_added() {
        let s = cc_state();
        let txt = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hi"}]}}
"#;
        let (st, ev) = classify_new_lines(SessionSource::ClaudeCode, s, txt);
        assert_eq!(st.pair_count, 1);
        assert!(!st.open_turn);
        assert_eq!(ev.len(), 1);
        assert!(matches!(ev[0], TailEvent::TurnAdded { idx: 0 }));
    }

    #[test]
    fn cc_assistant_with_tool_use_marks_open_turn() {
        let s = cc_state();
        let txt = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"X","input":{}}]}}
"#;
        let (st, ev) = classify_new_lines(SessionSource::ClaudeCode, s, txt);
        assert_eq!(st.pair_count, 1);
        assert!(st.open_turn);
        assert_eq!(st.last_open_idx, Some(0));
        assert!(matches!(ev[0], TailEvent::TurnAdded { idx: 0 }));
    }

    #[test]
    fn cc_tool_result_closes_previous_turn() {
        let s = cc_state();
        let txt = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"X","input":{}}]}}
{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"ok"}]}}
"#;
        let (st, ev) = classify_new_lines(SessionSource::ClaudeCode, s, txt);
        assert_eq!(st.pair_count, 1);
        assert!(!st.open_turn);
        assert_eq!(ev.len(), 2);
        assert!(matches!(ev[0], TailEvent::TurnAdded { idx: 0 }));
        assert!(matches!(ev[1], TailEvent::TurnUpdated { idx: 0 }));
    }

    #[test]
    fn copilot_assistant_then_tool_complete() {
        let s = cc_state();
        let txt = r#"{"type":"assistant.message"}
{"type":"tool.execution_complete"}
"#;
        let (st, ev) = classify_new_lines(SessionSource::Copilot, s, txt);
        assert_eq!(st.pair_count, 1);
        assert!(!st.open_turn);
        assert_eq!(ev.len(), 2);
        assert!(matches!(ev[0], TailEvent::TurnAdded { idx: 0 }));
        assert!(matches!(ev[1], TailEvent::TurnUpdated { idx: 0 }));
    }

    #[test]
    fn opencode_emits_no_events() {
        let s = cc_state();
        let txt = r#"{"type":"whatever"}
"#;
        let (st, ev) = classify_new_lines(SessionSource::OpenCode, s, txt);
        assert_eq!(st.pair_count, 0);
        assert!(ev.is_empty());
    }

    #[test]
    fn malformed_lines_skipped() {
        let s = cc_state();
        let txt = "not json\n\n";
        let (st, ev) = classify_new_lines(SessionSource::ClaudeCode, s, txt);
        assert_eq!(st.pair_count, 0);
        assert!(ev.is_empty());
    }

    #[tokio::test]
    async fn registry_creates_and_caches_tailer() {
        let reg = TailerRegistry::default();
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.jsonl");
        std::fs::write(&path, "").unwrap();

        let t1 = reg.ensure("a", SessionSource::ClaudeCode, path.clone()).await;
        let t2 = reg.ensure("a", SessionSource::ClaudeCode, path.clone()).await;
        assert!(Arc::ptr_eq(&t1, &t2));
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test --lib web::tailer
```

Expected: 7 tests pass (6 pure-logic + 1 registry).

- [ ] **Step 3: Commit**

```bash
git add src/web/tailer.rs
git commit -m "feat(web): tailer module with per-session shared broadcast"
```

---

### Task 7: SSE endpoint at `/session/:id/stream`

**Files:**
- Modify: `src/web/handlers.rs`
- Modify: `src/web/mod.rs`
- Modify: `Cargo.toml` (already done)

- [ ] **Step 1: Add the SSE handler**

Append to `src/web/handlers.rs`:

```rust
use axum::response::sse::{Event, KeepAlive, Sse};
use std::convert::Infallible;
use futures::stream::Stream;

/// `GET /session/:id/stream` — SSE stream of `TailEvent`s.
pub async fn sse_stream(
    State(state): State<super::WebState>,
    Path(session_id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    use tokio_stream::StreamExt;

    // Validate session id and resolve its JSONL.
    let sessions = state
        .sessions
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let session = sessions
        .iter()
        .find(|s| s.session_id == session_id)
        .ok_or(StatusCode::NOT_FOUND)?
        .clone();
    let jsonl = match session.source {
        crate::unified::SessionSource::ClaudeCode | crate::unified::SessionSource::Copilot => {
            session.jsonl_path.clone().ok_or(StatusCode::NOT_FOUND)?
        }
        crate::unified::SessionSource::OpenCode => return Err(StatusCode::NOT_IMPLEMENTED),
    };

    let tailer = state
        .tailer_registry
        .ensure(&session_id, session.source.clone(), jsonl.into())
        .await;
    let rx = tailer.subscribe();
    let release_tailer = tailer.clone();

    let stream = tokio_stream::wrappers::BroadcastStream::new(rx)
        .map(move |item| {
            // Drop the receiver-released signal cleanly; we still need a Result<Event,_>
            let _ = &release_tailer; // keep clone alive for the duration of stream
            match item {
                Ok(ev) => {
                    let payload = serde_json::to_string(&ev).unwrap_or_else(|_| "{}".to_string());
                    let kind = match &ev {
                        super::tailer::TailEvent::TurnAdded { .. } => "turn-added",
                        super::tailer::TailEvent::TurnUpdated { .. } => "turn-updated",
                        super::tailer::TailEvent::LivenessChanged { .. } => "liveness",
                    };
                    Ok(Event::default().event(kind).data(payload))
                }
                Err(_) => Ok(Event::default().event("lag").data("{}")),
            }
        });

    // Decrement refcount when the stream is dropped (subscriber disconnects).
    // We rely on the `release_tailer` clone being captured in the stream
    // closure; once the response future completes, the clone drops and
    // the next call to `tailer.release()` would adjust refcount. To do
    // this cleanly, we wrap the stream in a struct with a Drop impl.
    let stream = WithDropRelease::new(stream, tailer.clone());

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// Wrapper that calls `Tailer::release()` when dropped, decrementing
/// the refcount so the tailer's self-cleanup loop can wind down.
pub struct WithDropRelease<S> {
    inner: S,
    tailer: std::sync::Arc<super::tailer::Tailer>,
}

impl<S> WithDropRelease<S> {
    fn new(inner: S, tailer: std::sync::Arc<super::tailer::Tailer>) -> Self {
        Self { inner, tailer }
    }
}

impl<S: Stream + Unpin> Stream for WithDropRelease<S> {
    type Item = S::Item;
    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        std::pin::Pin::new(&mut self.inner).poll_next(cx)
    }
}

impl<S> Drop for WithDropRelease<S> {
    fn drop(&mut self) {
        self.tailer.release();
    }
}
```

- [ ] **Step 2: Add `futures` to Cargo.toml**

`tokio-stream` provides `wrappers::BroadcastStream` but the `Stream` trait it implements comes from `futures::stream::Stream` (re-exported by `futures-core`). Add to `Cargo.toml` under `[dependencies]`:

```toml
futures = "0.3"
```

- [ ] **Step 3: Wire the route**

In `src/web/mod.rs` `build_router`:

```rust
        .route("/session/:id/stream", get(handlers::sse_stream))
```

- [ ] **Step 4: Add an SSE integration test**

Append to the `#[cfg(test)] mod tests` block in `src/web/mod.rs`:

```rust
    #[tokio::test]
    async fn sse_emits_turn_added_when_jsonl_grows() {
        use crate::unified::{SessionSource, UnifiedSession};
        use std::time::SystemTime;

        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.jsonl");
        std::fs::write(&path, "").unwrap();

        let sessions = vec![UnifiedSession {
            session_id: "s1".to_string(),
            project_name: "p".to_string(),
            project_path: "/tmp".to_string(),
            modified: SystemTime::now(),
            message_count: 0,
            first_user_msg: String::new(),
            summary: String::new(),
            git_branch: String::new(),
            source: SessionSource::ClaudeCode,
            jsonl_path: Some(path.to_string_lossy().into_owned()),
            archived: false,
        }];
        let state = WebState {
            sessions: Arc::new(std::sync::Mutex::new(sessions)),
            liveness_cache: Arc::new(std::sync::Mutex::new(Default::default())),
            tailer_registry: tailer::TailerRegistry::default(),
        };
        let handle = start(state).await.unwrap();
        let url = format!("http://{}/session/s1/stream", handle.addr);

        // Subscribe in a background task, then write a turn into the JSONL
        // and wait for the first SSE event to arrive.
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(4);
        tokio::spawn(async move {
            let resp = reqwest::get(&url).await.unwrap();
            let mut stream = resp.bytes_stream();
            use futures::StreamExt;
            while let Some(Ok(chunk)) = stream.next().await {
                let s = String::from_utf8_lossy(&chunk).to_string();
                if !s.trim().is_empty() {
                    let _ = tx.send(s).await;
                    if !s.contains(":keep-alive") && s.contains("turn-added") {
                        return;
                    }
                }
            }
        });

        // Give the tailer time to start and the bootstrap read to complete.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Append an assistant line; the tailer should pick it up within ~1s.
        std::fs::write(
            &path,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hi"}]}}
"#,
        )
        .unwrap();

        // Wait up to 3s for the SSE chunk to arrive.
        let mut got_turn_added = false;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
        while tokio::time::Instant::now() < deadline {
            if let Ok(Some(chunk)) = tokio::time::timeout(
                std::time::Duration::from_millis(500),
                rx.recv(),
            )
            .await
            {
                if chunk.contains("turn-added") {
                    got_turn_added = true;
                    break;
                }
            }
        }
        assert!(got_turn_added, "expected SSE to emit turn-added within 3s");
        handle.shutdown();
    }
```

- [ ] **Step 5: Run the SSE test**

```bash
cargo test --lib web::tests::sse_emits_turn_added_when_jsonl_grows --release -- --nocapture
```

Expected: passes within 3 seconds. Use `--release` because the test uses real timing.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/web/handlers.rs src/web/mod.rs
git commit -m "feat(web): SSE endpoint for live turn-added events"
```

---

### Task 8: Session detail HTML page at `/session/:id`

**Files:**
- Create: `src/web/static/session.html`
- Modify: `src/web/assets.rs`
- Modify: `src/web/handlers.rs`
- Modify: `src/web/mod.rs`

- [ ] **Step 1: Create `src/web/static/session.html`**

```html
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>cc-speedy — session</title>
  <link rel="stylesheet" href="/static/app.css">
</head>
<body>
  <a href="/">← back to dashboard</a>
  <h1 id="session-title">Loading…
    <span id="live-badge" class="live-badge disconnected">○ idle</span>
  </h1>
  <div id="session-meta"></div>
  <div id="turns">Loading turns…</div>
  <script src="/static/app.js"></script>
  <script>window.ccSpeedy && window.ccSpeedy.initSession && window.ccSpeedy.initSession();</script>
</body>
</html>
```

- [ ] **Step 2: Add the constant to `src/web/assets.rs`**

```rust
pub const SESSION_HTML: &str = include_str!("static/session.html");
```

- [ ] **Step 3: Add the handler**

Append to `src/web/handlers.rs`:

```rust
pub async fn session_page(
    State(state): State<super::WebState>,
    Path(session_id): Path<String>,
) -> Result<axum::response::Response, StatusCode> {
    let sessions = state
        .sessions
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    if !sessions.iter().any(|s| s.session_id == session_id) {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        super::assets::SESSION_HTML,
    )
        .into_response())
}
```

- [ ] **Step 4: Wire the route**

In `src/web/mod.rs`:

```rust
        .route("/session/:id", get(handlers::session_page))
```

- [ ] **Step 5: Add a test**

Append to the `#[cfg(test)] mod tests` block in `src/web/mod.rs`:

```rust
    #[tokio::test]
    async fn session_page_renders_for_known_id() {
        use crate::unified::{SessionSource, UnifiedSession};
        use std::time::SystemTime;
        let sessions = vec![UnifiedSession {
            session_id: "abc".to_string(),
            project_name: "p".to_string(),
            project_path: "/tmp".to_string(),
            modified: SystemTime::now(),
            message_count: 0,
            first_user_msg: String::new(),
            summary: String::new(),
            git_branch: String::new(),
            source: SessionSource::ClaudeCode,
            jsonl_path: None,
            archived: false,
        }];
        let state = WebState {
            sessions: Arc::new(std::sync::Mutex::new(sessions)),
            liveness_cache: Arc::new(std::sync::Mutex::new(Default::default())),
            tailer_registry: tailer::TailerRegistry::default(),
        };
        let handle = start(state).await.unwrap();
        let url = format!("http://{}/session/abc", handle.addr);
        let resp = reqwest::get(&url).await.unwrap();
        assert_eq!(resp.status(), 200);
        let body = resp.text().await.unwrap();
        assert!(body.contains("Loading"));
        handle.shutdown();
    }

    #[tokio::test]
    async fn session_page_404_unknown_id() {
        let handle = start(empty_state()).await.unwrap();
        let url = format!("http://{}/session/nope", handle.addr);
        let resp = reqwest::get(&url).await.unwrap();
        assert_eq!(resp.status(), 404);
        handle.shutdown();
    }
```

- [ ] **Step 6: Run tests**

```bash
cargo test --lib web::tests
```

Expected: passes.

- [ ] **Step 7: Commit**

```bash
git add src/web/static/session.html src/web/assets.rs src/web/handlers.rs src/web/mod.rs
git commit -m "feat(web): session detail HTML route"
```

---

### Task 9: Frontend JavaScript (dashboard + session page + SSE)

**Files:**
- Modify: `src/web/static/app.js`

This is the largest single piece of frontend code. Vanilla JS, no framework.

- [ ] **Step 1: Replace `src/web/static/app.js` with the full implementation**

```javascript
// app.js — cc-speedy web view (vanilla JS, no build step).
// Exports `window.ccSpeedy` with `initDashboard`, `refreshDashboard`, and `initSession`.

(function () {
    "use strict";

    function escapeHtml(s) {
        return String(s)
            .replace(/&/g, "&amp;")
            .replace(/</g, "&lt;")
            .replace(/>/g, "&gt;")
            .replace(/"/g, "&quot;")
            .replace(/'/g, "&#039;");
    }

    function formatRelative(unixSecs) {
        const now = Math.floor(Date.now() / 1000);
        const delta = now - unixSecs;
        if (delta < 60) return delta + "s ago";
        if (delta < 3600) return Math.floor(delta / 60) + "m ago";
        if (delta < 86400) return Math.floor(delta / 3600) + "h ago";
        return Math.floor(delta / 86400) + "d ago";
    }

    function livenessGlyph(state) {
        if (state === "live") return '<span class="glyph-live">▶</span>';
        if (state === "recent") return '<span class="glyph-recent">◦</span>';
        return '<span class="glyph-idle">·</span>';
    }

    function badge(source) {
        const cls = "badge badge-" + source;
        return '<span class="' + cls + '">' + source.toUpperCase() + '</span>';
    }

    function renderSessionRow(s) {
        const truncatedSummary = (s.summary || s.first_user_msg || "")
            .slice(0, 80);
        return (
            '<a href="/session/' + encodeURIComponent(s.session_id) + '" class="session-row">' +
                livenessGlyph(s.liveness) +
                badge(s.source) +
                '<span class="summary">' + escapeHtml(truncatedSummary) + '</span>' +
                '<span class="path">' + escapeHtml(s.project_path) + '  ·  ' + formatRelative(s.modified_unix_secs) + '</span>' +
            '</a>'
        );
    }

    async function refreshDashboard() {
        const root = document.getElementById("dashboard");
        if (!root) return;
        try {
            const resp = await fetch("/api/sessions");
            if (!resp.ok) {
                root.innerHTML = "Error: " + resp.status;
                return;
            }
            const sessions = await resp.json();
            const groups = { cc: [], oc: [], co: [] };
            for (const s of sessions) {
                if (groups[s.source]) groups[s.source].push(s);
            }
            const sectionTitles = { cc: "Claude Code", oc: "OpenCode", co: "Copilot" };
            let html = "";
            for (const k of ["cc", "oc", "co"]) {
                if (groups[k].length === 0) continue;
                html += '<div class="section">';
                html += '<h2 class="section-header">' + sectionTitles[k] + " (" + groups[k].length + ")</h2>";
                html += groups[k].map(renderSessionRow).join("");
                html += "</div>";
            }
            if (html === "") html = "<p>No sessions found.</p>";
            root.innerHTML = html;
        } catch (e) {
            root.innerHTML = "Error: " + escapeHtml(e.message);
        }
    }

    function initDashboard() {
        refreshDashboard();
    }

    function renderTurn(turn) {
        let html = '<div class="turn">';
        if (turn.user_msg) {
            html += '<div class="turn-user"><strong>USER:</strong> ' + escapeHtml(turn.user_msg) + '</div>';
        }
        html += '<div class="turn-assistant">';
        for (const block of (turn.blocks || [])) {
            if (block.kind === "text") {
                html += '<div>' + escapeHtml(block.text || "") + '</div>';
            } else if (block.kind === "thinking") {
                html += '<details><summary>thinking</summary><pre>' + escapeHtml(block.text || "") + '</pre></details>';
            } else if (block.kind === "tool_use") {
                html += '<div class="tool-use"><strong>tool: ' + escapeHtml(block.name || "") + '</strong>';
                if (block.input_json) html += '<pre>' + escapeHtml(block.input_json) + '</pre>';
                html += '</div>';
            } else if (block.kind === "tool_result") {
                html += '<details><summary>tool result' + (block.is_error ? " (error)" : "") + '</summary>';
                html += '<pre>' + escapeHtml(block.text || "") + '</pre></details>';
            }
        }
        html += '</div></div>';
        return html;
    }

    async function initSession() {
        const sessionId = location.pathname.split("/").pop();
        const titleEl = document.getElementById("session-title");
        const turnsEl = document.getElementById("turns");
        const liveBadgeEl = document.getElementById("live-badge");
        if (!sessionId || !turnsEl) return;

        // Find the session in the dashboard payload to populate the title bar.
        try {
            const resp = await fetch("/api/sessions");
            const sessions = await resp.json();
            const session = sessions.find(s => s.session_id === sessionId);
            if (session) {
                titleEl.firstChild.textContent = session.summary || session.first_user_msg || sessionId;
                liveBadgeEl.textContent = session.liveness === "live" ? "▶ live" : (session.liveness === "recent" ? "◦ recent" : "○ idle");
                liveBadgeEl.className = "live-badge" + (session.liveness === "live" ? "" : " disconnected");

                // Eagerly render the most recent N turns by walking turn idx
                // until we hit a 404. We don't know the count up front, so we
                // walk forward until 404 (capped at 200 to bound work).
                turnsEl.innerHTML = "";
                let renderedAny = false;
                for (let i = 0; i < 200; i++) {
                    try {
                        const tResp = await fetch("/api/session/" + encodeURIComponent(sessionId) + "/turns/" + i);
                        if (!tResp.ok) break;
                        const turn = await tResp.json();
                        turnsEl.insertAdjacentHTML("beforeend", renderTurn(turn));
                        renderedAny = true;
                    } catch (e) {
                        break;
                    }
                }
                if (!renderedAny) {
                    turnsEl.textContent = "No turns yet.";
                }

                // If session is Live, open the SSE stream.
                if (session.liveness === "live") {
                    openStream(sessionId, turnsEl, liveBadgeEl);
                }
            } else {
                titleEl.firstChild.textContent = "Unknown session";
            }
        } catch (e) {
            turnsEl.innerHTML = "Error loading session: " + escapeHtml(e.message);
        }
    }

    function openStream(sessionId, turnsEl, liveBadgeEl) {
        const url = "/session/" + encodeURIComponent(sessionId) + "/stream";
        const es = new EventSource(url);
        es.addEventListener("turn-added", async (e) => {
            try {
                const data = JSON.parse(e.data);
                const idx = data.idx;
                const tResp = await fetch("/api/session/" + encodeURIComponent(sessionId) + "/turns/" + idx);
                if (tResp.ok) {
                    const turn = await tResp.json();
                    turnsEl.insertAdjacentHTML("beforeend", renderTurn(turn));
                    window.scrollTo(0, document.body.scrollHeight);
                }
            } catch (err) { /* ignore */ }
        });
        es.addEventListener("turn-updated", async (e) => {
            // For v1, naive: re-render the entire view. (Later could update in place.)
            await initSession();
        });
        es.addEventListener("liveness", (e) => {
            try {
                const data = JSON.parse(e.data);
                liveBadgeEl.textContent = data.state === "live" ? "▶ live" : (data.state === "recent" ? "◦ recent" : "○ idle");
                liveBadgeEl.className = "live-badge" + (data.state === "live" ? "" : " disconnected");
                if (data.state !== "live") {
                    es.close();
                }
            } catch (err) { /* ignore */ }
        });
        es.onerror = () => {
            liveBadgeEl.className = "live-badge disconnected";
            // Browser auto-reconnects EventSource on transient drops.
        };
    }

    window.ccSpeedy = {
        initDashboard,
        refreshDashboard,
        initSession,
    };
})();
```

- [ ] **Step 2: Manual smoke (optional but encouraged)**

```bash
cargo run
```

In the TUI: (we don't have the `W` toggle yet; that comes in Task 10). Use a separate test invocation if you want to verify the JS loads correctly.

- [ ] **Step 3: Commit**

```bash
git add src/web/static/app.js
git commit -m "feat(web): vanilla JS frontend for dashboard + session page + SSE"
```

---

### Task 10: AppState `web_handle` + `W` toggle keybind + status-line URL

**Files:**
- Modify: `src/tui.rs`

- [ ] **Step 1: Add the field to `AppState`**

In `struct AppState`, append:

```rust
    /// `Some(handle)` while the local web server is running. Toggle via `W`.
    web_handle: Option<crate::web::WebServerHandle>,
```

In `AppState::new`'s `Self { ... }` literal, add:

```rust
            web_handle: None,
```

- [ ] **Step 2: Add a `WebState` clone helper on `AppState`**

In the same `impl AppState { ... }` block:

```rust
    fn web_state(&self) -> crate::web::WebState {
        // The web layer needs Arc-wrapped session and liveness state.
        // For now we wrap the current session list in an Arc<Mutex> at
        // the time of capture; the server re-locks it per request.
        let sessions = std::sync::Arc::new(std::sync::Mutex::new(self.sessions.clone()));
        crate::web::WebState {
            sessions,
            liveness_cache: self.liveness_cache.clone(),
            tailer_registry: crate::web::tailer::TailerRegistry::default(),
        }
    }
```

(Note: in v1 the web layer sees a snapshot of sessions at the moment `W` is pressed. A future enhancement could wire `app.sessions` itself to `Arc<Mutex<Vec<UnifiedSession>>>` so the dashboard reflects refresh updates without restarting the server. For v1 this is acceptable — the user can press `W W` to restart with fresh data.)

- [ ] **Step 3: Add the `W` keybind**

In `run_event_loop`'s match arm for `(AppMode::Normal, KeyModifiers::NONE, KeyCode::Char('W')) =>` (add this arm next to the other Normal-mode keybinds):

```rust
                    (AppMode::Normal, KeyModifiers::NONE, KeyCode::Char('W')) => {
                        match app.web_handle.take() {
                            Some(handle) => {
                                handle.shutdown();
                                app.status_msg = Some(("web stopped".to_string(), Instant::now()));
                            }
                            None => {
                                let state = app.web_state();
                                match crate::web::start(state).await {
                                    Ok(handle) => {
                                        let msg = format!("web: http://{}", handle.addr);
                                        app.web_handle = Some(handle);
                                        app.status_msg = Some((msg, Instant::now()));
                                    }
                                    Err(e) => {
                                        app.status_msg =
                                            Some((format!("web start failed: {e}"), Instant::now()));
                                    }
                                }
                            }
                        }
                    }
```

- [ ] **Step 4: Display the URL in the status hint while running**

In the `AppMode::Normal` arm of the status-hint match (in `draw`), find the `prefix` declaration. After it, declare:

```rust
            let web_suffix = match &app.web_handle {
                Some(h) => format!("  · web: http://{}", h.addr),
                None => String::new(),
            };
```

Append `, web_suffix` to the format string and to the args list, e.g.:

```rust
            } else if app.filter.is_empty() {
                format!("{}  (F1: help  /: filter  ?: grep  L: library{})", prefix, web_suffix)
            } else {
                format!("{}  filter: {}{}", prefix, app.filter, web_suffix)
            };
```

(Adapt to whichever branches of the existing format chain in your code.)

- [ ] **Step 5: Build and smoke-test**

```bash
cargo build
cargo run
```

- Press `W` → status flashes `web: http://127.0.0.1:7457` (or another port).
- Open `http://127.0.0.1:7457` in a browser → see the dashboard.
- Press `W` again → status shows `web stopped`; server is unreachable.

- [ ] **Step 6: Commit**

```bash
git add src/tui.rs
git commit -m "feat(web): W toggle for local server + status-line URL"
```

---

### Task 11: `o` (open in browser) + `y` (yank URL) keybinds

**Files:**
- Modify: `src/tui.rs`

- [ ] **Step 1: Add the `o` keybind**

Add to the keybind dispatch in `run_event_loop`:

```rust
                    (AppMode::Normal, KeyModifiers::NONE, KeyCode::Char('o')) => {
                        if let Some(ref h) = app.web_handle {
                            let url = format!("http://{}", h.addr);
                            // Best-effort: try xdg-open on Linux, open on macOS, start on Windows.
                            let opener = if cfg!(target_os = "macos") {
                                "open"
                            } else if cfg!(target_os = "windows") {
                                "cmd"
                            } else {
                                "xdg-open"
                            };
                            let result = if cfg!(target_os = "windows") {
                                std::process::Command::new(opener)
                                    .args(["/C", "start", &url])
                                    .spawn()
                            } else {
                                std::process::Command::new(opener).arg(&url).spawn()
                            };
                            match result {
                                Ok(_) => app.status_msg = Some(("opened in browser".to_string(), Instant::now())),
                                Err(e) => app.status_msg = Some((format!("open failed: {e}"), Instant::now())),
                            }
                        }
                    }
```

Note: `o` is currently bound to "save current summary to Obsidian" elsewhere in the codebase. If that conflict exists, **rename the new web `o` binding to a non-conflicting key** like `Ctrl+B` (browser). Search `grep -n "Char('o')" src/tui.rs` to confirm. **Use the unconflicted key in the actual implementation.**

- [ ] **Step 2: Add the `y` keybind for clipboard**

```rust
                    (AppMode::Normal, KeyModifiers::NONE, KeyCode::Char('y')) => {
                        if let Some(ref h) = app.web_handle {
                            let url = format!("http://{}", h.addr);
                            match arboard::Clipboard::new() {
                                Ok(mut cb) => match cb.set_text(url.clone()) {
                                    Ok(_) => app.status_msg = Some(("URL copied".to_string(), Instant::now())),
                                    Err(e) => app.status_msg = Some((format!("clipboard error: {e}"), Instant::now())),
                                },
                                Err(e) => app.status_msg = Some((format!("clipboard unavailable: {e}"), Instant::now())),
                            }
                        }
                    }
```

If `y` conflicts with an existing keybind (search `grep -n "Char('y')" src/tui.rs`), use `Ctrl+Y` or similar instead.

- [ ] **Step 3: Build and smoke**

```bash
cargo build
```

If `arboard` fails to build on WSL2 (some Linux variants without proper xclip/xsel support), the test will surface this. The user will see a "clipboard unavailable: ..." message — that's the expected graceful-failure path.

- [ ] **Step 4: Commit**

```bash
git add src/tui.rs
git commit -m "feat(web): o (open) and y (yank URL) helpers"
```

---

### Task 12: Document new keybinds in the help screen

**Files:**
- Modify: `src/tui.rs`

- [ ] **Step 1: Find the help-popup `App` section**

Search:

```bash
grep -n '"  App"' src/tui.rs
```

The line is in `draw_help_popup`. The current "App" section ends with:

```rust
        Line::from("    s            settings   |   F1  this help   |   q  quit"),
```

- [ ] **Step 2: Add the new keybinds**

Insert before that line:

```rust
        Line::from("    W            toggle local web server (browser companion)"),
        Line::from("    o            open web URL in default browser   |   y  yank URL"),
```

(Adjust if `o` / `y` were renamed in Task 11 due to conflicts.)

- [ ] **Step 3: Build and verify**

```bash
cargo build
cargo run
```

Press `F1`. New lines appear.

- [ ] **Step 4: Commit**

```bash
git add src/tui.rs
git commit -m "docs(web): document W/o/y in help popup"
```

---

### Task 13: Public-API integration tests in `tests/web_test.rs`

**Files:**
- Create: `tests/web_test.rs`

The tests inside `src/web/mod.rs` use direct module access. This new suite exercises the public surface (`cc_speedy::web::start`, `WebState`, `WebServerHandle`) the way external code (the TUI) would.

- [ ] **Step 1: Create `tests/web_test.rs`**

```rust
use cc_speedy::liveness::CachedLiveness;
use cc_speedy::unified::{SessionSource, UnifiedSession};
use cc_speedy::web::{self, tailer::TailerRegistry, WebState};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

fn empty_state() -> WebState {
    WebState {
        sessions: Arc::new(Mutex::new(Vec::new())),
        liveness_cache: Arc::new(Mutex::new(Default::default())),
        tailer_registry: TailerRegistry::default(),
    }
}

fn state_with_sessions(sessions: Vec<UnifiedSession>) -> WebState {
    WebState {
        sessions: Arc::new(Mutex::new(sessions)),
        liveness_cache: Arc::new(Mutex::new(Default::default())),
        tailer_registry: TailerRegistry::default(),
    }
}

#[tokio::test]
async fn server_starts_and_health_responds() {
    let handle = web::start(empty_state()).await.unwrap();
    let url = format!("http://{}/health", handle.addr);
    let body = reqwest::get(&url).await.unwrap().text().await.unwrap();
    assert_eq!(body, "ok");
    handle.shutdown();
}

#[tokio::test]
async fn dashboard_html_contains_app_js_link() {
    let handle = web::start(empty_state()).await.unwrap();
    let url = format!("http://{}/", handle.addr);
    let body = reqwest::get(&url).await.unwrap().text().await.unwrap();
    assert!(body.contains("/static/app.js"));
    handle.shutdown();
}

#[tokio::test]
async fn api_sessions_lists_all_sources() {
    let sessions = vec![
        UnifiedSession {
            session_id: "cc-1".to_string(),
            project_name: "p".to_string(),
            project_path: "/tmp".to_string(),
            modified: SystemTime::now(),
            message_count: 0,
            first_user_msg: String::new(),
            summary: String::new(),
            git_branch: String::new(),
            source: SessionSource::ClaudeCode,
            jsonl_path: None,
            archived: false,
        },
        UnifiedSession {
            session_id: "oc-1".to_string(),
            project_name: "p".to_string(),
            project_path: "/tmp".to_string(),
            modified: SystemTime::now(),
            message_count: 0,
            first_user_msg: String::new(),
            summary: String::new(),
            git_branch: String::new(),
            source: SessionSource::OpenCode,
            jsonl_path: None,
            archived: false,
        },
    ];
    let handle = web::start(state_with_sessions(sessions)).await.unwrap();
    let url = format!("http://{}/api/sessions", handle.addr);
    let body: serde_json::Value = reqwest::get(&url).await.unwrap().json().await.unwrap();
    let arr = body.as_array().unwrap();
    let sources: std::collections::HashSet<&str> =
        arr.iter().map(|v| v["source"].as_str().unwrap()).collect();
    assert!(sources.contains("cc"));
    assert!(sources.contains("oc"));
    handle.shutdown();
}

#[tokio::test]
async fn bound_address_is_localhost_only() {
    let handle = web::start(empty_state()).await.unwrap();
    assert!(handle.addr.ip().is_loopback());
    handle.shutdown();
}

#[tokio::test]
async fn session_page_404_for_unknown_id() {
    let handle = web::start(empty_state()).await.unwrap();
    let url = format!("http://{}/session/unknown", handle.addr);
    let resp = reqwest::get(&url).await.unwrap();
    assert_eq!(resp.status(), 404);
    handle.shutdown();
}

#[tokio::test]
async fn liveness_cache_reflected_in_api_sessions() {
    let sessions = vec![UnifiedSession {
        session_id: "live-1".to_string(),
        project_name: "p".to_string(),
        project_path: "/tmp".to_string(),
        modified: SystemTime::now(),
        message_count: 0,
        first_user_msg: String::new(),
        summary: String::new(),
        git_branch: String::new(),
        source: SessionSource::ClaudeCode,
        jsonl_path: None,
        archived: false,
    }];
    let mut cache_map = std::collections::HashMap::new();
    cache_map.insert(
        "live-1".to_string(),
        CachedLiveness {
            state: cc_speedy::liveness::Liveness::Live,
            observed_at: std::time::Instant::now(),
        },
    );
    let state = WebState {
        sessions: Arc::new(Mutex::new(sessions)),
        liveness_cache: Arc::new(Mutex::new(cache_map)),
        tailer_registry: TailerRegistry::default(),
    };
    let handle = web::start(state).await.unwrap();
    let url = format!("http://{}/api/sessions", handle.addr);
    let body: serde_json::Value = reqwest::get(&url).await.unwrap().json().await.unwrap();
    assert_eq!(body[0]["liveness"], "live");
    handle.shutdown();
}
```

- [ ] **Step 2: Run the test suite**

```bash
cargo test --test web_test
```

Expected: 6 tests pass.

- [ ] **Step 3: Commit**

```bash
git add tests/web_test.rs
git commit -m "test(web): public-API integration suite"
```

---

### Task 14: Final verification

- [ ] **Step 1: Full release build**

```bash
cargo build --release
```

Expected: succeeds, with no NEW warnings introduced by this branch.

- [ ] **Step 2: Full test suite**

```bash
cargo test
```

Expected: every test passes — including the new `web::tests` (~10 tests in `src/web/mod.rs`), `web::wire::tests` (2 tests), `web::tailer::tests` (7 tests), and `tests/web_test.rs` (6 tests).

- [ ] **Step 3: Lint**

```bash
cargo clippy --all-targets
cargo fmt --check
```

Expected: pre-existing warnings only; format clean.

- [ ] **Step 4: End-to-end manual smoke**

```bash
cargo run
```

Expected behavior:
- TUI opens normally.
- Press `W` → status flashes `web: http://127.0.0.1:7457`. Open the URL.
- Browser dashboard shows three columns (or only those with sessions). Click a session → loads the session detail page; turns render top-to-bottom.
- For a CC session that's currently running in another terminal, the page shows `▶ live` and new turns append in real time.
- Press `W` again → status shows `web stopped`. Browser tab gets a connection-refused / closed.
- Press `y` while the server is running → URL is on the clipboard.
- Press `o` → browser tab opens (best-effort).
- `F1` → help popup mentions `W`, `o`, `y`.

- [ ] **Step 5: No further commit needed if everything passes**

---

## Self-Review

**Spec coverage:**
- Lifecycle (off-by-default, `W` toggle, port-fallback): Tasks 2 and 10.
- Bind to `127.0.0.1`: enforced in Task 2.
- Routes (`/`, `/session/:id`, `/session/:id/stream`, `/api/sessions`, `/api/session/:id/turns/:idx`, static): Tasks 2-8.
- SSE event format (`turn-added`, `turn-updated`, `liveness`): Tasks 6 (events) + 7 (SSE wire-up).
- Dashboard grouping by source: Task 9 (frontend).
- Session page with live updates: Task 9.
- Tailer with shared broadcast + refcount + self-cleanup: Task 6.
- Static asset embedding via `include_str!`: Task 4.
- HTML rendering as thin shells, JS-driven: Tasks 4, 8, 9.
- JSON serialization (Serialize derives + WireSession + WireLiveness): Tasks 1 and 3.
- Shutdown semantics (oneshot signal, graceful close): Task 2.
- Path-traversal protection: Task 5.
- Tests: Tasks 2-8 inline tests + Task 13 integration tests.

**Placeholder scan:**
- No "TBD" / "TODO" markers.
- Task 11 acknowledges potential `o` / `y` keybind conflicts with explicit fallback instructions; that's a context-dependent step, not a placeholder.
- Task 9 says "for v1, naive: re-render the entire view" for `turn-updated` handling — that's an explicit deferred-optimization choice, documented in code.

**Type consistency:**
- `WebState`, `WebServerHandle` — same shape across Tasks 2, 10, 13.
- `WireSession`, `WireSource`, `WireLiveness` — defined in Task 3; serialized identically in Tasks 3, 9, 13.
- `TailEvent { TurnAdded { idx }, TurnUpdated { idx }, LivenessChanged { state } }` — defined in Task 6; consumed by Task 7 SSE handler and Task 9 frontend.
- `TailerRegistry` API: `default()`, `ensure(id, source, jsonl)`, `contains(id)` — same across Tasks 2 (stub), 6 (implementation), 13 (test).
- `Tailer` API: `subscribe()` returns `broadcast::Receiver`, `release()` decrements refcount. Used in Tasks 6 (impl), 7 (handler), 13 indirectly.

**Branch baseline:** the plan assumes `feat/in-progress-indicator` is the branch base (for `liveness::Liveness`, `liveness::CachedLiveness`, and `AppState.liveness_cache`). If executing on a fresh `master` instead, the implementer must merge or rebase first; this is documented at the top of the plan.

**Out-of-scope follow-ups documented in the spec are NOT part of this plan:**
- Cargo feature flag for `web` (always-on for v1).
- Markdown / syntax highlighting.
- Mobile layout polish.
- Auth.
- Persistent server.
- WebSocket upgrade for browser→server actions.
