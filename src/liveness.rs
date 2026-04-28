//! Per-session liveness detection. Pure detection logic — no async, no
//! I/O beyond a single `metadata()` + bounded tail read. Runtime polling
//! and caching live in `tui.rs`.
//!
//! ## States
//!
//! - [`Liveness::Live`] — the agent is currently producing output (CC/Copilot
//!   only; OpenCode never reaches this state in v1, see below).
//! - [`Liveness::Recent`] — the session was active in the last few minutes
//!   but the trailing turn is closed.
//! - [`Liveness::Idle`] — older than [`RECENT_WINDOW_SECS`], or no signal.
//!
//! ## OpenCode caveat
//!
//! OpenCode sessions are stored in a SQLite database, not a JSONL log, so
//! we cannot tail-parse them for unclosed turns. `detect_oc` therefore
//! relies on the cached `UnifiedSession.modified` and only ever returns
//! `Idle` or `Recent`. A future pass could query the OC db directly.

use crate::unified::{SessionSource, UnifiedSession};
use std::path::Path;
use std::time::SystemTime;

/// Tri-state liveness signal for a session.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Liveness {
    Idle,
    Recent,
    Live,
}

/// Cached liveness with the absolute time it was observed. The renderer
/// uses `observed_at` to apply an idle-decay rule: a `Live` entry older
/// than [`LIVE_WINDOW_SECS`] without an update is shown as `Recent` for
/// display purposes (the cache itself is not mutated; the next poll
/// overwrites).
#[derive(Copy, Clone, Debug)]
pub struct CachedLiveness {
    pub state: Liveness,
    pub observed_at: std::time::Instant,
}

/// Window for the `Live` state — the JSONL must have been written to
/// within this many seconds AND the trailing turn must have an unclosed
/// tool_use.
pub const LIVE_WINDOW_SECS: u64 = 5;

/// Window for the `Recent` state — the session was written to within
/// this many seconds but the trailing turn is closed.
pub const RECENT_WINDOW_SECS: u64 = 300;

/// How many trailing bytes of a JSONL we read to look for an open turn.
pub const TAIL_BYTES: u64 = 8 * 1024;

/// Classify the liveness of a session. Returns immediately on disk
/// errors, treating them as `Idle` (a session whose log has disappeared
/// is, by definition, not live).
pub fn detect(session: &UnifiedSession) -> Liveness {
    match session.source {
        SessionSource::ClaudeCode => match session.jsonl_path.as_deref() {
            Some(path) => detect_cc(Path::new(path)),
            None => Liveness::Idle,
        },
        SessionSource::Copilot => match session.jsonl_path.as_deref() {
            Some(path) => detect_copilot(Path::new(path)),
            None => Liveness::Idle,
        },
        SessionSource::OpenCode => detect_oc(session.modified),
    }
}

/// Mtime-only classifier: returns `Idle` if the file was last modified
/// more than [`RECENT_WINDOW_SECS`] ago, otherwise returns the input
/// `then_inside_window` (typically the result of a tail parse).
fn classify_by_mtime(modified: SystemTime, then_inside_window: Liveness) -> Liveness {
    let elapsed = SystemTime::now()
        .duration_since(modified)
        .map(|d| d.as_secs())
        .unwrap_or(u64::MAX);
    if elapsed > RECENT_WINDOW_SECS {
        Liveness::Idle
    } else if elapsed > LIVE_WINDOW_SECS {
        Liveness::Recent
    } else {
        then_inside_window
    }
}

/// OpenCode detector. We have no log file to tail (sessions live in
/// SQLite), so the strongest signal we can give is `Recent` — never
/// `Live`. Caller is expected to refresh the session list to update the
/// `modified` field.
pub fn detect_oc(modified: SystemTime) -> Liveness {
    classify_by_mtime(modified, Liveness::Recent)
}

/// Claude Code detector. Stub — full implementation in Task 2.
pub fn detect_cc(_path: &Path) -> Liveness {
    Liveness::Idle
}

/// Copilot detector. Stub — full implementation in Task 3.
pub fn detect_copilot(_path: &Path) -> Liveness {
    Liveness::Idle
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn make_session(
        source: SessionSource,
        jsonl: Option<&str>,
        mtime_secs_ago: u64,
    ) -> UnifiedSession {
        UnifiedSession {
            session_id: "sid".to_string(),
            project_name: "p".to_string(),
            project_path: "/tmp/p".to_string(),
            modified: SystemTime::now() - Duration::from_secs(mtime_secs_ago),
            message_count: 0,
            first_user_msg: String::new(),
            summary: String::new(),
            git_branch: String::new(),
            source,
            jsonl_path: jsonl.map(|s| s.to_string()),
            archived: false,
        }
    }

    #[test]
    fn oc_idle_when_old() {
        let s = make_session(SessionSource::OpenCode, None, RECENT_WINDOW_SECS + 60);
        assert_eq!(detect(&s), Liveness::Idle);
    }

    #[test]
    fn oc_recent_when_inside_recent_window() {
        let s = make_session(SessionSource::OpenCode, None, 30);
        assert_eq!(detect(&s), Liveness::Recent);
    }

    #[test]
    fn oc_recent_never_live() {
        // Inside the LIVE window but OC can't be Live.
        let s = make_session(SessionSource::OpenCode, None, 1);
        assert_eq!(detect(&s), Liveness::Recent);
    }

    #[test]
    fn cc_returns_idle_when_jsonl_path_missing() {
        let s = make_session(SessionSource::ClaudeCode, None, 1);
        assert_eq!(detect(&s), Liveness::Idle);
    }

    #[test]
    fn copilot_returns_idle_when_jsonl_path_missing() {
        let s = make_session(SessionSource::Copilot, None, 1);
        assert_eq!(detect(&s), Liveness::Idle);
    }

    #[test]
    fn classify_by_mtime_idle_when_old() {
        let mtime = SystemTime::now() - Duration::from_secs(RECENT_WINDOW_SECS + 1);
        assert_eq!(classify_by_mtime(mtime, Liveness::Live), Liveness::Idle);
    }

    #[test]
    fn classify_by_mtime_recent_when_between() {
        let mtime = SystemTime::now() - Duration::from_secs(LIVE_WINDOW_SECS + 10);
        assert_eq!(classify_by_mtime(mtime, Liveness::Live), Liveness::Recent);
    }

    #[test]
    fn classify_by_mtime_passes_through_when_fresh() {
        let mtime = SystemTime::now() - Duration::from_secs(1);
        assert_eq!(classify_by_mtime(mtime, Liveness::Live), Liveness::Live);
    }
}
