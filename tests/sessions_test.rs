use cc_speedy::sessions::{parse_messages, SessionIndex};

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
