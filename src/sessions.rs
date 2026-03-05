use anyhow::Result;
use serde_json::Value;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Message {
    pub role: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct Session {
    pub session_id: String,
    pub project_name: String,
    pub project_path: String,
    pub modified: std::time::SystemTime,
    pub message_count: usize,
    pub first_user_msg: String,
    pub jsonl_path: String,
}

pub fn parse_messages(path: &Path) -> Result<Vec<Message>> {
    let content = std::fs::read_to_string(path)?;
    let mut msgs = Vec::new();
    for line in content.lines() {
        let Ok(v): Result<Value, _> = serde_json::from_str(line) else { continue };
        let role = v["type"].as_str().unwrap_or("").to_string();
        if role != "user" && role != "assistant" { continue; }
        let text = extract_text(&v["message"]["content"]);
        if text.is_empty() { continue; }
        msgs.push(Message { role, text });
    }
    Ok(msgs)
}

fn extract_text(content: &Value) -> String {
    if let Some(s) = content.as_str() {
        return s.to_string();
    }
    if let Some(arr) = content.as_array() {
        for item in arr {
            if item["type"] == "text" {
                if let Some(s) = item["text"].as_str() {
                    return s.to_string();
                }
            }
        }
    }
    String::new()
}

pub fn list_sessions() -> Result<Vec<Session>> {
    let claude_dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))?
        .join(".claude")
        .join("projects");

    let mut sessions = Vec::new();

    for proj_entry in std::fs::read_dir(&claude_dir)? {
        let proj_entry = proj_entry?;
        let proj_path = proj_entry.path();
        if !proj_path.is_dir() { continue; }

        for file_entry in std::fs::read_dir(&proj_path)? {
            let file_entry = file_entry?;
            let file_path = file_entry.path();
            if file_path.extension().and_then(|e| e.to_str()) != Some("jsonl") { continue; }

            let metadata = file_path.metadata()?;
            let modified = metadata.modified()?;

            let msgs = parse_messages(&file_path).unwrap_or_default();
            if msgs.len() < 4 { continue; }

            let first_user_text = msgs.iter()
                .find(|m| m.role == "user")
                .map(|m| m.text.as_str())
                .unwrap_or_default();

            if first_user_text.contains("local-command-caveat") && msgs.len() < 10 { continue; }

            let first_user_msg: String = first_user_text.chars().take(80).collect();

            let session_id = file_path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();

            let dir_name = proj_path.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let project_name = dir_name_to_path(&dir_name);
            let project_path_str = dir_name_to_abs_path(&dir_name);

            sessions.push(Session {
                session_id,
                project_name,
                project_path: project_path_str,
                modified,
                message_count: msgs.len(),
                first_user_msg,
                jsonl_path: file_path.to_string_lossy().to_string(),
            });
        }
    }

    sessions.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(sessions)
}

/// Convert Claude Code project dir name (e.g. "-home-weibin-repo-ai-foo") to an absolute path.
/// NOTE: This translation is ambiguous — a `-` in the dir name could be either a path separator
/// or an original hyphen in a directory name. For projects with hyphens in path components,
/// the reconstructed path may be incorrect.
pub fn dir_name_to_abs_path(dir_name: &str) -> String {
    let replaced = dir_name.replace('-', "/");
    let trimmed = replaced.trim_start_matches('/');
    format!("/{}", trimmed)
}

pub fn dir_name_to_path(dir_name: &str) -> String {
    let abs = dir_name_to_abs_path(dir_name);
    let parts: Vec<&str> = abs.trim_end_matches('/').split('/').filter(|s| !s.is_empty()).collect();
    match parts.len() {
        0 => abs.clone(),
        1 => parts[0].to_string(),
        n => parts[n-2..].join("/"),
    }
}
