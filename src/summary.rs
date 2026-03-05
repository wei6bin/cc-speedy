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

pub async fn generate_summary(_session_id: &str, _messages: &[Message]) -> Result<String> {
    Ok("stub".to_string())
}

pub async fn run_hook() -> Result<()> {
    Ok(())
}
