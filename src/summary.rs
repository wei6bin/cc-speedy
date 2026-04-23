use anyhow::Result;
use std::path::{Path, PathBuf};
use dirs::home_dir;
use crate::sessions::Message;

// Uses `claude -p` (Claude Code CLI) so no separate API key is needed —
// authentication comes from your existing Claude subscription.

pub fn summaries_dir() -> PathBuf {
    home_dir().expect("HOME directory must be set").join(".claude").join("summaries")
}

pub fn summary_path(session_id: &str) -> PathBuf {
    // Sanitize: keep only chars valid in a UUID/session-id to prevent path traversal
    let safe: String = session_id.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    summaries_dir().join(format!("{}.md", safe))
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

/// Strip `[📷 filename]` and similar attachment reference tokens from message text.
/// The Claude CLI interprets `[emoji path]` as a file-read request, which hangs
/// indefinitely in non-interactive mode. We keep the text that follows the `]`.
fn strip_attachment_refs(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '[' {
            // Collect the bracket contents to check if it looks like an attachment
            let mut inner = String::new();
            let mut closed = false;
            for ch in chars.by_ref() {
                if ch == ']' { closed = true; break; }
                inner.push(ch);
            }
            // Heuristic: attachment refs contain an emoji and a filename (has a '.')
            let looks_like_attachment = closed
                && inner.chars().next().map(|c| !c.is_alphanumeric()).unwrap_or(false)
                && inner.contains('.');
            if !looks_like_attachment {
                result.push('[');
                result.push_str(&inner);
                if closed { result.push(']'); }
            }
            // Either way, skip the leading space after the `]` if present
            if closed && looks_like_attachment {
                if chars.peek() == Some(&' ') { chars.next(); }
            }
        } else {
            result.push(c);
        }
    }
    result
}

pub async fn generate_summary(
    messages: &[Message],
    existing_learnings: &[crate::store::LearningPoint],
) -> Result<(String, Vec<crate::store::LearningPoint>)> {
    // Take last 50 messages. Strip [📷 file] attachment refs first — when included
    // verbatim in a `claude --print` prompt they cause the CLI to attempt file reads,
    // which hangs indefinitely in non-interactive mode (e.g. Copilot sessions).
    let snippet: String = messages.iter().rev().take(50).rev()
        .map(|m| {
            let text = strip_attachment_refs(&m.text).chars().take(200).collect::<String>();
            format!("{}: {}", m.role, text)
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Format existing learnings so Claude knows what's already captured
    let existing_text = if existing_learnings.is_empty() {
        "(none)".to_string()
    } else {
        existing_learnings.iter()
            .map(|l| format!("[{}] {}", l.category, l.point))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let prompt = format!(
        "Analyze this AI coding session and produce exactly two sections separated by the delimiter <!-- LEARNINGS -->.\n\
        \n\
        <INSTRUCTIONS: output these exact headings and bullets only — do not reproduce this instruction line>\n\
        ## What was done\n- bullet (3-5 bullets max)\n\
        \n\
        ## Files changed\n- file path (or \"none\")\n\
        \n\
        ## Status\nCompleted / In progress\n\
        \n\
        ## Problem context\n1-2 sentences on what problem was being solved and why\n\
        \n\
        ## Approach taken\nKey steps and decisions (2-4 bullets)\n\
        \n\
        <!-- LEARNINGS -->\n\
        \n\
        <INSTRUCTIONS: extract ONLY new points not already listed in EXISTING LEARNINGS — do not reproduce this instruction line>\n\
        ## Decision points\n- technical design choice: brief rationale (or \"none\")\n\
        \n\
        ## Lessons & gotchas\n- surprise, pitfall, or thing to do differently (or \"none\")\n\
        \n\
        ## Tools & commands discovered\n- CLI flag/library/API found (or \"none\")\n\
        \n\
        EXISTING LEARNINGS (do not repeat these):\n\
        {}\n\
        \n\
        Conversation:\n{}",
        existing_text, snippet
    );

    // Note: proxy env vars (ANTHROPIC_AUTH_TOKEN/BASE_URL/MODEL/API_KEY) are
    // stripped once at process startup in main.rs, so `claude --print` inherits
    // a clean environment and uses the user's default subscription.
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(180),
        tokio::process::Command::new("claude")
            .args(["--print", &prompt])
            .output(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("claude --print timed out after 180 seconds"))?
    .map_err(|e| anyhow::anyhow!("failed to run `claude`: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = if !stderr.trim().is_empty() {
            stderr.to_string()
        } else if !stdout.trim().is_empty() {
            stdout.to_string()
        } else {
            format!("exit code {:?}", output.status.code())
        };
        anyhow::bail!("claude --print failed: {}", detail);
    }

    let text = String::from_utf8(output.stdout)
        .map_err(|e| anyhow::anyhow!("claude output was not valid UTF-8: {}", e))?;
    let text = text.trim();

    // Split on the delimiter — graceful degradation if missing
    let (factual, learning_md) = match text.split_once("<!-- LEARNINGS -->") {
        Some((f, l)) => (f.trim().to_string(), l.trim().to_string()),
        None => (text.to_string(), String::new()),
    };

    let new_points = parse_learning_output(&learning_md);
    Ok((factual, new_points))
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

    let conn = crate::store::open_db()?;

    // Skip if already summarised
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM summaries WHERE session_id = ?1",
            rusqlite::params![session_id],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0)
        > 0;
    if exists {
        return Ok(());
    }

    let jsonl = find_jsonl(&session_id);
    let Some(jsonl_path) = jsonl else {
        eprintln!("cc-speedy: jsonl not found for session {}", session_id);
        return Ok(());
    };

    let messages = crate::sessions::parse_messages(std::path::Path::new(&jsonl_path))?;
    let existing_learnings = crate::store::load_learnings(&conn, &session_id).unwrap_or_default();
    let (summary_text, new_points) = generate_summary(&messages, &existing_learnings).await?;
    crate::store::save_summary(&conn, &session_id, "cc", &summary_text)?;
    if !new_points.is_empty() {
        crate::store::save_learnings(&conn, &session_id, &new_points)?;
    }
    eprintln!("cc-speedy: summary saved to db for session {}", session_id);
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

/// Parse the learning section of the enriched prompt output into structured points.
/// Recognises headings "## Decision points", "## Lessons & gotchas", "## Tools & commands discovered".
/// Bullets containing only "none" (case-insensitive) are skipped.
pub fn parse_learning_output(learning_md: &str) -> Vec<crate::store::LearningPoint> {
    let mut points = Vec::new();
    let mut current_category: Option<&'static str> = None;

    for line in learning_md.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("## ") {
            let heading = trimmed.trim_start_matches("## ").trim_end_matches(':').to_lowercase();
            current_category = match heading.as_str() {
                "decision points" | "decision_points" => Some("decision_points"),
                "lessons & gotchas" | "lessons_&_gotchas" | "lessons and gotchas" => Some("lessons_gotchas"),
                "tools & commands discovered" | "tools_&_commands_discovered" | "tools and commands discovered" => Some("tools_commands"),
                _ => None,
            };
        } else if trimmed.starts_with("- ") {
            if let Some(cat) = current_category {
                let point = trimmed.trim_start_matches("- ").trim().to_string();
                if !point.is_empty() && !point.to_lowercase().trim_matches(|c| c == '(' || c == ')').trim().eq("none") {
                    points.push(crate::store::LearningPoint { category: cat.to_string(), point });
                }
            }
        }
    }
    points
}

/// Build the combined display string for the TUI preview pane:
/// factual summary first, then accumulated learning points grouped by category.
pub fn build_combined_display(factual: &str, learnings: &[crate::store::LearningPoint]) -> String {
    if learnings.is_empty() {
        return factual.to_string();
    }

    let mut out = String::from(factual);
    out.push_str("\n\n── Knowledge Capture ──────────────────────");

    let categories = [
        ("decision_points",  "## Decision points"),
        ("lessons_gotchas",  "## Lessons & gotchas"),
        ("tools_commands",   "## Tools & commands discovered"),
    ];

    for (cat, heading) in &categories {
        let items: Vec<&str> = learnings.iter()
            .filter(|l| l.category == *cat)
            .map(|l| l.point.as_str())
            .collect();
        if !items.is_empty() {
            out.push('\n');
            out.push_str(heading);
            for item in items {
                out.push_str("\n- ");
                out.push_str(item);
            }
        }
    }
    out
}

/// Path for OpenCode session summaries.
/// Stored under ~/.local/share/opencode/summaries/<session-id>.md
pub fn opencode_summary_path(session_id: &str) -> PathBuf {
    let safe: String = session_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    dirs::data_local_dir()
        .expect("data_local_dir must be set")
        .join("opencode")
        .join("summaries")
        .join(format!("{}.md", safe))
}
