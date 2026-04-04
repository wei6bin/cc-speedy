#![allow(unused_imports)]

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};
use crate::sessions::Message;
use crate::unified::{UnifiedSession, SessionSource};

pub fn copilot_sessions_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".copilot").join("session-state"))
}

pub fn list_copilot_sessions() -> Result<Vec<UnifiedSession>> {
    Ok(vec![])
}

pub fn list_copilot_sessions_from_dir(_base: &Path) -> Result<Vec<UnifiedSession>> {
    Ok(vec![])
}

pub fn parse_copilot_messages(_session_id: &str) -> Result<Vec<Message>> {
    Ok(vec![])
}

pub fn parse_copilot_messages_from_path(_path: &Path) -> Result<Vec<Message>> {
    Ok(vec![])
}
