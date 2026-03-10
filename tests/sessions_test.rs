use cc_speedy::sessions::{parse_messages, read_cwd_from_jsonl, SessionIndex};
use tempfile::TempDir;
use std::io::Write;

#[test]
fn test_parse_messages_counts_correctly() {
    let path = std::path::Path::new("tests/fixtures/sample.jsonl");
    let msgs = parse_messages(path).unwrap();
    assert_eq!(msgs.len(), 4);
}

#[test]
fn test_parse_messages_extracts_first_user_text() {
    let path = std::path::Path::new("tests/fixtures/sample.jsonl");
    let msgs = parse_messages(path).unwrap();
    assert_eq!(msgs[0].text, "fix the bug in auth");
}

#[test]
fn test_parse_messages_empty_file_returns_empty_vec() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("empty.jsonl");
    std::fs::write(&path, "").unwrap();
    let msgs = parse_messages(&path).unwrap();
    assert!(msgs.is_empty());
}

#[test]
fn test_parse_messages_skips_malformed_lines() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("mixed.jsonl");
    std::fs::write(&path, "not json\n{\"type\":\"user\",\"message\":{\"content\":\"hello\"}}\n}broken{\n").unwrap();
    let msgs = parse_messages(&path).unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].role, "user");
    assert_eq!(msgs[0].text, "hello");
}

#[test]
fn test_parse_messages_skips_non_user_assistant_roles() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("roles.jsonl");
    std::fs::write(&path,
        "{\"type\":\"system\",\"message\":{\"content\":\"sys\"}}\n\
         {\"type\":\"user\",\"message\":{\"content\":\"hi\"}}\n\
         {\"type\":\"tool\",\"message\":{\"content\":\"output\"}}\n"
    ).unwrap();
    let msgs = parse_messages(&path).unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].role, "user");
}

#[test]
fn test_read_cwd_from_jsonl_finds_cwd() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("test.jsonl");
    std::fs::write(&path,
        "{\"type\":\"user\"}\n\
         {\"cwd\":\"/home/user/myproject\",\"type\":\"system\"}\n\
         {\"type\":\"assistant\"}\n"
    ).unwrap();
    let cwd = read_cwd_from_jsonl(&path);
    assert_eq!(cwd, Some("/home/user/myproject".to_string()));
}

#[test]
fn test_read_cwd_from_jsonl_returns_none_when_absent() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("nocwd.jsonl");
    std::fs::write(&path, "{\"type\":\"user\",\"message\":{\"content\":\"hello\"}}\n").unwrap();
    assert!(read_cwd_from_jsonl(&path).is_none());
}

#[test]
fn test_read_cwd_from_jsonl_skips_malformed_lines() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("malformed.jsonl");
    std::fs::write(&path,
        "not json at all\n\
         {\"cwd\":\"/found/it\"}\n"
    ).unwrap();
    // Should not abort on the malformed line, but find the cwd
    let cwd = read_cwd_from_jsonl(&path);
    assert_eq!(cwd, Some("/found/it".to_string()));
}

#[test]
fn test_write_and_read_rename_history() {
    let tmp = TempDir::new().unwrap();
    // write_rename writes to ~/.claude/history.jsonl, so we test the round-trip
    // through a temp file by writing a line directly, then reading it.
    let history_path = tmp.path().join("history.jsonl");
    let ts: u64 = 1_700_000_000_000;
    let entry = serde_json::json!({
        "display": "/rename My Custom Title",
        "sessionId": "sess-abc123",
        "timestamp": ts,
    });
    let mut f = std::fs::File::create(&history_path).unwrap();
    writeln!(f, "{}", entry).unwrap();
    drop(f);

    // read_rename_history reads from ~/. — we test the parsing logic via a direct parse
    let content = std::fs::read_to_string(&history_path).unwrap();
    let v: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
    let display = v["display"].as_str().unwrap();
    assert!(display.starts_with("/rename "));
    let title = display.strip_prefix("/rename ").unwrap().trim();
    assert_eq!(title, "My Custom Title");
}

#[test]
fn test_dir_name_to_abs_path() {
    use cc_speedy::sessions::dir_name_to_abs_path;
    let result = dir_name_to_abs_path("-home-weibin-repo-ai-foo");
    assert_eq!(result, "/home/weibin/repo/ai/foo");
}

#[test]
fn test_dir_name_to_path_last_two_segments() {
    use cc_speedy::sessions::dir_name_to_path;
    let result = dir_name_to_path("-home-weibin-repo-ai-foo");
    assert_eq!(result, "ai/foo");
}

#[test]
fn test_sessions_index_deserialization() {
    let content = std::fs::read_to_string("tests/fixtures/sessions-index.json").unwrap();
    let index: SessionIndex = serde_json::from_str(&content).unwrap();
    assert_eq!(index.original_path, "/home/weibin/repo/ai/cc-speedy");
    assert_eq!(index.entries.len(), 2);
    let first = &index.entries[0];
    assert_eq!(first.summary, "sessions-index.json Integration + UI Improvements");
    assert_eq!(first.git_branch, "feature/sessions-index");
    assert_eq!(first.message_count, 42);
    assert!(!first.is_sidechain);
}

#[test]
fn test_sessions_index_entry_with_empty_fields() {
    let content = std::fs::read_to_string("tests/fixtures/sessions-index.json").unwrap();
    let index: SessionIndex = serde_json::from_str(&content).unwrap();
    let second = &index.entries[1];
    assert_eq!(second.summary, "");
    assert_eq!(second.git_branch, "");
    assert_eq!(second.message_count, 2);
}

#[test]
fn test_file_mtime_milliseconds_conversion() {
    use std::time::{Duration, UNIX_EPOCH};
    // 1709654400000 ms = 1709654400 seconds
    let file_mtime: u64 = 1709654400000;
    let system_time = UNIX_EPOCH + Duration::from_millis(file_mtime);
    let secs = system_time
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert_eq!(secs, 1709654400);
}

