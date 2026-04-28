//! Route handlers. Each handler is a free async fn; axum extracts the
//! shared [`super::WebState`] from the router.

use crate::liveness::Liveness;
use crate::turn_detail::TurnDetail;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::Json;

pub async fn health() -> &'static str {
    "ok"
}

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
            (
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            ),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        super::assets::APP_JS,
    )
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

/// `GET /api/session/:id/turns/:idx` — single turn payload.
/// Validates the session id against the in-memory list before any
/// path is touched (path-traversal protection).
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

use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::Stream;
use std::convert::Infallible;

/// `GET /session/{id}/stream` — SSE stream of `TailEvent`s.
pub async fn sse_stream(
    State(state): State<super::WebState>,
    Path(session_id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    use tokio_stream::StreamExt;

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
    let rx = tailer.subscribe(); // bumps refcount; receiver matched by Drop in WithDropRelease

    let stream = tokio_stream::wrappers::BroadcastStream::new(rx).map(|item| match item {
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
    });

    let stream = WithDropRelease::new(stream, tailer);
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
