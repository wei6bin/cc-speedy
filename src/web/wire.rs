//! Wire types for browser ↔ server JSON. These project the internal
//! domain types (`UnifiedSession`, `Liveness`, …) into a stable JSON
//! shape that the browser code in `app.js` consumes.

use crate::liveness::Liveness;
use crate::unified::{SessionSource, UnifiedSession};
use serde::Serialize;
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize)]
pub struct WireSession {
    pub session_id: String,
    pub source: WireSource,
    pub project_path: String,
    pub project_name: String,
    pub modified_unix_secs: u64,
    pub message_count: usize,
    pub first_user_msg: String,
    pub summary: String,
    pub liveness: WireLiveness,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WireSource {
    Cc,
    Oc,
    Co,
}

impl From<SessionSource> for WireSource {
    fn from(s: SessionSource) -> Self {
        match s {
            SessionSource::ClaudeCode => WireSource::Cc,
            SessionSource::OpenCode => WireSource::Oc,
            SessionSource::Copilot => WireSource::Co,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WireLiveness {
    Idle,
    Recent,
    Live,
}

impl From<Liveness> for WireLiveness {
    fn from(l: Liveness) -> Self {
        match l {
            Liveness::Idle => WireLiveness::Idle,
            Liveness::Recent => WireLiveness::Recent,
            Liveness::Live => WireLiveness::Live,
        }
    }
}

pub fn project(session: &UnifiedSession, liveness: Liveness) -> WireSession {
    WireSession {
        session_id: session.session_id.clone(),
        source: session.source.clone().into(),
        project_path: session.project_path.clone(),
        project_name: session.project_name.clone(),
        modified_unix_secs: session
            .modified
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        message_count: session.message_count,
        first_user_msg: session.first_user_msg.clone(),
        summary: session.summary.clone(),
        liveness: liveness.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    fn make_session(id: &str) -> UnifiedSession {
        UnifiedSession {
            session_id: id.to_string(),
            project_name: "p".to_string(),
            project_path: "/tmp/p".to_string(),
            modified: UNIX_EPOCH + Duration::from_secs(100),
            message_count: 5,
            first_user_msg: "hi".to_string(),
            summary: "test".to_string(),
            git_branch: String::new(),
            source: SessionSource::ClaudeCode,
            jsonl_path: None,
            archived: false,
        }
    }

    #[test]
    fn projects_unified_to_wire() {
        let s = make_session("a");
        let w = project(&s, Liveness::Live);
        assert_eq!(w.session_id, "a");
        assert!(matches!(w.source, WireSource::Cc));
        assert!(matches!(w.liveness, WireLiveness::Live));
        assert_eq!(w.modified_unix_secs, 100);
        assert_eq!(w.message_count, 5);
    }

    #[test]
    fn serializes_to_expected_json_shape() {
        let s = make_session("a");
        let w = project(&s, Liveness::Recent);
        let json = serde_json::to_value(&w).unwrap();
        assert_eq!(json["session_id"], "a");
        assert_eq!(json["source"], "cc");
        assert_eq!(json["liveness"], "recent");
        assert_eq!(json["modified_unix_secs"], 100);
    }
}
