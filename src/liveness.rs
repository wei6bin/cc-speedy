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
#[derive(Copy, Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
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

/// Claude Code detector. mtime-gated; if inside the live window, we
/// tail-parse the JSONL for an unmatched `tool_use`.
pub fn detect_cc(path: &Path) -> Liveness {
    let mtime = match std::fs::metadata(path).and_then(|m| m.modified()) {
        Ok(t) => t,
        Err(_) => return Liveness::Idle,
    };
    let coarse = classify_by_mtime(mtime, Liveness::Live);
    if coarse != Liveness::Live {
        return coarse;
    }
    let tail = match read_tail(path) {
        Some(s) => s,
        None => return Liveness::Recent,
    };
    if cc_tail_has_open_tool_use(&tail) {
        Liveness::Live
    } else {
        Liveness::Recent
    }
}

/// Copilot detector. Same mtime gate as CC; tail-parses
/// `events.jsonl` for an unclosed `assistant.message`.
pub fn detect_copilot(path: &Path) -> Liveness {
    let mtime = match std::fs::metadata(path).and_then(|m| m.modified()) {
        Ok(t) => t,
        Err(_) => return Liveness::Idle,
    };
    let coarse = classify_by_mtime(mtime, Liveness::Live);
    if coarse != Liveness::Live {
        return coarse;
    }
    let tail = match read_tail(path) {
        Some(s) => s,
        None => return Liveness::Recent,
    };
    if copilot_tail_has_open_turn(&tail) {
        Liveness::Live
    } else {
        Liveness::Recent
    }
}

/// Classify the trailing CC JSONL bytes as having an open `tool_use`
/// (no matching `tool_result` further down) or not.
///
/// Pure function — takes the tail content as a string and returns a
/// boolean. The caller is responsible for the mtime gate; this only
/// reports the structural state of the trailing turn.
///
/// Returns `true` when there is an unmatched `tool_use` in the tail.
/// Returns `false` on parse errors or when all `tool_use` blocks are
/// matched — both are interpreted as "closed turn" so a malformed tail
/// degrades to `Recent`, never a false `Live`.
pub fn cc_tail_has_open_tool_use(tail: &str) -> bool {
    use std::collections::HashSet;
    let mut pending: HashSet<String> = HashSet::new();

    // Note: any partial first line at the start of the tail (when the
    // read started mid-line) will simply fail to parse as JSON and be
    // skipped by the `Err(_) => continue` arm below. We do not skip the
    // first line unconditionally, because callers may pass a tail that
    // begins on a clean line boundary.
    for line in tail.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let ty = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
        let content = v
            .get("message")
            .and_then(|m| m.get("content"))
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let blocks: Vec<serde_json::Value> = match content {
            serde_json::Value::Array(a) => a,
            // Older CC sessions store `content` as a plain string; treat as text-only, no tool use.
            serde_json::Value::String(_) => continue,
            _ => continue,
        };
        match ty {
            "assistant" => {
                for b in &blocks {
                    if b.get("type").and_then(|x| x.as_str()) == Some("tool_use") {
                        if let Some(id) = b.get("id").and_then(|x| x.as_str()) {
                            pending.insert(id.to_string());
                        }
                    }
                }
            }
            "user" => {
                for b in &blocks {
                    if b.get("type").and_then(|x| x.as_str()) == Some("tool_result") {
                        if let Some(id) = b.get("tool_use_id").and_then(|x| x.as_str()) {
                            pending.remove(id);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    !pending.is_empty()
}

/// Classify the trailing Copilot `events.jsonl` content as having an
/// open turn (last `assistant.message` not yet followed by a
/// `tool.execution_complete`).
///
/// Pure function — takes the tail content as a string. Returns `true`
/// when a turn is open, `false` otherwise (including parse errors).
pub fn copilot_tail_has_open_turn(tail: &str) -> bool {
    let mut last_assistant: Option<usize> = None;
    let mut last_tool_complete: Option<usize> = None;

    // Note: any partial first line at the start of the tail will fail
    // to parse as JSON and be skipped by the `Err(_) => continue` arm.
    // `idx` increments on every line including blank/parse-failed ones;
    // that's fine because the comparison `a > t` only uses relative
    // ordering and both branches observe the same indexing.
    for (idx, line) in tail.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        match v.get("type").and_then(|x| x.as_str()) {
            Some("assistant.message") => last_assistant = Some(idx),
            Some("tool.execution_complete") => last_tool_complete = Some(idx),
            _ => {}
        }
    }

    match (last_assistant, last_tool_complete) {
        (Some(a), Some(t)) => a > t,
        (Some(_), None) => true,
        _ => false,
    }
}

/// Read the last [`TAIL_BYTES`] of a file. Returns the bytes as a
/// `String` (lossy). Returns `None` on any I/O error or if the file is
/// empty — the caller treats `None` as "closed turn / Recent."
fn read_tail(path: &Path) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    if len == 0 {
        return None;
    }
    let start = len.saturating_sub(TAIL_BYTES);
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = Vec::with_capacity(TAIL_BYTES as usize);
    f.read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
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

    #[test]
    fn cc_tail_open_tool_use_detected() {
        let tail = r#"{"type":"user","message":{"content":"hi"}}
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"tool_1","name":"Bash","input":{}}]}}
"#;
        assert!(cc_tail_has_open_tool_use(tail));
    }

    #[test]
    fn cc_tail_open_tool_use_closed_when_result_present() {
        let tail = r#"{"type":"user","message":{"content":"hi"}}
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"tool_1","name":"Bash","input":{}}]}}
{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"tool_1","content":"ok"}]}}
"#;
        assert!(!cc_tail_has_open_tool_use(tail));
    }

    #[test]
    fn cc_tail_text_only_assistant_is_closed() {
        let tail = r#"{"type":"user","message":{"content":"hi"}}
{"type":"assistant","message":{"content":[{"type":"text","text":"ok"}]}}
"#;
        assert!(!cc_tail_has_open_tool_use(tail));
    }

    #[test]
    fn cc_tail_handles_legacy_string_content() {
        let tail = r#"{"type":"user","message":{"content":"hi"}}
{"type":"assistant","message":{"content":"ok"}}
"#;
        assert!(!cc_tail_has_open_tool_use(tail));
    }

    #[test]
    fn cc_tail_skips_first_partial_line() {
        let tail = r#"garbage_partial_line_here
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"tool_2","name":"Bash","input":{}}]}}
"#;
        assert!(cc_tail_has_open_tool_use(tail));
    }

    #[test]
    fn cc_tail_handles_malformed_lines() {
        let tail = r#"{"type":"user","message":{"content":"hi"}}
not json garbage
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"tool_3","name":"Bash","input":{}}]}}
"#;
        assert!(cc_tail_has_open_tool_use(tail));
    }

    #[test]
    fn cc_tail_empty_input_is_closed() {
        assert!(!cc_tail_has_open_tool_use(""));
    }

    #[test]
    fn cc_tail_multiple_tool_uses_one_unmatched() {
        let tail = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"a","name":"X","input":{}},{"type":"tool_use","id":"b","name":"Y","input":{}}]}}
{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"a","content":"ok"}]}}
"#;
        assert!(cc_tail_has_open_tool_use(tail));
    }

    #[test]
    fn copilot_tail_open_when_assistant_then_no_tool_complete() {
        let tail = r#"{"type":"user.message"}
{"type":"assistant.message"}
"#;
        assert!(copilot_tail_has_open_turn(tail));
    }

    #[test]
    fn copilot_tail_closed_when_tool_complete_after_assistant() {
        let tail = r#"{"type":"user.message"}
{"type":"assistant.message"}
{"type":"tool.execution_complete"}
"#;
        assert!(!copilot_tail_has_open_turn(tail));
    }

    #[test]
    fn copilot_tail_closed_when_no_assistant() {
        let tail = r#"{"type":"user.message"}
{"type":"system.notice"}
"#;
        assert!(!copilot_tail_has_open_turn(tail));
    }

    #[test]
    fn copilot_tail_open_when_only_assistant() {
        let tail = r#"{"type":"assistant.message"}
"#;
        assert!(copilot_tail_has_open_turn(tail));
    }

    #[test]
    fn copilot_tail_handles_malformed_lines() {
        let tail = r#"not json
{"type":"assistant.message"}
"#;
        // First line skipped (partial); the second line is `assistant.message`
        // and there's no later `tool.execution_complete`.
        assert!(copilot_tail_has_open_turn(tail));
    }

    #[test]
    fn copilot_tail_empty_input_is_closed() {
        assert!(!copilot_tail_has_open_turn(""));
    }
}
