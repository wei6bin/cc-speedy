//! Per-session insights extracted from a Copilot `events.jsonl`. Companion to
//! `insights.rs` (Claude Code). Both produce the same `SessionInsights` so the
//! Insights panel renderer is source-agnostic.

use crate::insights::SessionInsights;
use anyhow::Result;
use std::path::Path;

pub fn parse_insights(path: &Path) -> Result<SessionInsights> {
    let content = std::fs::read_to_string(path)?;
    Ok(parse_insights_from_str(&content))
}

pub fn parse_insights_from_str(_content: &str) -> SessionInsights {
    unimplemented!("Task 7 will fill this in")
}
