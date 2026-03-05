use anyhow::Result;
use std::path::{Path, PathBuf};
use dirs::home_dir;
use crate::sessions::Message;

pub fn summaries_dir() -> PathBuf {
    home_dir().expect("HOME directory must be set").join(".claude").join("summaries")
}

pub fn summary_path(session_id: &str) -> PathBuf {
    summaries_dir().join(format!("{}.md", session_id))
}

pub fn read_summary(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

pub fn write_summary(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)?;
    Ok(())
}

pub async fn generate_summary(messages: &[Message]) -> Result<String> {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .map_err(|_| anyhow::anyhow!("ANTHROPIC_API_KEY not set"))?;

    // Take last 50 messages, format as conversation snippet
    let snippet: String = messages.iter().rev().take(50).rev()
        .map(|m| format!("{}: {}", m.role, m.text.chars().take(200).collect::<String>()))
        .collect::<Vec<_>>()
        .join("\n");

    let prompt = format!(
        "Summarize this Claude Code conversation in 3-5 bullet points.\n\
        Focus on: what was asked, what was done, files changed, final status.\n\
        Output markdown with ONLY these sections:\n\
        ## What was done\n- bullet\n\n## Files changed\n- file (or \"none\")\n\n## Status\nCompleted/In progress\n\n\
        Conversation:\n{}",
        snippet
    );

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&serde_json::json!({
            "model": "claude-haiku-4-5",
            "max_tokens": 512,
            "messages": [{"role": "user", "content": prompt}]
        }))
        .send()
        .await?;

    let body: serde_json::Value = resp.json().await?;
    let text = body["content"][0]["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("unexpected API response shape: {:?}", body))?
        .to_string();

    Ok(text)
}

pub async fn run_hook() -> Result<()> {
    // Claude Code sets these env vars in hook context
    let session_id = std::env::var("CLAUDE_SESSION_ID")
        .or_else(|_| std::env::var("SESSION_ID"))
        .unwrap_or_default();

    if session_id.is_empty() {
        eprintln!("cc-speedy: no CLAUDE_SESSION_ID in environment, skipping summary");
        return Ok(());
    }

    let out_path = summary_path(&session_id);
    if out_path.exists() {
        return Ok(());  // already summarized
    }

    let jsonl = find_jsonl(&session_id);
    let Some(jsonl_path) = jsonl else {
        eprintln!("cc-speedy: jsonl not found for session {}", session_id);
        return Ok(());
    };

    let messages = crate::sessions::parse_messages(std::path::Path::new(&jsonl_path))?;
    let summary = generate_summary(&messages).await?;
    write_summary(&out_path, &summary)?;
    eprintln!("cc-speedy: summary written to {:?}", out_path);
    Ok(())
}

pub fn find_jsonl(session_id: &str) -> Option<String> {
    let base = home_dir()?.join(".claude").join("projects");
    for proj in std::fs::read_dir(&base).ok()? {
        let Ok(proj) = proj else { continue; };
        let candidate = proj.path().join(format!("{}.jsonl", session_id));
        if candidate.exists() {
            return Some(candidate.to_string_lossy().to_string());
        }
    }
    None
}
