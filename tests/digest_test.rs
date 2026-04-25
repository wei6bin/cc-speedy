use cc_speedy::digest::{build_digest, render_digest, LearningWithSession};
use cc_speedy::unified::{SessionSource, UnifiedSession};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn mk_session(id: &str, path: &str, secs_ago: u64, now: SystemTime) -> UnifiedSession {
    UnifiedSession {
        session_id: id.to_string(),
        project_path: path.to_string(),
        project_name: path.rsplit('/').next().unwrap_or(path).to_string(),
        summary: format!("title-{}", id),
        modified: now - Duration::from_secs(secs_ago),
        message_count: 5,
        source: SessionSource::ClaudeCode,
        jsonl_path: None,
        git_branch: String::new(),
        first_user_msg: String::new(),
        archived: false,
    }
}

fn learning(sid: &str, cat: &str, point: &str, captured_at: i64) -> LearningWithSession {
    LearningWithSession {
        session_id: sid.to_string(),
        category: cat.to_string(),
        point: point.to_string(),
        captured_at,
    }
}

fn now_fixed() -> SystemTime {
    // 2026-04-23 00:00 UTC (seconds since epoch)
    UNIX_EPOCH + Duration::from_secs(1777200000)
}

#[test]
fn test_session_in_window_counted() {
    let now = now_fixed();
    let sessions = vec![mk_session("s1", "/p/a", 86400, now)]; // 1 day ago
    let d = build_digest(&sessions, &[], 7, now);
    assert_eq!(d.session_count, 1);
    assert_eq!(d.projects.len(), 1);
}

#[test]
fn test_session_outside_window_excluded() {
    let now = now_fixed();
    let sessions = vec![
        mk_session("recent", "/p/a", 86400, now),   // 1 day ago — in
        mk_session("old", "/p/b", 10 * 86400, now), // 10 days ago — out
    ];
    let d = build_digest(&sessions, &[], 7, now);
    assert_eq!(d.session_count, 1);
    assert_eq!(d.projects.len(), 1);
    assert_eq!(d.projects[0].project_path, "/p/a");
}

#[test]
fn test_projects_sorted_by_last_active_desc() {
    let now = now_fixed();
    let sessions = vec![
        mk_session("a1", "/p/alpha", 3 * 86400, now),
        mk_session("b1", "/p/beta", 1 * 86400, now),
        mk_session("c1", "/p/gamma", 2 * 86400, now),
    ];
    let d = build_digest(&sessions, &[], 7, now);
    let names: Vec<&str> = d.projects.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["p/beta", "p/gamma", "p/alpha"]);
}

#[test]
fn test_learnings_filtered_by_captured_at() {
    let now = now_fixed();
    let now_secs = now.duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
    let sessions = vec![mk_session("s1", "/p/a", 86400, now)];
    let learnings = vec![
        learning("s1", "decision_points", "recent", now_secs - 86400), // in
        learning("s1", "lessons_gotchas", "old", now_secs - 10 * 86400), // out
    ];
    let d = build_digest(&sessions, &learnings, 7, now);
    assert_eq!(d.learning_count, 1);
    assert_eq!(d.learnings[0].point, "recent");
}

#[test]
fn test_empty_window_renders_no_activity() {
    let now = now_fixed();
    let d = build_digest(&[], &[], 7, now);
    let text = render_digest(&d);
    assert!(text.contains("(No activity in this window."));
}

#[test]
fn test_render_contains_header_and_sections() {
    let now = now_fixed();
    let now_secs = now.duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
    let sessions = vec![mk_session("s1", "/p/alpha", 86400, now)];
    let learnings = vec![learning(
        "s1",
        "decision_points",
        "pick postgres",
        now_secs - 86400,
    )];
    let d = build_digest(&sessions, &learnings, 7, now);
    let text = render_digest(&d);
    assert!(text.contains("Weekly Digest"));
    assert!(text.contains("By project"));
    assert!(text.contains("p/alpha"));
    assert!(text.contains("Learnings captured"));
    assert!(text.contains("pick postgres"));
    assert!(text.contains("[DEC]"));
}
