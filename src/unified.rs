use std::time::SystemTime;
use crate::sessions::Session;

#[derive(Debug, Clone, PartialEq)]
pub enum SessionSource {
    ClaudeCode,
    OpenCode,
}

#[derive(Debug, Clone)]
pub struct UnifiedSession {
    pub session_id:    String,
    pub project_name:  String,
    pub project_path:  String,
    pub modified:      SystemTime,
    pub message_count: usize,
    pub first_user_msg: String,
    pub summary:       String,
    pub git_branch:    String,
    pub source:        SessionSource,
    /// Some(path) for Claude Code sessions; None for OpenCode sessions.
    pub jsonl_path:    Option<String>,
}

impl From<Session> for UnifiedSession {
    fn from(s: Session) -> Self {
        UnifiedSession {
            session_id:    s.session_id,
            project_name:  s.project_name,
            project_path:  s.project_path,
            modified:      s.modified,
            message_count: s.message_count,
            first_user_msg: s.first_user_msg,
            summary:       s.summary,
            git_branch:    s.git_branch,
            source:        SessionSource::ClaudeCode,
            jsonl_path:    Some(s.jsonl_path),
        }
    }
}
