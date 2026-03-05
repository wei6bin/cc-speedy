use cc_speedy::summary::{read_summary, write_summary, summary_path};
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
