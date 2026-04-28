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
