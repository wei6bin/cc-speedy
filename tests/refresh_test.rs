use cc_speedy::refresh::{compute_refresh_diff, select_index_for_session_id, RefreshResult};
use cc_speedy::unified::{SessionSource, UnifiedSession};
use std::time::{Duration, UNIX_EPOCH};

fn make_session(id: &str, modified_secs: u64) -> UnifiedSession {
    UnifiedSession {
        session_id: id.to_string(),
        project_name: "p".to_string(),
        project_path: "/tmp/p".to_string(),
        modified: UNIX_EPOCH + Duration::from_secs(modified_secs),
        message_count: 0,
        first_user_msg: String::new(),
        summary: String::new(),
        git_branch: String::new(),
        source: SessionSource::ClaudeCode,
        jsonl_path: None,
        archived: false,
    }
}

#[test]
fn refresh_result_carries_sessions_and_counts() {
    let prior = vec![make_session("a", 100)];
    let new = vec![make_session("a", 150), make_session("b", 200)];
    let r: RefreshResult = compute_refresh_diff(&prior, new);
    assert_eq!(r.sessions.len(), 2);
    assert_eq!(r.new_count, 1);
    assert_eq!(r.updated_count, 1);
}

#[test]
fn selection_preserved_across_diff() {
    let prior = vec![make_session("a", 100), make_session("b", 200)];
    let r = compute_refresh_diff(
        &prior,
        vec![
            make_session("a", 100),
            make_session("b", 250),
            make_session("c", 300),
        ],
    );
    let filtered: Vec<usize> = (0..r.sessions.len()).collect();
    let pos = select_index_for_session_id(&filtered, &r.sessions, Some("b"));
    assert_eq!(pos, Some(1));
}

#[test]
fn selection_falls_back_when_session_removed() {
    let new = vec![make_session("a", 100), make_session("b", 200)];
    let filtered: Vec<usize> = (0..new.len()).collect();
    let pos = select_index_for_session_id(&filtered, &new, Some("lost"));
    assert_eq!(pos, Some(0));
}

#[test]
fn selection_none_on_empty_list() {
    let new: Vec<UnifiedSession> = vec![];
    let filtered: Vec<usize> = vec![];
    let pos = select_index_for_session_id(&filtered, &new, Some("anything"));
    assert_eq!(pos, None);
}
