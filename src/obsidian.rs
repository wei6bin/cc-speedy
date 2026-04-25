use crate::store::LearningPoint;
use crate::unified::UnifiedSession;
use anyhow::Result;

/// Escape a string for use inside a YAML double-quoted scalar. Backslash MUST
/// be escaped first so the other expansions don't double-escape its output.
fn yaml_dq_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

/// Parse the `## Status` line out of a factual summary body and normalise it.
/// Returns one of `"completed"`, `"in_progress"`, or `"unknown"`.
pub fn parse_status_from_factual(body: &str) -> &'static str {
    let mut lines = body.lines();
    while let Some(l) = lines.next() {
        if l.trim().eq_ignore_ascii_case("## Status") {
            // Read forward until the first non-empty line.
            for next in lines.by_ref() {
                let t = next.trim();
                if t.is_empty() {
                    continue;
                }
                let lc = t.to_ascii_lowercase();
                return match lc.as_str() {
                    "completed" => "completed",
                    "in progress" => "in_progress",
                    _ => "unknown",
                };
            }
        }
    }
    "unknown"
}

/// Build the ordered list of tags that go into the session note's frontmatter.
/// Order is deterministic so re-exports produce stable diffs.
///
/// `source` should be `"cc"`, `"oc"`, or `"co"`. `status` is the lower-snake form
/// from `parse_status_from_factual`.
pub fn build_frontmatter_tags(
    source: &str,
    status: &str,
    learnings: &[crate::store::LearningPoint],
) -> Vec<String> {
    let mut count_decisions = 0usize;
    let mut count_lessons = 0usize;
    let mut count_tools = 0usize;
    for l in learnings {
        match l.category.as_str() {
            "decision_points" => count_decisions += 1,
            "lessons_gotchas" => count_lessons += 1,
            "tools_commands" => count_tools += 1,
            _ => {}
        }
    }

    let mut tags: Vec<String> = Vec::with_capacity(16);
    tags.push("agent-session".to_string());
    tags.push(format!("cc-source/{}", source));
    tags.push(format!("cc-status/{}", status));

    // Counted slash-tags first.
    if count_decisions > 0 {
        tags.push(format!("cc-decisions/{}", count_decisions));
    }
    if count_lessons > 0 {
        tags.push(format!("cc-lessons/{}", count_lessons));
    }
    if count_tools > 0 {
        tags.push(format!("cc-tools/{}", count_tools));
    }

    // Bare facets second.
    if count_decisions > 0 {
        tags.push("cc-has-decisions".to_string());
    }
    if count_lessons > 0 {
        tags.push("cc-has-lessons".to_string());
    }
    if count_tools > 0 {
        tags.push("cc-has-tools".to_string());
    }

    tags
}

/// Compute the filename stem (filename minus `.md`) for a session note. Uses
/// the same project-slug + id-prefix scheme as `export_to_obsidian` so the
/// daily-note wikilink resolves to the right file.
pub fn note_stem_for_session(session: &UnifiedSession, date_str: &str) -> String {
    let project_slug: String = crate::util::path_last_n(&session.project_path, 2)
        .replace('/', "-")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    let id_prefix: String = session
        .session_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(8)
        .collect();
    format!("{}-{}-{}", date_str, project_slug, id_prefix)
}

/// Build the bullet line that gets appended to today's daily note.
pub fn build_daily_line(
    session: &UnifiedSession,
    note_stem: &str,
    status: &str,
    factual_title: &str,
) -> String {
    let emoji = match status {
        "completed" => "✅",
        "in_progress" => "🔧",
        _ => "🚧",
    };
    let title_truncated: String = factual_title.chars().take(80).collect();
    format!(
        "- [[{}]] **{}** · {} msgs · {} {} #cc-session",
        note_stem, session.project_name, session.message_count, emoji, title_truncated,
    )
}

/// Write a weekly digest markdown file to `<vault>/cc-speedy/digests/YYYY-Www.md`.
/// Returns the relative path (from vault root) for display.
pub fn export_digest(vault_path: &str, digest_text: &str) -> Result<String> {
    use chrono::{Datelike, Local};
    let now = Local::now();
    let year = now.iso_week().year();
    let week = now.iso_week().week();
    let filename = format!("{}-W{:02}.md", year, week);
    let rel_dir = std::path::Path::new("cc-speedy").join("digests");
    let abs_dir = std::path::Path::new(vault_path).join(&rel_dir);
    std::fs::create_dir_all(&abs_dir)?;
    let abs_path = abs_dir.join(&filename);
    let front = format!(
        "---\ndate: {}\ntype: weekly-digest\ntags: [cc-speedy, digest]\n---\n\n",
        now.format("%Y-%m-%d"),
    );
    let body = format!("{}```\n{}\n```\n", front, digest_text);
    std::fs::write(&abs_path, body)?;
    Ok(rel_dir.join(&filename).to_string_lossy().to_string())
}

/// Write a combined Obsidian Markdown note for a session.
/// Skips sessions with fewer than 5 messages.
/// Overwrites the file if it already exists.
pub fn export_to_obsidian(
    session: &UnifiedSession,
    factual: &str,
    learnings: &[LearningPoint],
    vault_path: &str,
) -> Result<()> {
    if session.message_count < 5 {
        return Ok(());
    }

    let date_str = chrono::Local::now().format("%Y-%m-%d").to_string();
    let last_exported = chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string();

    let stem = note_stem_for_session(session, &date_str);
    let filename = format!("{}.md", stem);
    let file_path = std::path::Path::new(vault_path).join(&filename);

    let source_str = match session.source {
        crate::unified::SessionSource::ClaudeCode => "cc",
        crate::unified::SessionSource::OpenCode => "oc",
        crate::unified::SessionSource::Copilot => "co",
    };
    let status = parse_status_from_factual(factual);
    let project_name = crate::util::path_last_n(&session.project_path, 1);
    let tags = build_frontmatter_tags(source_str, status, learnings);

    let mut front = String::new();
    front.push_str("---\n");
    front.push_str(&format!("date: {}\n", date_str));
    front.push_str(&format!(
        "project: \"{}\"\n",
        yaml_dq_escape(&session.project_path)
    ));
    front.push_str(&format!(
        "project_name: \"{}\"\n",
        yaml_dq_escape(&project_name)
    ));
    front.push_str(&format!(
        "session_id: \"{}\"\n",
        yaml_dq_escape(&session.session_id)
    ));
    front.push_str(&format!("source: \"{}\"\n", source_str));
    front.push_str(&format!("status: \"{}\"\n", status));
    front.push_str(&format!("message_count: {}\n", session.message_count));
    front.push_str(&format!("learnings_count: {}\n", learnings.len()));
    if !session.git_branch.is_empty() {
        front.push_str(&format!(
            "git_branch: \"{}\"\n",
            yaml_dq_escape(&session.git_branch)
        ));
    }
    front.push_str(&format!("last_exported: \"{}\"\n", last_exported));
    front.push_str("tags: [");
    for (i, t) in tags.iter().enumerate() {
        if i > 0 {
            front.push_str(", ");
        }
        front.push_str(t);
    }
    front.push_str("]\n");
    front.push_str("---\n\n");

    let mut content = format!("{}{}", front, factual);

    if !learnings.is_empty() {
        content.push_str("\n\n---\n");
        let categories = [
            ("decision_points", "## Decision points"),
            ("lessons_gotchas", "## Lessons & gotchas"),
            ("tools_commands", "## Tools & commands discovered"),
        ];
        for (cat, heading) in &categories {
            let items: Vec<&str> = learnings
                .iter()
                .filter(|l| l.category == *cat)
                .map(|l| l.point.as_str())
                .collect();
            if !items.is_empty() {
                content.push('\n');
                content.push_str(heading);
                content.push('\n');
                for item in items {
                    content.push_str("- ");
                    content.push_str(item);
                    content.push('\n');
                }
            }
        }
    }

    std::fs::write(&file_path, content)?;
    Ok(())
}
