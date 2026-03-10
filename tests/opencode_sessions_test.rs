use cc_speedy::opencode_sessions::opencode_db_path;

#[test]
fn test_opencode_db_path_ends_with_db_file() {
    if let Some(p) = opencode_db_path() {
        let s = p.to_string_lossy();
        assert!(s.ends_with("opencode.db"), "expected path to end with opencode.db, got: {}", s);
        assert!(s.contains("opencode"), "expected path to contain 'opencode': {}", s);
    }
    // If None: opencode not installed; that's acceptable — test passes
}
