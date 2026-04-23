use cc_speedy::git_status::{parse_porcelain, GitStatus};

#[test]
fn test_parse_clean_with_tracking() {
    let stdout = "## feat/x...origin/feat/x\n";
    assert_eq!(parse_porcelain(stdout), GitStatus::Clean { branch: "feat/x".to_string() });
}

#[test]
fn test_parse_clean_no_tracking() {
    let stdout = "## master\n";
    assert_eq!(parse_porcelain(stdout), GitStatus::Clean { branch: "master".to_string() });
}

#[test]
fn test_parse_dirty_modified_file() {
    let stdout = "## feat/x...origin/feat/x\n M src/main.rs\n";
    assert_eq!(parse_porcelain(stdout), GitStatus::Dirty { branch: "feat/x".to_string() });
}

#[test]
fn test_parse_dirty_untracked_only() {
    let stdout = "## master\n?? new-file.txt\n";
    assert_eq!(parse_porcelain(stdout), GitStatus::Dirty { branch: "master".to_string() });
}

#[test]
fn test_parse_dirty_multiple_changes() {
    let stdout = "## feat/x\n M a.rs\nMM b.rs\n?? c.rs\n";
    assert_eq!(parse_porcelain(stdout), GitStatus::Dirty { branch: "feat/x".to_string() });
}

#[test]
fn test_parse_detached_head() {
    let stdout = "## HEAD (no branch)\n";
    assert_eq!(parse_porcelain(stdout), GitStatus::Clean { branch: "HEAD (no branch)".to_string() });
}

#[test]
fn test_parse_ahead_behind_tracking_stripped() {
    let stdout = "## feat/x...origin/feat/x [ahead 2, behind 1]\n";
    assert_eq!(parse_porcelain(stdout), GitStatus::Clean { branch: "feat/x".to_string() });
}

#[test]
fn test_parse_empty_stdout_is_error() {
    assert_eq!(parse_porcelain(""), GitStatus::Error);
}

#[test]
fn test_parse_no_branch_line_is_error() {
    let stdout = "fatal: not a git repository\n";
    assert_eq!(parse_porcelain(stdout), GitStatus::Error);
}

#[test]
fn test_branch_accessor_returns_for_clean_and_dirty() {
    assert_eq!(GitStatus::Clean { branch: "m".into() }.branch(), Some("m"));
    assert_eq!(GitStatus::Dirty { branch: "d".into() }.branch(), Some("d"));
    assert_eq!(GitStatus::NoGit.branch(), None);
    assert_eq!(GitStatus::Error.branch(), None);
}
