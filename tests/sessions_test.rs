use cc_speedy::sessions::parse_messages;

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
