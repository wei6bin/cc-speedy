use cc_speedy::copilot_sessions::{parse_copilot_messages_from_path, list_copilot_sessions_from_dir};
use tempfile::TempDir;
use std::fs;

fn make_session(base: &TempDir, id: &str, yaml: &str, jsonl: &str) {
    let dir = base.path().join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("workspace.yaml"), yaml).unwrap();
    fs::write(dir.join("events.jsonl"), jsonl).unwrap();
}

const FOUR_MSGS: &str = concat!(
    "{\"type\":\"user.message\",\"data\":{\"content\":\"q1\"}}\n",
    "{\"type\":\"assistant.message\",\"data\":{\"content\":\"a1\"}}\n",
    "{\"type\":\"user.message\",\"data\":{\"content\":\"q2\"}}\n",
    "{\"type\":\"assistant.message\",\"data\":{\"content\":\"a2\"}}\n",
);

const THREE_MSGS: &str = concat!(
    "{\"type\":\"user.message\",\"data\":{\"content\":\"q1\"}}\n",
    "{\"type\":\"assistant.message\",\"data\":{\"content\":\"a1\"}}\n",
    "{\"type\":\"user.message\",\"data\":{\"content\":\"q2\"}}\n",
);

#[test]
fn test_list_sessions_filters_under_4_messages() {
    let tmp = TempDir::new().unwrap();
    make_session(&tmp, "sess-a",
        "id: sess-a\ncwd: /home/user/proj\nupdated_at: 2026-01-01T00:00:00Z\n",
        THREE_MSGS,
    );
    make_session(&tmp, "sess-b",
        "id: sess-b\ncwd: /home/user/proj2\nupdated_at: 2026-01-02T00:00:00Z\n",
        FOUR_MSGS,
    );
    let sessions = list_copilot_sessions_from_dir(tmp.path()).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_id, "sess-b");
}

#[test]
fn test_list_sessions_skips_dirs_without_workspace_yaml() {
    let tmp = TempDir::new().unwrap();
    // Dir without workspace.yaml (old format)
    let legacy = tmp.path().join("legacy-session");
    fs::create_dir_all(&legacy).unwrap();
    fs::write(legacy.join("events.jsonl"), FOUR_MSGS).unwrap();
    // Dir with workspace.yaml
    make_session(&tmp, "valid-session",
        "id: valid-session\ncwd: /home/user/proj\nupdated_at: 2026-01-01T00:00:00Z\n",
        FOUR_MSGS,
    );
    let sessions = list_copilot_sessions_from_dir(tmp.path()).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_id, "valid-session");
}

#[test]
fn test_session_title_name_takes_priority_over_summary() {
    let tmp = TempDir::new().unwrap();
    make_session(&tmp, "sess-x",
        "id: sess-x\ncwd: /home/user/proj\nupdated_at: 2026-01-01T00:00:00Z\nname: my-name\nsummary: my-summary\n",
        FOUR_MSGS,
    );
    let sessions = list_copilot_sessions_from_dir(tmp.path()).unwrap();
    assert_eq!(sessions[0].summary, "my-name");
}

#[test]
fn test_session_title_falls_back_to_summary() {
    let tmp = TempDir::new().unwrap();
    make_session(&tmp, "sess-y",
        "id: sess-y\ncwd: /home/user/proj\nupdated_at: 2026-01-01T00:00:00Z\nsummary: my-summary\n",
        FOUR_MSGS,
    );
    let sessions = list_copilot_sessions_from_dir(tmp.path()).unwrap();
    assert_eq!(sessions[0].summary, "my-summary");
}

#[test]
fn test_session_git_branch_extracted() {
    let tmp = TempDir::new().unwrap();
    make_session(&tmp, "sess-z",
        "id: sess-z\ncwd: /home/user/repo\nupdated_at: 2026-01-01T00:00:00Z\nbranch: feature-x\n",
        FOUR_MSGS,
    );
    let sessions = list_copilot_sessions_from_dir(tmp.path()).unwrap();
    assert_eq!(sessions[0].git_branch, "feature-x");
}

#[test]
fn test_session_first_user_message_truncated_to_80_chars() {
    let tmp = TempDir::new().unwrap();
    let long_msg = "a".repeat(200);
    let jsonl = format!(
        "{{\"type\":\"user.message\",\"data\":{{\"content\":\"{}\"}}}}\n\
         {{\"type\":\"assistant.message\",\"data\":{{\"content\":\"a1\"}}}}\n\
         {{\"type\":\"user.message\",\"data\":{{\"content\":\"q2\"}}}}\n\
         {{\"type\":\"assistant.message\",\"data\":{{\"content\":\"a2\"}}}}\n",
        long_msg
    );
    make_session(&tmp, "sess-long",
        "id: sess-long\ncwd: /home/user/proj\nupdated_at: 2026-01-01T00:00:00Z\n",
        &jsonl,
    );
    let sessions = list_copilot_sessions_from_dir(tmp.path()).unwrap();
    assert_eq!(sessions[0].first_user_msg.len(), 80);
}

#[test]
fn test_list_returns_empty_for_nonexistent_dir() {
    let result = list_copilot_sessions_from_dir(std::path::Path::new("/nonexistent/path/xyz"));
    assert!(result.unwrap().is_empty());
}

#[test]
fn test_parse_messages_user_and_assistant() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("events.jsonl");
    fs::write(&path, concat!(
        "{\"type\":\"session.start\",\"data\":{}}\n",
        "{\"type\":\"user.message\",\"data\":{\"content\":\"Hello\"}}\n",
        "{\"type\":\"assistant.message\",\"data\":{\"content\":\"Hi there\"}}\n",
        "{\"type\":\"tool.execution_start\",\"data\":{}}\n",
    )).unwrap();
    let msgs = parse_copilot_messages_from_path(&path).unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].role, "user");
    assert_eq!(msgs[0].text, "Hello");
    assert_eq!(msgs[1].role, "assistant");
    assert_eq!(msgs[1].text, "Hi there");
}

#[test]
fn test_parse_messages_skips_empty_assistant_content() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("events.jsonl");
    fs::write(&path, concat!(
        "{\"type\":\"user.message\",\"data\":{\"content\":\"query\"}}\n",
        "{\"type\":\"assistant.message\",\"data\":{\"content\":\"\"}}\n",
        "{\"type\":\"assistant.message\",\"data\":{\"content\":\"answer\"}}\n",
    )).unwrap();
    let msgs = parse_copilot_messages_from_path(&path).unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[1].text, "answer");
}

#[test]
fn test_parse_messages_skips_non_message_events() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("events.jsonl");
    fs::write(&path, concat!(
        "{\"type\":\"session.start\",\"data\":{}}\n",
        "{\"type\":\"assistant.turn_start\",\"data\":{}}\n",
        "{\"type\":\"user.message\",\"data\":{\"content\":\"only msg\"}}\n",
        "{\"type\":\"tool.execution_complete\",\"data\":{}}\n",
    )).unwrap();
    let msgs = parse_copilot_messages_from_path(&path).unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].text, "only msg");
}
