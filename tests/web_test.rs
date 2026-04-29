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
