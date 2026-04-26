use cc_speedy::tui::build_project_rows;
use cc_speedy::unified::{SessionSource, UnifiedSession};
use std::collections::{HashMap, HashSet};
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
    let rows = build_project_rows(&sessions, &pinned, &HashSet::new(), &HashMap::new(), None);
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
    let rows = build_project_rows(
        &sessions,
        &HashSet::new(),
        &HashSet::new(),
        &HashMap::new(),
        None,
    );
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
    let rows = build_project_rows(&sessions, &pinned, &HashSet::new(), &HashMap::new(), None);
    assert_eq!(rows[0].pinned_count, 2);
}

#[test]
fn test_empty_input_returns_empty() {
    let rows = build_project_rows(&[], &HashSet::new(), &HashSet::new(), &HashMap::new(), None);
    assert!(rows.is_empty());
}

#[test]
fn test_name_is_last_two_path_segments() {
    let sessions = vec![mk("s1", "/home/user/code/my-repo", 1)];
    let rows = build_project_rows(
        &sessions,
        &HashSet::new(),
        &HashSet::new(),
        &HashMap::new(),
        None,
    );
    assert_eq!(rows[0].name, "code/my-repo");
}

#[test]
fn test_learnings_count() {
    let sessions = vec![
        mk("s1", "/repo/alpha", 100),
        mk("s2", "/repo/alpha", 200),
        mk("s3", "/repo/alpha", 300),
        mk("s4", "/repo/beta", 100),
    ];
    let has_learnings: HashSet<String> = ["s1".into(), "s3".into(), "s4".into()]
        .into_iter()
        .collect();
    let rows = build_project_rows(
        &sessions,
        &HashSet::new(),
        &has_learnings,
        &HashMap::new(),
        None,
    );
    let alpha = rows
        .iter()
        .find(|r| r.project_path == "/repo/alpha")
        .unwrap();
    let beta = rows
        .iter()
        .find(|r| r.project_path == "/repo/beta")
        .unwrap();
    assert_eq!(alpha.learnings_count, 2);
    assert_eq!(beta.learnings_count, 1);
}

#[test]
fn test_pending_count_counts_sessions_without_summary() {
    let sessions = vec![
        mk("s1", "/repo/alpha", 100),
        mk("s2", "/repo/alpha", 200),
        mk("s3", "/repo/alpha", 300),
    ];
    let mut summaries: HashMap<String, String> = HashMap::new();
    summaries.insert("s1".into(), "summary text".into());
    let rows = build_project_rows(
        &sessions,
        &HashSet::new(),
        &HashSet::new(),
        &summaries,
        None,
    );
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].pending_count, 2);
}

fn mk_with_source(id: &str, path: &str, secs: u64, source: SessionSource) -> UnifiedSession {
    let mut s = mk(id, path, secs);
    s.source = source;
    s
}

#[test]
fn test_source_filter_drops_projects_with_no_matching_sessions() {
    let sessions = vec![
        mk_with_source("a1", "/repo/alpha", 100, SessionSource::ClaudeCode),
        mk_with_source("a2", "/repo/alpha", 200, SessionSource::OpenCode),
        mk_with_source("b1", "/repo/beta", 100, SessionSource::OpenCode),
        mk_with_source("c1", "/repo/gamma", 100, SessionSource::Copilot),
    ];
    let rows = build_project_rows(
        &sessions,
        &HashSet::new(),
        &HashSet::new(),
        &HashMap::new(),
        Some(SessionSource::ClaudeCode),
    );
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].project_path, "/repo/alpha");
    assert_eq!(rows[0].session_count, 1);
}

#[test]
fn test_source_filter_scopes_counts() {
    let sessions = vec![
        mk_with_source("a1", "/repo/alpha", 100, SessionSource::ClaudeCode),
        mk_with_source("a2", "/repo/alpha", 200, SessionSource::ClaudeCode),
        mk_with_source("a3", "/repo/alpha", 300, SessionSource::ClaudeCode),
        mk_with_source("a4", "/repo/alpha", 400, SessionSource::OpenCode),
    ];
    // pinned: a1 (CC) and a4 (OC). Only a1 should count under CC filter.
    let pinned: HashSet<String> = ["a1".into(), "a4".into()].into_iter().collect();
    // learnings: a2 (CC) and a4 (OC). Only a2 should count under CC filter.
    let has_learnings: HashSet<String> = ["a2".into(), "a4".into()].into_iter().collect();
    // summaries: a1 (CC) and a4 (OC) summarized. Under CC filter, only a2/a3 are unsummarized → pending_count == 2.
    let mut summaries: HashMap<String, String> = HashMap::new();
    summaries.insert("a1".into(), "ok".into());
    summaries.insert("a4".into(), "ok".into());

    let rows = build_project_rows(
        &sessions,
        &pinned,
        &has_learnings,
        &summaries,
        Some(SessionSource::ClaudeCode),
    );
    assert_eq!(rows.len(), 1);
    let r = &rows[0];
    assert_eq!(r.session_count, 3); // a1, a2, a3 — not a4
    assert_eq!(r.pinned_count, 1); // a1 only
    assert_eq!(r.learnings_count, 1); // a2 only
    assert_eq!(r.pending_count, 2); // a2, a3 (a1 has summary, a4 wrong source)
}
