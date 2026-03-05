use anyhow::Result;
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

pub fn parse_messages(_path: &Path) -> Result<Vec<Message>> {
    Ok(vec![])
}

pub fn list_sessions() -> Result<Vec<Session>> {
    Ok(vec![])
}
