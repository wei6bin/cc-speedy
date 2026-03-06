use cc_speedy::tmux::{session_name_from_path, pin_window_title};

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

/// Integration test: verifies pin_window_title renames the tmux window and prevents
/// automatic-rename from overriding it. Skipped if tmux is not installed.
#[test]
fn test_pin_window_title_sets_and_locks_name() {
    // Skip if tmux is not available
    if std::process::Command::new("tmux").arg("-V").output().is_err() {
        eprintln!("tmux not found, skipping");
        return;
    }

    let session = "cc-speedy-title-test";
    let title = "TestTitle";

    // Clean up any leftover session from a previous run
    let _ = std::process::Command::new("tmux")
        .args(["kill-session", "-t", session])
        .output();

    // Create a detached session running sleep (not claude)
    let created = std::process::Command::new("tmux")
        .args(["new-session", "-d", "-s", session, "-n", "initial", "sleep", "30"])
        .status()
        .expect("tmux new-session failed");
    assert!(created.success(), "Could not create test tmux session");

    // Apply the pin logic
    pin_window_title(session, title);

    // Query the window name
    let out = std::process::Command::new("tmux")
        .args(["display-message", "-t", session, "-p", "#{window_name}"])
        .output()
        .expect("tmux display-message failed");
    let window_name = String::from_utf8_lossy(&out.stdout).trim().to_string();

    // Query automatic-rename option
    let ar_out = std::process::Command::new("tmux")
        .args(["show-window-options", "-t", session, "automatic-rename"])
        .output()
        .expect("tmux show-window-options failed");
    let ar_value = String::from_utf8_lossy(&ar_out.stdout).trim().to_string();

    // Clean up
    let _ = std::process::Command::new("tmux")
        .args(["kill-session", "-t", session])
        .output();

    assert_eq!(window_name, title, "window name should be '{}', got '{}'", title, window_name);
    assert!(ar_value.contains("off"), "automatic-rename should be 'off', got '{}'", ar_value);
}
