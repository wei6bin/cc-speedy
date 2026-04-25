use crate::unified::UnifiedSession;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

/// One project's activity inside a digest window.
pub struct ProjectDigest {
    pub project_path: String,
    pub name: String,
    pub session_count: usize,
    pub last_active: SystemTime,
    pub session_titles: Vec<String>,
}

/// One learning point within the digest window, annotated with its source
/// session's project name for display.
pub struct LearningLine {
    pub category: String,
    pub point: String,
    pub project_name: String,
    pub captured_at: i64,
}

/// A joined learning row (learnings.session_id already resolved to its project).
/// Caller constructs these from (session_id, category, point, captured_at)
/// + a sessions lookup for the project_name.
pub struct LearningWithSession {
    pub session_id: String,
    pub category: String,
    pub point: String,
    pub captured_at: i64,
}

pub struct DigestData {
    pub window_start: SystemTime,
    pub window_end: SystemTime,
    pub session_count: usize,
    pub learning_count: usize,
    pub projects: Vec<ProjectDigest>,
    pub learnings: Vec<LearningLine>,
}

pub fn build_digest(
    sessions: &[UnifiedSession],
    learnings: &[LearningWithSession],
    window_days: i64,
    now: SystemTime,
) -> DigestData {
    let window_start = now
        .checked_sub(Duration::from_secs(window_days as u64 * 86400))
        .unwrap_or(std::time::UNIX_EPOCH);
    let window_end = now;

    // Index sessions by id for learning lookup.
    let session_by_id: HashMap<&str, &UnifiedSession> = sessions
        .iter()
        .map(|s| (s.session_id.as_str(), s))
        .collect();

    // Sessions in the window.
    let in_window: Vec<&UnifiedSession> = sessions
        .iter()
        .filter(|s| s.modified >= window_start && s.modified <= window_end)
        .collect();

    // Group by project_path.
    let mut by_path: HashMap<String, ProjectDigest> = HashMap::new();
    for s in &in_window {
        let row = by_path
            .entry(s.project_path.clone())
            .or_insert_with(|| ProjectDigest {
                project_path: s.project_path.clone(),
                name: crate::util::path_last_n(&s.project_path, 2),
                session_count: 0,
                last_active: std::time::UNIX_EPOCH,
                session_titles: Vec::new(),
            });
        row.session_count += 1;
        if s.modified > row.last_active {
            row.last_active = s.modified;
        }
        let title = if !s.summary.is_empty() {
            s.summary.clone()
        } else {
            s.project_name.clone()
        };
        row.session_titles.push(title);
    }

    // Sort session titles within each project by... we lost the original modified
    // time here, so sort alphabetically as a stable fallback. Good enough for v1.
    for row in by_path.values_mut() {
        row.session_titles.sort();
        row.session_titles.dedup();
    }

    let mut projects: Vec<ProjectDigest> = by_path.into_values().collect();
    projects.sort_by(|a, b| b.last_active.cmp(&a.last_active));

    // Learnings in window.
    let window_start_secs = window_start
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let window_end_secs = window_end
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(i64::MAX);

    let mut learning_lines: Vec<LearningLine> = learnings
        .iter()
        .filter(|l| l.captured_at >= window_start_secs && l.captured_at <= window_end_secs)
        .map(|l| {
            let project_name = session_by_id
                .get(l.session_id.as_str())
                .map(|s| crate::util::path_last_n(&s.project_path, 2))
                .unwrap_or_else(|| "(unknown)".to_string());
            LearningLine {
                category: l.category.clone(),
                point: l.point.clone(),
                project_name,
                captured_at: l.captured_at,
            }
        })
        .collect();
    learning_lines.sort_by(|a, b| b.captured_at.cmp(&a.captured_at));

    DigestData {
        window_start,
        window_end,
        session_count: in_window.len(),
        learning_count: learning_lines.len(),
        projects,
        learnings: learning_lines,
    }
}

pub fn render_digest(d: &DigestData) -> String {
    let mut out = String::new();

    out.push_str("── Weekly Digest ─────────────────────────────────────\n");
    out.push_str(&format!(
        "  Window:     {} → {}\n",
        fmt_date(d.window_start),
        fmt_date(d.window_end),
    ));
    out.push_str(&format!(
        "  Sessions:   {}       Projects: {}       Learnings: {}\n",
        d.session_count,
        d.projects.len(),
        d.learning_count,
    ));

    if d.session_count == 0 && d.learning_count == 0 {
        out.push_str("\n  (No activity in this window.)\n");
        return out;
    }

    out.push_str("\n── By project ───────────────────────────────────────\n");
    for p in &d.projects {
        out.push_str(&format!(
            "▸ {}  ({} session{}, last {})\n",
            p.name,
            p.session_count,
            if p.session_count == 1 { "" } else { "s" },
            fmt_date(p.last_active),
        ));
        for title in &p.session_titles {
            out.push_str(&format!("    • {}\n", title));
        }
        out.push('\n');
    }

    if !d.learnings.is_empty() {
        out.push_str("── Learnings captured ───────────────────────────────\n");
        for l in &d.learnings {
            let tag = match l.category.as_str() {
                "decision_points" => "DEC",
                "lessons_gotchas" => "LSN",
                "tools_commands" => "TOL",
                _ => "???",
            };
            let ts =
                std::time::UNIX_EPOCH + std::time::Duration::from_secs(l.captured_at.max(0) as u64);
            out.push_str(&format!(
                "  [{}] {} — {} · {}\n",
                tag,
                l.point,
                l.project_name,
                fmt_date(ts),
            ));
        }
    }

    out
}

fn fmt_date(t: SystemTime) -> String {
    let secs = t
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let dt = chrono::DateTime::<chrono::Local>::from(
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs),
    );
    dt.format("%Y-%m-%d").to_string()
}
