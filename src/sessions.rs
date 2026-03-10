use anyhow::Result;
use serde::Deserialize;
use serde_json::Value;
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

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
    pub summary: String,
    pub git_branch: String,
}

#[derive(Debug, Deserialize)]
pub struct SessionIndex {
    #[serde(default)]
    pub entries: Vec<SessionEntry>,
    #[serde(rename = "originalPath", default)]
    pub original_path: String,
}

#[derive(Debug, Deserialize)]
pub struct SessionEntry {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "fullPath")]
    pub full_path: String,
    #[serde(rename = "fileMtime")]
    pub file_mtime: u64,
    #[serde(rename = "firstPrompt", default)]
    pub first_prompt: String,
    #[serde(default)]
    pub summary: String,
    #[serde(rename = "messageCount", default)]
    pub message_count: usize,
    #[serde(default)]
    pub modified: String,
    #[serde(rename = "gitBranch", default)]
    pub git_branch: String,
    #[serde(rename = "isSidechain", default)]
    pub is_sidechain: bool,
}

pub fn parse_messages(path: &Path) -> Result<Vec<Message>> {
    let content = std::fs::read_to_string(path)?;
    let mut msgs = Vec::new();
    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        let role = v["type"].as_str().unwrap_or("").to_string();
        if role != "user" && role != "assistant" { continue; }
        let text = extract_text(&v["message"]["content"]);
        if text.is_empty() { continue; }
        msgs.push(Message { role, text });
    }
    Ok(msgs)
}

/// Read the actual cwd from the first line of a JSONL file.
/// Every JSONL entry has a "cwd" field — this is far more accurate than
/// reconstructing the path from the directory name (which has hyphen ambiguity).
pub fn read_cwd_from_jsonl(path: &Path) -> Option<String> {
    use std::io::BufRead;
    let file = std::fs::File::open(path).ok()?;
    let reader = std::io::BufReader::new(file);
    for line in reader.lines() {
        let Ok(line) = line else { continue };
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        if let Some(cwd) = v["cwd"].as_str() {
            if !cwd.is_empty() {
                return Some(cwd.to_string());
            }
        }
    }
    None
}

/// Extract the session rename/summary title from a JSONL file.
/// Claude Code writes `{"type":"summary","summary":"..."}` when /rename is used
/// or auto-generates a one-liner title.
pub fn parse_session_title(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    // The summary entry is typically near the end — scan all, keep last one found
    let mut title = None;
    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        if v["type"].as_str() == Some("summary") {
            if let Some(s) = v["summary"].as_str() {
                if !s.is_empty() {
                    title = Some(s.to_string());
                }
            }
        }
    }
    title
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

/// Read ~/.claude/history.jsonl and extract the latest /rename title per session.
/// History entries look like: {"display":"/rename my-title","sessionId":"...","timestamp":...}
pub fn read_rename_history() -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let path = match dirs::home_dir() {
        Some(h) => h.join(".claude").join("history.jsonl"),
        None => return map,
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return map,
    };
    // Collect (timestamp, session_id, rename_title) triples, then keep latest per session
    let mut entries: Vec<(u64, String, String)> = Vec::new();
    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        let session_id = match v["sessionId"].as_str() {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => continue,
        };
        let display = match v["display"].as_str() {
            Some(s) => s,
            None => continue,
        };
        // First line of display, stripping leading `'` (artifact from cc-speedy display)
        let first_line = display.lines().next().unwrap_or("").trim_start_matches('\'');
        let title = if let Some(t) = first_line.strip_prefix("/rename ") {
            t.trim().to_string()
        } else {
            continue;
        };
        if title.is_empty() { continue; }
        let ts = v["timestamp"].as_u64().unwrap_or(0);
        entries.push((ts, session_id, title));
    }
    // Sort by timestamp, then insert — later entries overwrite earlier ones
    entries.sort_by_key(|(ts, _, _)| *ts);
    for (_, session_id, title) in entries {
        map.insert(session_id, title);
    }
    map
}

/// Append a /rename entry to ~/.claude/history.jsonl so Claude Code picks it up.
pub fn write_rename(session_id: &str, title: &str) -> anyhow::Result<()> {
    let path = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("no home dir"))?
        .join(".claude")
        .join("history.jsonl");
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as u64;
    let entry = serde_json::json!({
        "display": format!("/rename {}", title),
        "sessionId": session_id,
        "timestamp": ts,
    });
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().append(true).create(true).open(&path)?;
    writeln!(file, "{}", entry)?;
    Ok(())
}

fn read_sessions_index(proj_path: &Path) -> Option<SessionIndex> {
    let content = std::fs::read_to_string(proj_path.join("sessions-index.json")).ok()?;
    serde_json::from_str::<SessionIndex>(&content).ok()
}

fn path_to_display_name(abs_path: &str) -> String {
    crate::util::path_last_n(abs_path, 2)
}

pub fn list_sessions() -> Result<Vec<Session>> {
    let claude_dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))?
        .join(".claude")
        .join("projects");

    let mut sessions = Vec::new();
    let renames = read_rename_history();

    'proj: for proj_entry in std::fs::read_dir(&claude_dir)? {
        let proj_entry = proj_entry?;
        let proj_path = proj_entry.path();
        if !proj_path.is_dir() { continue; }

        // Index path: try sessions-index.json first
        if let Some(index) = read_sessions_index(&proj_path) {
            let project_path = index.original_path.clone();
            let project_name = path_to_display_name(&project_path);

            for entry in &index.entries {
                if entry.is_sidechain { continue; }
                if entry.message_count < 4 { continue; }
                if entry.first_prompt.contains("local-command-caveat") && entry.message_count < 10 { continue; }

                let modified = UNIX_EPOCH + Duration::from_millis(entry.file_mtime);
                let first_user_msg: String = entry.first_prompt.chars().take(80).collect();

                let summary = renames.get(&entry.session_id)
                    .cloned()
                    .unwrap_or_else(|| entry.summary.clone());
                sessions.push(Session {
                    session_id: entry.session_id.clone(),
                    project_name: project_name.clone(),
                    project_path: project_path.clone(),
                    modified,
                    message_count: entry.message_count,
                    first_user_msg,
                    jsonl_path: entry.full_path.clone(),
                    summary,
                    git_branch: entry.git_branch.clone(),
                });
            }
            continue 'proj;
        }

        // Fallback path: existing jsonl parsing logic
        let Ok(dir_iter) = std::fs::read_dir(&proj_path) else { continue; };
        for file_entry in dir_iter {
            let Ok(file_entry) = file_entry else { continue; };
            let file_path = file_entry.path();
            if file_path.extension().and_then(|e| e.to_str()) != Some("jsonl") { continue; }

            let Ok(metadata) = file_path.metadata() else { continue; };
            let Ok(modified) = metadata.modified() else { continue; };

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
            // Prefer cwd from JSONL (accurate) over dir-name decoding (has hyphen ambiguity)
            let project_path_str = read_cwd_from_jsonl(&file_path)
                .unwrap_or_else(|| dir_name_to_abs_path(&dir_name));
            let project_name = path_to_display_name(&project_path_str);

            let jsonl_title = parse_session_title(&file_path).unwrap_or_default();
            let summary = renames.get(&session_id)
                .cloned()
                .unwrap_or(jsonl_title);

            sessions.push(Session {
                session_id,
                project_name,
                project_path: project_path_str,
                modified,
                message_count: msgs.len(),
                first_user_msg,
                jsonl_path: file_path.to_string_lossy().to_string(),
                summary,
                git_branch: String::new(),
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
