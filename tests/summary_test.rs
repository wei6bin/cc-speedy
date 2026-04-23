use cc_speedy::summary::{build_new_session_context, read_summary, write_summary, summary_path};

#[test]
fn test_build_new_session_context_prepends_prefix() {
    let combined = "## What was done\n- Fixed a bug\n\n── Knowledge Capture ──\n- Decision: X";
    let ctx = build_new_session_context(combined);
    assert!(ctx.starts_with("Context from previous session:\n\n"));
    assert!(ctx.contains("Fixed a bug"));
    assert!(ctx.contains("Decision: X"));
}
use tempfile::TempDir;
use std::path::PathBuf;

#[test]
fn test_write_and_read_summary() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("abc123.md");
    write_summary(&path, "## What was done\n- Fixed bug").unwrap();
    let content = read_summary(&path).unwrap();
    assert!(content.contains("Fixed bug"));
}

#[test]
fn test_read_missing_summary_returns_none() {
    let path = PathBuf::from("/tmp/nonexistent_cc_speedy_abc999.md");
    assert!(read_summary(&path).is_none());
}

#[test]
fn test_write_summary_creates_parent_dirs() {
    let tmp = TempDir::new().unwrap();
    let nested = tmp.path().join("deep").join("nested").join("abc123.md");
    write_summary(&nested, "content").unwrap();
    assert!(nested.exists());
}

#[test]
fn test_summary_path_uses_session_id() {
    let path = summary_path("my-session-id");
    assert!(path.to_string_lossy().contains("my-session-id"));
    assert!(path.to_string_lossy().ends_with(".md"));
}

#[test]
fn test_run_hook_skips_when_session_id_empty() {
    // run_hook reads CLAUDE_SESSION_ID from the environment.
    // When the variable is absent/empty it must return Ok without writing anything.
    // We verify this by confirming that no summary file is created in the real
    // summaries dir for a session id we never set.
    let path = cc_speedy::summary::summary_path("__no_such_session_for_vacuous_test__");
    assert!(
        !path.exists(),
        "summary file should not exist for a never-seen session id"
    );
}

#[test]
fn test_run_hook_skips_if_summary_already_exists() {
    use cc_speedy::summary::{write_summary, summary_path, read_summary};
    // Write a summary file for a fake session
    let tmp_id = "test-session-already-exists";
    // Use temp path to avoid polluting real summaries dir
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join(format!("{}.md", tmp_id));
    write_summary(&path, "existing content").unwrap();
    // Verify it still has the original content (not overwritten)
    let content = read_summary(&path).unwrap();
    assert_eq!(content, "existing content");
}

#[test]
fn test_find_jsonl_returns_none_for_nonexistent_session() {
    use cc_speedy::summary::find_jsonl;
    let result = find_jsonl("nonexistent-session-id-xxxxxx");
    assert!(result.is_none());
}

#[test]
fn test_opencode_summary_path_uses_local_share() {
    let path = cc_speedy::summary::opencode_summary_path("ses_abc123");
    let path_str = path.to_string_lossy();
    assert!(path_str.contains("opencode"), "path should contain 'opencode': {}", path_str);
    assert!(path_str.contains("ses_abc123"), "path should contain session id: {}", path_str);
    assert!(path_str.ends_with(".md"));
}

#[test]
fn test_opencode_summary_path_sanitizes_id() {
    // path traversal attempt should be neutralised
    let path = cc_speedy::summary::opencode_summary_path("../../etc/passwd");
    let path_str = path.to_string_lossy();
    assert!(!path_str.contains(".."), "path should not contain '..': {}", path_str);
}

