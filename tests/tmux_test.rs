use cc_speedy::tmux::session_name_from_path;

#[test]
fn test_session_name_from_path_two_segments() {
    let name = session_name_from_path("/home/weibin/repo/ai/zero-drift-chat");
    assert_eq!(name, "ai-zero-drift-chat");
}

#[test]
fn test_session_name_truncated_to_50_chars() {
    let long = "/a/b/c/d/e/f/g/h/i/j/k/l/m/n/this-is-a-very-long-project-name-that-exceeds-limits";
    let name = session_name_from_path(long);
    assert!(name.len() <= 50, "session name was {} chars: {}", name.len(), name);
}

#[test]
fn test_session_name_from_root_path() {
    let name = session_name_from_path("/single");
    assert_eq!(name, "single");
}

#[test]
fn test_session_name_sanitizes_special_chars() {
    // Path segments with dots or spaces should be sanitized
    let name = session_name_from_path("/home/user/my.project");
    // dots should be filtered out
    assert!(!name.contains('.'));
}

#[test]
fn test_is_inside_tmux_returns_bool() {
    use cc_speedy::tmux::is_inside_tmux;
    // Just verify it returns without panic
    let _ = is_inside_tmux();
}
