//! Refresh primitives: diff computation and selection preservation.
//! Pure functions only — no I/O and no async. Lives in its own module
//! so the logic is unit-testable without spinning up the TUI.

use crate::unified::UnifiedSession;
use std::collections::{HashMap, HashSet};
use std::time::SystemTime;

/// Outcome of a session re-scan, ready to apply to `AppState`.
pub struct RefreshResult {
    pub sessions: Vec<UnifiedSession>,
    pub new_count: usize,
    pub updated_count: usize,
}

/// Compare `prior` against `new` and return a `RefreshResult` carrying the
/// counts plus ownership of `new`.
///
/// - `new_count`: sessions in `new` whose `session_id` is absent from `prior`.
/// - `updated_count`: sessions in `new` whose `session_id` was in `prior` and
///   whose `modified` strictly advanced.
///
/// Removed sessions are not counted; the caller drops them implicitly by
/// replacing the list.
pub fn compute_refresh_diff(prior: &[UnifiedSession], new: Vec<UnifiedSession>) -> RefreshResult {
    let prior_ids: HashSet<&str> = prior.iter().map(|s| s.session_id.as_str()).collect();
    let prior_modified: HashMap<&str, SystemTime> = prior
        .iter()
        .map(|s| (s.session_id.as_str(), s.modified))
        .collect();

    let mut new_count = 0;
    let mut updated_count = 0;
    for s in &new {
        if !prior_ids.contains(s.session_id.as_str()) {
            new_count += 1;
        } else if prior_modified
            .get(s.session_id.as_str())
            .map(|t| s.modified > *t)
            .unwrap_or(false)
        {
            updated_count += 1;
        }
    }

    RefreshResult {
        sessions: new,
        new_count,
        updated_count,
    }
}

/// Returns the new selection index (a position into `filtered`) for a session
/// whose `session_id` matches `target_id`. Falls back to `Some(0)` when the
/// target is missing but `filtered` is non-empty, and `None` when `filtered`
/// is empty. Used by `AppState::drain_refresh_results` to preserve selection
/// across re-scans.
pub fn select_index_for_session_id(
    filtered: &[usize],
    sessions: &[UnifiedSession],
    target_id: Option<&str>,
) -> Option<usize> {
    if filtered.is_empty() {
        return None;
    }
    if let Some(target) = target_id {
        for (pos, &raw) in filtered.iter().enumerate() {
            if let Some(s) = sessions.get(raw) {
                if s.session_id == target {
                    return Some(pos);
                }
            }
        }
    }
    Some(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unified::SessionSource;
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
    fn empty_prior_all_new() {
        let new = vec![make_session("a", 100), make_session("b", 200)];
        let r = compute_refresh_diff(&[], new);
        assert_eq!(r.new_count, 2);
        assert_eq!(r.updated_count, 0);
        assert_eq!(r.sessions.len(), 2);
    }

    #[test]
    fn empty_new_no_changes() {
        let prior = vec![make_session("a", 100)];
        let r = compute_refresh_diff(&prior, vec![]);
        assert_eq!(r.new_count, 0);
        assert_eq!(r.updated_count, 0);
        assert_eq!(r.sessions.len(), 0);
    }

    #[test]
    fn unchanged_set_reports_zero() {
        let prior = vec![make_session("a", 100), make_session("b", 200)];
        let new = vec![make_session("a", 100), make_session("b", 200)];
        let r = compute_refresh_diff(&prior, new);
        assert_eq!(r.new_count, 0);
        assert_eq!(r.updated_count, 0);
    }

    #[test]
    fn detects_new_only() {
        let prior = vec![make_session("a", 100)];
        let new = vec![make_session("a", 100), make_session("b", 200)];
        let r = compute_refresh_diff(&prior, new);
        assert_eq!(r.new_count, 1);
        assert_eq!(r.updated_count, 0);
    }

    #[test]
    fn detects_updated_only() {
        let prior = vec![make_session("a", 100)];
        let new = vec![make_session("a", 200)];
        let r = compute_refresh_diff(&prior, new);
        assert_eq!(r.new_count, 0);
        assert_eq!(r.updated_count, 1);
    }

    #[test]
    fn updated_means_strictly_greater() {
        let prior = vec![make_session("a", 200)];
        let new = vec![make_session("a", 200)];
        let r = compute_refresh_diff(&prior, new);
        assert_eq!(r.updated_count, 0);
    }

    #[test]
    fn removed_session_is_silent() {
        let prior = vec![make_session("a", 100), make_session("b", 200)];
        let new = vec![make_session("a", 100)];
        let r = compute_refresh_diff(&prior, new);
        assert_eq!(r.new_count, 0);
        assert_eq!(r.updated_count, 0);
        assert_eq!(r.sessions.len(), 1);
    }

    #[test]
    fn mixed_new_and_updated() {
        let prior = vec![make_session("a", 100), make_session("b", 200)];
        let new = vec![
            make_session("a", 150),
            make_session("b", 200),
            make_session("c", 300),
            make_session("d", 400),
        ];
        let r = compute_refresh_diff(&prior, new);
        assert_eq!(r.new_count, 2);
        assert_eq!(r.updated_count, 1);
    }

    #[test]
    fn select_returns_index_when_target_present() {
        let sessions = vec![
            make_session("a", 100),
            make_session("b", 200),
            make_session("c", 300),
        ];
        let filtered = vec![0, 1, 2];
        let idx = select_index_for_session_id(&filtered, &sessions, Some("b"));
        assert_eq!(idx, Some(1));
    }

    #[test]
    fn select_falls_back_to_zero_when_target_missing_but_list_non_empty() {
        let sessions = vec![make_session("a", 100), make_session("b", 200)];
        let filtered = vec![0, 1];
        let idx = select_index_for_session_id(&filtered, &sessions, Some("zzz"));
        assert_eq!(idx, Some(0));
    }

    #[test]
    fn select_returns_none_when_filtered_is_empty() {
        let sessions = vec![make_session("a", 100)];
        let filtered: Vec<usize> = vec![];
        let idx = select_index_for_session_id(&filtered, &sessions, Some("a"));
        assert_eq!(idx, None);
    }

    #[test]
    fn select_returns_zero_when_target_id_is_none() {
        let sessions = vec![make_session("a", 100)];
        let filtered = vec![0];
        let idx = select_index_for_session_id(&filtered, &sessions, None);
        assert_eq!(idx, Some(0));
    }

    #[test]
    fn select_works_with_sparse_filtered() {
        let sessions = vec![
            make_session("a", 100),
            make_session("b", 200),
            make_session("c", 300),
        ];
        let filtered = vec![0, 2];
        assert_eq!(
            select_index_for_session_id(&filtered, &sessions, Some("c")),
            Some(1)
        );
        assert_eq!(
            select_index_for_session_id(&filtered, &sessions, Some("b")),
            Some(0)
        );
    }
}
