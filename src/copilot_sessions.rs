use anyhow::Result;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};
use crate::sessions::Message;
use crate::unified::{UnifiedSession, SessionSource};

pub fn copilot_sessions_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".copilot").join("session-state"))
}

struct WorkspaceYaml {
    id: String,
    cwd: String,
    summary: Option<String>,
    name: Option<String>,
    updated_at: Option<String>,
    branch: Option<String>,
}

fn parse_workspace_yaml(path: &Path) -> Option<WorkspaceYaml> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut id = String::new();
    let mut cwd = String::new();
    let mut summary = None;
    let mut name = None;
    let mut updated_at = None;
    let mut branch = None;

    for line in content.lines() {
        // split_once(':') splits on the FIRST colon only, so ISO timestamps
        // (e.g. "2026-03-12T01:18:55Z") come through intact in the value.
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim();
            let val = v.trim().to_string();
            if val.is_empty() { continue; }
            match key {
                "id"         => id = val,
                "cwd"        => cwd = val,
                "summary"    => summary = Some(val),
                "name"       => name = Some(val),
                "updated_at" => updated_at = Some(val),
                "branch"     => branch = Some(val),
                _            => {}
            }
        }
    }

    if id.is_empty() || cwd.is_empty() { return None; }
    Some(WorkspaceYaml { id, cwd, summary, name, updated_at, branch })
}

fn parse_iso8601(s: &str) -> Option<std::time::SystemTime> {
    use chrono::DateTime;
    let dt = DateTime::parse_from_rfc3339(s).ok()?;
    let secs = dt.timestamp();
    if secs < 0 { return None; }
    Some(UNIX_EPOCH + Duration::from_secs(secs as u64))
}

fn count_messages_and_first(path: &Path) -> (usize, String) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return (0, String::new()),
    };
    let mut count = 0usize;
    let mut first_user = String::new();
    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        let t = v["type"].as_str().unwrap_or("");
        match t {
            "user.message" | "assistant.message" => {
                count += 1;
                if t == "user.message" && first_user.is_empty() {
                    if let Some(text) = v["data"]["content"].as_str() {
                        first_user = text.chars().take(80).collect();
                    }
                }
            }
            _ => {}
        }
    }
    (count, first_user)
}

pub fn list_copilot_sessions() -> Result<Vec<UnifiedSession>> {
    let base = match copilot_sessions_dir() {
        Some(p) if p.exists() => p,
        _ => return Ok(vec![]),
    };
    list_copilot_sessions_from_dir(&base)
}

pub fn list_copilot_sessions_from_dir(base: &Path) -> Result<Vec<UnifiedSession>> {
    let entries = match std::fs::read_dir(base) {
        Ok(e) => e,
        Err(_) => return Ok(vec![]),
    };
    let mut sessions = Vec::new();
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let ws = match parse_workspace_yaml(&path.join("workspace.yaml")) {
            Some(ws) => ws,
            None => continue,
        };
        let (message_count, first_user_msg) =
            count_messages_and_first(&path.join("events.jsonl"));
        if message_count < 4 { continue; }
        let modified = ws.updated_at
            .as_deref()
            .and_then(parse_iso8601)
            .unwrap_or(UNIX_EPOCH);
        let title = ws.name.or(ws.summary).unwrap_or_default();
        let project_name = crate::util::path_last_n(&ws.cwd, 2);
        sessions.push(UnifiedSession {
            session_id:    ws.id,
            project_name,
            project_path:  ws.cwd,
            modified,
            message_count,
            first_user_msg,
            summary:       title,
            git_branch:    ws.branch.unwrap_or_default(),
            source:        SessionSource::Copilot,
            jsonl_path:    Some(path.join("events.jsonl").to_string_lossy().into_owned()),
        });
    }
    Ok(sessions)
}

pub fn parse_copilot_messages(session_id: &str) -> Result<Vec<Message>> {
    let path = match copilot_sessions_dir() {
        Some(d) => d.join(session_id).join("events.jsonl"),
        None => return Ok(vec![]),
    };
    if !path.exists() { return Ok(vec![]); }
    parse_copilot_messages_from_path(&path)
}

pub fn parse_copilot_messages_from_path(path: &Path) -> Result<Vec<Message>> {
    let content = std::fs::read_to_string(path)?;
    let mut messages = Vec::new();
    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        let role = match v["type"].as_str().unwrap_or("") {
            "user.message"      => "user",
            "assistant.message" => "assistant",
            _                   => continue,
        };
        let text = v["data"]["content"].as_str().unwrap_or("");
        if text.is_empty() { continue; }
        messages.push(Message { role: role.to_string(), text: text.to_string() });
    }
    Ok(messages)
}
