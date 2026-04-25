use cc_speedy::tui::build_project_rows;
use cc_speedy::unified::{SessionSource, UnifiedSession};
use std::collections::HashSet;
use std::time::{Duration, UNIX_EPOCH};

fn mk(id: &str, path: &str, secs: u64) -> UnifiedSession {
    UnifiedSession {
        session_id: id.to_string(),
        project_path: path.to_string(),
        project_name: path.rsplit('/').next().unwrap_or(path).to_string(),
        summary: format!("title for {}", id),
        modified: UNIX_EPOCH + Duration::from_secs(secs),
        message_count: 1,
        source: SessionSource::ClaudeCode,
        jsonl_path: None,
        git_branch: String::new(),
        first_user_msg: String::new(),
        archived: false,
    }
}

#[test]
fn test_build_groups_by_project_path() {
    let sessions = vec![
        mk("a1", "/repo/alpha", 100),
        mk("a2", "/repo/alpha", 200),
        mk("b1", "/repo/beta", 150),
    ];
    let pinned = HashSet::new();
    let rows = build_project_rows(&sessions, &pinned);
    assert_eq!(rows.len(), 2);
    let alpha = rows
        .iter()
        .find(|r| r.project_path == "/repo/alpha")
        .unwrap();
    assert_eq!(alpha.session_count, 2);
    assert_eq!(alpha.last_active, UNIX_EPOCH + Duration::from_secs(200));
    let beta = rows
        .iter()
        .find(|r| r.project_path == "/repo/beta")
        .unwrap();
    assert_eq!(beta.session_count, 1);
    assert_eq!(beta.last_active, UNIX_EPOCH + Duration::from_secs(150));
}

#[test]
fn test_last_active_is_max_of_group() {
    let sessions = vec![
        mk("s1", "/p", 100),
        mk("s2", "/p", 50),
        mk("s3", "/p", 200),
        mk("s4", "/p", 10),
    ];
    let rows = build_project_rows(&sessions, &HashSet::new());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].last_active, UNIX_EPOCH + Duration::from_secs(200));
}

#[test]
fn test_pinned_count() {
    let sessions = vec![
        mk("s1", "/p", 100),
        mk("s2", "/p", 200),
        mk("s3", "/p", 300),
    ];
    let pinned: HashSet<String> = ["s1".to_string(), "s3".to_string()].into_iter().collect();
    let rows = build_project_rows(&sessions, &pinned);
    assert_eq!(rows[0].pinned_count, 2);
}

#[test]
fn test_empty_input_returns_empty() {
    let rows = build_project_rows(&[], &HashSet::new());
    assert!(rows.is_empty());
}

#[test]
fn test_name_is_last_two_path_segments() {
    let sessions = vec![mk("s1", "/home/user/code/my-repo", 1)];
    let rows = build_project_rows(&sessions, &HashSet::new());
    assert_eq!(rows[0].name, "code/my-repo");
}
