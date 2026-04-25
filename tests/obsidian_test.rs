use cc_speedy::obsidian::export_to_obsidian;
use cc_speedy::obsidian::parse_status_from_factual;
use cc_speedy::store::LearningPoint;
use cc_speedy::unified::{SessionSource, UnifiedSession};
use std::time::{Duration, UNIX_EPOCH};
use tempfile::TempDir;

fn make_session(msg_count: usize) -> UnifiedSession {
    UnifiedSession {
        session_id: "abc12345-test".to_string(),
        project_name: "cc-speedy".to_string(),
        project_path: "/home/user/ai/cc-speedy".to_string(),
        modified: UNIX_EPOCH + Duration::from_secs(1_700_000_000),
        message_count: msg_count,
        first_user_msg: "hello".to_string(),
        summary: "Fix the bug".to_string(),
        git_branch: "main".to_string(),
        source: SessionSource::ClaudeCode,
        jsonl_path: None,
        archived: false,
    }
}

#[test]
fn test_export_writes_markdown_file() {
    let tmp = TempDir::new().unwrap();
    let session = make_session(10);
    let learnings = vec![
        LearningPoint {
            category: "decision_points".to_string(),
            point: "used tokio::spawn".to_string(),
        },
        LearningPoint {
            category: "lessons_gotchas".to_string(),
            point: "watch lock order".to_string(),
        },
    ];
    export_to_obsidian(
        &session,
        "## What was done\n- fixed bug",
        &learnings,
        tmp.path().to_str().unwrap(),
    )
    .unwrap();

    let files: Vec<_> = std::fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(files.len(), 1);
    let content = std::fs::read_to_string(files[0].path()).unwrap();
    assert!(content.contains("session_id: \"abc12345-test\""));
    assert!(content.contains("project: \"/home/user/ai/cc-speedy\""));
    assert!(content.contains("tags: [agent-session]"));
    assert!(content.contains("## What was done"));
    assert!(content.contains("## Decision points"));
    assert!(content.contains("used tokio::spawn"));
    assert!(content.contains("## Lessons & gotchas"));
    assert!(content.contains("watch lock order"));
}

#[test]
fn test_export_skips_sessions_with_few_messages() {
    let tmp = TempDir::new().unwrap();
    let session = make_session(3);
    export_to_obsidian(&session, "summary", &[], tmp.path().to_str().unwrap()).unwrap();
    let files: Vec<_> = std::fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert!(
        files.is_empty(),
        "should not write file for session with < 5 messages"
    );
}

#[test]
fn test_export_filename_format() {
    let tmp = TempDir::new().unwrap();
    let session = make_session(10);
    export_to_obsidian(&session, "summary", &[], tmp.path().to_str().unwrap()).unwrap();
    let files: Vec<_> = std::fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(files.len(), 1);
    let name = files[0].file_name();
    let name_str = name.to_string_lossy();
    assert!(name_str.ends_with(".md"));
    assert!(
        name_str.contains("abc1234"),
        "should contain first 8 chars of session_id: {}",
        name_str
    );
    assert!(
        name_str.contains("ai-cc-speedy"),
        "should contain project slug: {}",
        name_str
    );
}

#[test]
fn test_export_overwrites_existing_file() {
    let tmp = TempDir::new().unwrap();
    let session = make_session(10);
    export_to_obsidian(&session, "old content", &[], tmp.path().to_str().unwrap()).unwrap();
    export_to_obsidian(&session, "new content", &[], tmp.path().to_str().unwrap()).unwrap();
    let files: Vec<_> = std::fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(files.len(), 1, "should not create two files on re-export");
    let content = std::fs::read_to_string(files[0].path()).unwrap();
    assert!(content.contains("new content"));
    assert!(!content.contains("old content"));
}

#[test]
fn test_parse_status_completed() {
    let body = "## What was done\n- x\n\n## Status\nCompleted\n\n## Approach\n";
    assert_eq!(parse_status_from_factual(body), "completed");
}

#[test]
fn test_parse_status_in_progress_two_words() {
    let body = "## Status\nIn progress\n";
    assert_eq!(parse_status_from_factual(body), "in_progress");
}

#[test]
fn test_parse_status_missing_returns_unknown() {
    let body = "## What was done\n- only this\n";
    assert_eq!(parse_status_from_factual(body), "unknown");
}

#[test]
fn test_parse_status_extra_whitespace() {
    let body = "## Status\n  Completed   \n";
    assert_eq!(parse_status_from_factual(body), "completed");
}

#[test]
fn test_parse_status_unrecognised_value() {
    let body = "## Status\nBlocked on infra\n";
    assert_eq!(parse_status_from_factual(body), "unknown");
}
