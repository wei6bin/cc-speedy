use cc_speedy::sessions::Session;
use cc_speedy::unified::{UnifiedSession, SessionSource};
use std::time::SystemTime;

fn make_cc_session() -> Session {
    Session {
        session_id: "abc-123".to_string(),
        project_name: "ai/myproj".to_string(),
        project_path: "/home/user/ai/myproj".to_string(),
        modified: SystemTime::UNIX_EPOCH,
        message_count: 10,
        first_user_msg: "fix the bug".to_string(),
        jsonl_path: "/home/user/.claude/projects/x/abc-123.jsonl".to_string(),
        summary: "Fixed auth bug".to_string(),
        git_branch: "main".to_string(),
    }
}

#[test]
fn test_from_cc_session_sets_source() {
    let s: UnifiedSession = make_cc_session().into();
    assert!(matches!(s.source, SessionSource::ClaudeCode));
}

#[test]
fn test_from_cc_session_preserves_fields() {
    let s: UnifiedSession = make_cc_session().into();
    assert_eq!(s.session_id, "abc-123");
    assert_eq!(s.project_path, "/home/user/ai/myproj");
    assert_eq!(s.message_count, 10);
    assert_eq!(s.summary, "Fixed auth bug");
    assert_eq!(s.jsonl_path, Some("/home/user/.claude/projects/x/abc-123.jsonl".to_string()));
}

#[test]
fn test_opencode_session_has_no_jsonl_path() {
    let s = UnifiedSession {
        session_id: "ses_abc".to_string(),
        project_name: "ai/myproj".to_string(),
        project_path: "/home/user/ai/myproj".to_string(),
        modified: SystemTime::UNIX_EPOCH,
        message_count: 5,
        first_user_msg: "build feature".to_string(),
        summary: "".to_string(),
        git_branch: "".to_string(),
        source: SessionSource::OpenCode,
        jsonl_path: None,
    };
    assert!(s.jsonl_path.is_none());
}

use cc_speedy::unified::list_all_sessions;

#[test]
fn test_list_all_sessions_does_not_panic() {
    // Smoke test: just ensure it returns without panicking.
    // Real DB may or may not exist on the test machine.
    let result = list_all_sessions();
    assert!(result.is_ok(), "list_all_sessions returned error: {:?}", result);
}

