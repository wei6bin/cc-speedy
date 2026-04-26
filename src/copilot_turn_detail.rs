//! Per-turn detail extracted from a Copilot `events.jsonl`. Companion to
//! `turn_detail.rs` (Claude Code). Both produce the same `TurnDetail` so the
//! modal renderer is source-agnostic.

use crate::turn_detail::TurnDetail;
use anyhow::Result;
use std::path::Path;

/// Tool name that fires every Copilot turn as protocol noise. Suppressed from
/// the histogram and dominant-glyph picking but still rendered as a Tool block
/// in the modal.
pub(crate) const SUPPRESSED_TOOL: &str = "report_intent";

pub fn extract_turn(path: &Path, turn_idx: u32) -> Result<TurnDetail> {
    let content = std::fs::read_to_string(path)?;
    extract_turn_from_str(&content, turn_idx)
}

pub fn extract_turn_from_str(_content: &str, _turn_idx: u32) -> Result<TurnDetail> {
    unimplemented!("Task 2 will fill this in")
}
