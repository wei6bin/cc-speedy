use anyhow::Result;
use std::path::{Path, PathBuf};
use crate::sessions::Message;
use crate::unified::UnifiedSession;

pub fn copilot_sessions_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".copilot").join("session-state"))
}

pub fn list_copilot_sessions() -> Result<Vec<UnifiedSession>> {
    Ok(vec![])
}

pub fn list_copilot_sessions_from_dir(_base: &Path) -> Result<Vec<UnifiedSession>> {
    Ok(vec![])
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
