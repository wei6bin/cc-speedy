use crate::opencode_sessions::list_opencode_sessions;
use crate::sessions::{list_sessions, Session};
use anyhow::Result;
use std::collections::HashMap;
use std::time::SystemTime;

#[derive(Debug, Clone, PartialEq)]
pub enum SessionSource {
    ClaudeCode,
    OpenCode,
    Copilot,
}

#[derive(Debug, Clone)]
pub struct UnifiedSession {
    pub session_id: String,
    pub project_name: String,
    pub project_path: String,
    pub modified: SystemTime,
    pub message_count: usize,
    pub first_user_msg: String,
    pub summary: String,
    pub git_branch: String,
    pub source: SessionSource,
    /// Some(path) for Claude Code sessions; None for OpenCode and Copilot sessions.
    pub jsonl_path: Option<String>,
    /// Whether this session is archived (shown at bottom of list).
    pub archived: bool,
}

impl From<Session> for UnifiedSession {
    fn from(s: Session) -> Self {
        UnifiedSession {
            session_id: s.session_id,
            project_name: s.project_name,
            project_path: s.project_path,
            modified: s.modified,
            message_count: s.message_count,
            first_user_msg: s.first_user_msg,
            summary: s.summary,
            git_branch: s.git_branch,
            source: SessionSource::ClaudeCode,
            jsonl_path: Some(s.jsonl_path),
            archived: false,
        }
    }
}

/// Merge Claude Code, OpenCode, and Copilot sessions into a single list sorted by recency.
pub fn list_all_sessions() -> Result<Vec<UnifiedSession>> {
    let cc = list_sessions()
        .unwrap_or_default()
        .into_iter()
        .map(UnifiedSession::from)
        .collect::<Vec<_>>();

    let oc = list_opencode_sessions().unwrap_or_default();
    let co = crate::copilot_sessions::list_copilot_sessions().unwrap_or_default();

    let mut all: Vec<UnifiedSession> = cc.into_iter().chain(oc).chain(co).collect();
    all.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(all)
}

/// Map keyed by `session_id` for O(1) lookups during incremental refresh.
pub type PriorById<'a> = HashMap<&'a str, &'a UnifiedSession>;

/// If `prior_by_id` contains an entry for `session_id` whose `modified` equals
/// `current_mtime`, return a clone of the prior session — the caller can push
/// it directly and skip the expensive per-session parse. Otherwise return
/// `None` and the caller falls through to its normal parse path.
pub fn try_reuse_prior(
    prior_by_id: &PriorById<'_>,
    session_id: &str,
    current_mtime: SystemTime,
) -> Option<UnifiedSession> {
    prior_by_id.get(session_id).and_then(|p| {
        if p.modified == current_mtime {
            Some((*p).clone())
        } else {
            None
        }
    })
}

/// Incremental variant of `list_all_sessions` for use by refresh. For each
/// session encountered, the per-source listers compare a cheap mtime signal
/// against `prior` before doing the expensive per-session parse — when the
/// mtime is unchanged, the prior `UnifiedSession` is reused as-is.
///
/// On a 400+ session corpus this turns refresh from ~hundreds of file/SQL
/// reads into a stat-only walk in the common case.
pub fn list_all_sessions_incremental(prior: &[UnifiedSession]) -> Result<Vec<UnifiedSession>> {
    let prior_by_id: PriorById<'_> = prior.iter().map(|s| (s.session_id.as_str(), s)).collect();

    let cc = crate::sessions::list_sessions_incremental(&prior_by_id).unwrap_or_default();
    let oc = crate::opencode_sessions::list_opencode_sessions_incremental(&prior_by_id)
        .unwrap_or_default();
    let co = crate::copilot_sessions::list_copilot_sessions_incremental(&prior_by_id)
        .unwrap_or_default();

    let mut all: Vec<UnifiedSession> = cc.into_iter().chain(oc).chain(co).collect();
    all.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    fn make_session(id: &str, modified_secs: u64) -> UnifiedSession {
        UnifiedSession {
            session_id: id.to_string(),
            project_name: "p".to_string(),
            project_path: "/tmp/p".to_string(),
            modified: UNIX_EPOCH + Duration::from_secs(modified_secs),
            message_count: 0,
            first_user_msg: String::new(),
            summary: String::new(),
            git_branch: String::new(),
            source: SessionSource::ClaudeCode,
            jsonl_path: None,
            archived: false,
        }
    }

    #[test]
    fn try_reuse_prior_returns_clone_when_mtime_matches() {
        let prior = vec![make_session("a", 100)];
        let map: PriorById = prior.iter().map(|s| (s.session_id.as_str(), s)).collect();
        let mtime = UNIX_EPOCH + Duration::from_secs(100);
        let reused = try_reuse_prior(&map, "a", mtime);
        assert!(reused.is_some());
        assert_eq!(reused.unwrap().session_id, "a");
    }

    #[test]
    fn try_reuse_prior_returns_none_when_mtime_advanced() {
        let prior = vec![make_session("a", 100)];
        let map: PriorById = prior.iter().map(|s| (s.session_id.as_str(), s)).collect();
        let mtime = UNIX_EPOCH + Duration::from_secs(200);
        assert!(try_reuse_prior(&map, "a", mtime).is_none());
    }

    #[test]
    fn try_reuse_prior_returns_none_when_id_absent() {
        let prior = vec![make_session("a", 100)];
        let map: PriorById = prior.iter().map(|s| (s.session_id.as_str(), s)).collect();
        let mtime = UNIX_EPOCH + Duration::from_secs(100);
        assert!(try_reuse_prior(&map, "b", mtime).is_none());
    }

    #[test]
    fn try_reuse_prior_handles_empty_prior() {
        let map: PriorById = HashMap::new();
        let mtime = UNIX_EPOCH + Duration::from_secs(100);
        assert!(try_reuse_prior(&map, "a", mtime).is_none());
    }
}
