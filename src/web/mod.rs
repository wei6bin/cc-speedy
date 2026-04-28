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

pub mod assets;
pub mod handlers;
pub mod tailer;
pub mod wire;

#[derive(Clone)]
pub struct WebState {
    pub sessions: Arc<std::sync::Mutex<Vec<crate::unified::UnifiedSession>>>,
    pub liveness_cache:
        Arc<std::sync::Mutex<std::collections::HashMap<String, crate::liveness::CachedLiveness>>>,
    pub tailer_registry: tailer::TailerRegistry,
}

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
        drop(self.join);
    }
}

const DEFAULT_PORT: u16 = 7457;

pub async fn start(state: WebState) -> Result<WebServerHandle> {
    let app = build_router(state);
    let preferred: SocketAddr = ([127, 0, 0, 1], DEFAULT_PORT).into();
    let listener = match tokio::net::TcpListener::bind(preferred).await {
        Ok(l) => l,
        Err(_) => {
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
        .route("/", get(handlers::dashboard))
        .route("/health", get(handlers::health))
        .route("/api/sessions", get(handlers::api_sessions))
        .route("/api/session/{id}/turns/{idx}", get(handlers::api_turn))
        .route("/static/app.css", get(handlers::static_app_css))
        .route("/static/app.js", get(handlers::static_app_js))
        .with_state(state)
}

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
        let blocker_addr: SocketAddr = ([127, 0, 0, 1], DEFAULT_PORT).into();
        let blocker = tokio::net::TcpListener::bind(blocker_addr).await.ok();
        let handle = start(empty_state()).await.unwrap();
        if blocker.is_some() {
            assert_ne!(handle.addr.port(), DEFAULT_PORT);
        }
        handle.shutdown();
        drop(blocker);
    }

    #[tokio::test]
    async fn dashboard_returns_html() {
        let handle = start(empty_state()).await.unwrap();
        let url = format!("http://{}/", handle.addr);
        let resp = reqwest::get(&url).await.unwrap();
        assert_eq!(resp.status(), 200);
        let ct = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
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
        assert!(css
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("text/css"));
        let js = reqwest::get(&js_url).await.unwrap();
        assert_eq!(js.status(), 200);
        assert!(js
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("application/javascript"));
        handle.shutdown();
    }

    #[tokio::test]
    async fn api_sessions_returns_json_with_projected_shape() {
        use crate::unified::{SessionSource, UnifiedSession};
        use std::time::{Duration, UNIX_EPOCH};

        let sessions = vec![UnifiedSession {
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
        }];
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
        // URL-encoded `../../etc/passwd` as a session id; the handler
        // looks up the id in the in-memory session list before resolving
        // any path, so this can never reach `Path::new`.
        let url = format!(
            "http://{}/api/session/..%2F..%2Fetc%2Fpasswd/turns/0",
            handle.addr
        );
        let resp = reqwest::get(&url).await.unwrap();
        assert!(resp.status().is_client_error());
        handle.shutdown();
    }
}
