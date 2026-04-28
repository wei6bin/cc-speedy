use cc_speedy::liveness::{detect, Liveness};
use cc_speedy::unified::{SessionSource, UnifiedSession};
use std::io::Write;
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

fn write_jsonl(dir: &TempDir, name: &str, content: &str) -> String {
    let path = dir.path().join(name);
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
    path.to_string_lossy().into_owned()
}

fn make_session(
    source: SessionSource,
    jsonl: Option<String>,
    modified: SystemTime,
) -> UnifiedSession {
    UnifiedSession {
        session_id: "sid".to_string(),
        project_name: "p".to_string(),
        project_path: "/tmp/p".to_string(),
        modified,
        message_count: 0,
        first_user_msg: String::new(),
        summary: String::new(),
        git_branch: String::new(),
        source,
        jsonl_path: jsonl,
        archived: false,
    }
}

#[test]
fn cc_live_when_unclosed_tool_use_and_fresh_mtime() {
    let dir = TempDir::new().unwrap();
    let path = write_jsonl(
        &dir,
        "live.jsonl",
        r#"{"type":"user","message":{"content":"hi"}}
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"tool_1","name":"Bash","input":{}}]}}
"#,
    );
    let s = make_session(SessionSource::ClaudeCode, Some(path), SystemTime::now());
    assert_eq!(detect(&s), Liveness::Live);
}

#[test]
fn cc_recent_when_closed_turn_and_fresh_mtime() {
    let dir = TempDir::new().unwrap();
    let path = write_jsonl(
        &dir,
        "recent.jsonl",
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"ok"}]}}
"#,
    );
    let s = make_session(SessionSource::ClaudeCode, Some(path), SystemTime::now());
    assert_eq!(detect(&s), Liveness::Recent);
}

#[test]
fn cc_idle_when_jsonl_path_missing() {
    let s = make_session(SessionSource::ClaudeCode, None, SystemTime::now());
    assert_eq!(detect(&s), Liveness::Idle);
}

#[test]
fn copilot_live_when_unclosed_assistant_event_and_fresh_mtime() {
    let dir = TempDir::new().unwrap();
    let path = write_jsonl(
        &dir,
        "events.jsonl",
        r#"{"type":"user.message"}
{"type":"assistant.message"}
"#,
    );
    let s = make_session(SessionSource::Copilot, Some(path), SystemTime::now());
    assert_eq!(detect(&s), Liveness::Live);
}

#[test]
fn copilot_recent_when_terminated() {
    let dir = TempDir::new().unwrap();
    let path = write_jsonl(
        &dir,
        "events.jsonl",
        r#"{"type":"user.message"}
{"type":"assistant.message"}
{"type":"tool.execution_complete"}
"#,
    );
    let s = make_session(SessionSource::Copilot, Some(path), SystemTime::now());
    assert_eq!(detect(&s), Liveness::Recent);
}

#[test]
fn opencode_recent_when_inside_window() {
    let s = make_session(
        SessionSource::OpenCode,
        None,
        SystemTime::now() - Duration::from_secs(30),
    );
    assert_eq!(detect(&s), Liveness::Recent);
}

#[test]
fn opencode_idle_when_old() {
    let s = make_session(
        SessionSource::OpenCode,
        None,
        SystemTime::now() - Duration::from_secs(3600),
    );
    assert_eq!(detect(&s), Liveness::Idle);
}

#[test]
fn missing_jsonl_file_returns_idle() {
    let s = make_session(
        SessionSource::ClaudeCode,
        Some("/nonexistent/path.jsonl".to_string()),
        SystemTime::now(),
    );
    assert_eq!(detect(&s), Liveness::Idle);
}
