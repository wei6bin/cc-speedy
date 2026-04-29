use cc_speedy::opencode_sessions::{opencode_db_path, parse_opencode_messages_from_conn};

#[test]
fn test_opencode_db_path_ends_with_db_file() {
    if let Some(p) = opencode_db_path() {
        let s = p.to_string_lossy();
        assert!(
            s.ends_with("opencode.db"),
            "expected path to end with opencode.db, got: {}",
            s
        );
        assert!(
            s.contains("opencode"),
            "expected path to contain 'opencode': {}",
            s
        );
    }
    // If None: opencode not installed; that's acceptable — test passes
}

use cc_speedy::opencode_sessions::query_sessions_from_conn;
use rusqlite::Connection;

fn setup_fixture_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "
        CREATE TABLE project (
            id TEXT PRIMARY KEY,
            worktree TEXT NOT NULL,
            time_created INTEGER,
            time_updated INTEGER
        );
        CREATE TABLE session (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            parent_id TEXT,
            title TEXT,
            time_updated INTEGER NOT NULL,
            time_archived INTEGER,
            summary_diffs TEXT
        );
        CREATE TABLE message (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            time_created INTEGER,
            data TEXT
        );
        CREATE TABLE part (
            id TEXT PRIMARY KEY,
            message_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            time_created INTEGER,
            data TEXT
        );

        INSERT INTO project VALUES ('proj1', '/home/user/ai/myproj', 1000, 2000);

        -- top-level session (should appear)
        INSERT INTO session VALUES (
            'ses_aaa', 'proj1', NULL, 'my title',
            1741600000000, NULL, NULL
        );
        -- sub-agent session (parent_id set — should be filtered out)
        INSERT INTO session VALUES (
            'ses_bbb', 'proj1', 'ses_aaa', 'subagent',
            1741600001000, NULL, NULL
        );
        -- archived session (should be filtered out)
        INSERT INTO session VALUES (
            'ses_ccc', 'proj1', NULL, 'old',
            1741599000000, 1741600000000, NULL
        );

        INSERT INTO message VALUES ('msg1', 'ses_aaa', 1741600000001, '{\"role\":\"user\"}');
        INSERT INTO message VALUES ('msg2', 'ses_aaa', 1741600000002, '{\"role\":\"assistant\"}');

        INSERT INTO part VALUES (
            'prt1', 'msg1', 'ses_aaa', 1741600000001,
            '{\"type\":\"text\",\"text\":\"help me write tests\"}'
        );
        INSERT INTO part VALUES (
            'prt2', 'msg2', 'ses_aaa', 1741600000002,
            '{\"type\":\"text\",\"text\":\"Sure, here are some tests.\"}'
        );
    ",
    )
    .unwrap();
    conn
}

#[test]
fn test_query_returns_top_level_sessions_only() {
    let conn = setup_fixture_db();
    let sessions = query_sessions_from_conn(&conn).unwrap();
    assert_eq!(
        sessions.len(),
        1,
        "expected 1 session, got: {:?}",
        sessions.iter().map(|s| &s.session_id).collect::<Vec<_>>()
    );
    assert_eq!(sessions[0].session_id, "ses_aaa");
}

#[test]
fn test_query_session_title_and_project_path() {
    let conn = setup_fixture_db();
    let sessions = query_sessions_from_conn(&conn).unwrap();
    assert_eq!(sessions[0].summary, "my title");
    assert_eq!(sessions[0].project_path, "/home/user/ai/myproj");
}

#[test]
fn test_query_message_count() {
    let conn = setup_fixture_db();
    let sessions = query_sessions_from_conn(&conn).unwrap();
    assert_eq!(sessions[0].message_count, 2);
}

#[test]
fn test_query_first_user_msg_extracted_from_parts() {
    let conn = setup_fixture_db();
    let sessions = query_sessions_from_conn(&conn).unwrap();
    assert_eq!(sessions[0].first_user_msg, "help me write tests");
}

#[test]
fn test_parse_opencode_messages_returns_role_and_text() {
    let conn = setup_fixture_db();
    let messages = parse_opencode_messages_from_conn(&conn, "ses_aaa").unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].role, "user");
    assert_eq!(messages[0].text, "help me write tests");
    assert_eq!(messages[1].role, "assistant");
    assert_eq!(messages[1].text, "Sure, here are some tests.");
}

#[test]
fn test_parse_opencode_messages_empty_for_unknown_session() {
    let conn = setup_fixture_db();
    let messages = parse_opencode_messages_from_conn(&conn, "no-such-session").unwrap();
    assert!(messages.is_empty());
}

#[test]
fn test_parse_opencode_messages_skips_non_text_parts() {
    let conn = setup_fixture_db();
    // Insert a tool-use part that should be ignored
    conn.execute(
        "INSERT INTO part VALUES ('prt3', 'msg1', 'ses_aaa', 1741600000003, '{\"type\":\"tool-use\",\"text\":\"ignored\"}')",
        [],
    ).unwrap();
    let messages = parse_opencode_messages_from_conn(&conn, "ses_aaa").unwrap();
    // Still only 2 text messages; tool-use part skipped
    assert_eq!(messages.len(), 2);
}

#[test]
fn test_incremental_empty_prior_matches_non_incremental() {
    use cc_speedy::opencode_sessions::query_sessions_from_conn_incremental;
    use cc_speedy::unified::PriorById;
    use std::collections::HashMap;
    let conn = setup_fixture_db();
    let baseline = query_sessions_from_conn(&conn).unwrap();
    let map: PriorById = HashMap::new();
    let incr = query_sessions_from_conn_incremental(&conn, &map).unwrap();
    assert_eq!(incr.len(), baseline.len());
    assert_eq!(incr[0].session_id, baseline[0].session_id);
    assert_eq!(incr[0].first_user_msg, baseline[0].first_user_msg);
}

#[test]
fn test_incremental_reuses_prior_when_mtime_matches() {
    // The decisive evidence: the prior carries a marker first_user_msg that
    // does NOT match what query_first_user_text would return. If the
    // incremental path reused prior, the marker is preserved; if it re-
    // queried, the marker would be replaced with "help me write tests".
    use cc_speedy::opencode_sessions::query_sessions_from_conn_incremental;
    use cc_speedy::unified::{PriorById, SessionSource, UnifiedSession};
    use std::time::{Duration, UNIX_EPOCH};
    let conn = setup_fixture_db();
    let baseline = query_sessions_from_conn(&conn).unwrap();
    assert_eq!(baseline.len(), 1);

    let prior = vec![UnifiedSession {
        session_id: "ses_aaa".to_string(),
        project_name: "ai/myproj".to_string(),
        project_path: "/home/user/ai/myproj".to_string(),
        modified: UNIX_EPOCH + Duration::from_millis(1741600000000),
        message_count: 2,
        first_user_msg: "MARKER_FROM_PRIOR".to_string(),
        summary: "my title".to_string(),
        git_branch: String::new(),
        source: SessionSource::OpenCode,
        jsonl_path: None,
        archived: false,
    }];
    let map: PriorById = prior.iter().map(|s| (s.session_id.as_str(), s)).collect();
    let incr = query_sessions_from_conn_incremental(&conn, &map).unwrap();
    assert_eq!(incr.len(), 1);
    assert_eq!(
        incr[0].first_user_msg, "MARKER_FROM_PRIOR",
        "incremental path should have reused the prior session and skipped query_first_user_text"
    );
}
