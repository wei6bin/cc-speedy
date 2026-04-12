use anyhow::Result;
use crate::unified::UnifiedSession;
use crate::store::LearningPoint;

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

    // Project slug: last 2 path segments, slashes → dashes, sanitised
    let project_slug: String = crate::util::path_last_n(&session.project_path, 2)
        .replace('/', "-")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect();

    // First 8 alphanumeric-or-dash chars of session_id
    let id_prefix: String = session.session_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(8)
        .collect();

    let filename = format!("{}-{}-{}.md", date_str, project_slug, id_prefix);
    let file_path = std::path::Path::new(vault_path).join(&filename);

    let front_matter = format!(
        "---\ndate: {}\nproject: \"{}\"\nsession_id: \"{}\"\ntags: [agent-session]\n---\n\n",
        date_str,
        session.project_path.replace('"', "\\\""),
        session.session_id.replace('"', "\\\""),
    );

    let mut content = format!("{}{}", front_matter, factual);

    if !learnings.is_empty() {
        content.push_str("\n\n---\n");
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
