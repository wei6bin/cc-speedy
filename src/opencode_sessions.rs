use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};
use std::path::PathBuf;
use std::time::{Duration, UNIX_EPOCH};
use crate::unified::{UnifiedSession, SessionSource};

/// Path to the OpenCode SQLite database (~/.local/share/opencode/opencode.db).
/// Returns None if the data_local_dir cannot be determined.
pub fn opencode_db_path() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| d.join("opencode").join("opencode.db"))
}

/// List all top-level, non-archived OpenCode sessions from the real DB.
/// Returns Ok(vec![]) if the DB does not exist (OpenCode not installed).
pub fn list_opencode_sessions() -> Result<Vec<UnifiedSession>> {
    let path = match opencode_db_path() {
        Some(p) if p.exists() => p,
        _ => return Ok(vec![]),
    };
    let conn = Connection::open(&path)?;
    query_sessions_from_conn(&conn)
}

/// Query sessions from an open connection (also used in tests with in-memory DB).
pub fn query_sessions_from_conn(conn: &Connection) -> Result<Vec<UnifiedSession>> {
    let mut stmt = conn.prepare("
        SELECT
            s.id,
            COALESCE(s.title, ''),
            s.time_updated,
            p.worktree,
            COUNT(DISTINCT m.id) AS message_count
        FROM session s
        JOIN project p ON p.id = s.project_id
        LEFT JOIN message m ON m.session_id = s.id
        WHERE s.time_archived IS NULL
          AND s.parent_id IS NULL
        GROUP BY s.id
        ORDER BY s.time_updated DESC
    ")?;

    let rows: Vec<(String, String, i64, String, usize)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, usize>(4)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    let mut sessions = Vec::with_capacity(rows.len());
    for (id, title, time_updated_ms, worktree, message_count) in rows {
        let first_user_msg = query_first_user_text(conn, &id)
            .unwrap_or_default()
            .chars()
            .take(80)
            .collect();

        let modified = UNIX_EPOCH
            + Duration::from_millis(time_updated_ms.max(0) as u64);

        let project_name = path_last_two(&worktree);

        sessions.push(UnifiedSession {
            session_id:    id,
            project_name,
            project_path:  worktree,
            modified,
            message_count,
            first_user_msg,
            summary:       title,
            git_branch:    String::new(),
            source:        SessionSource::OpenCode,
            jsonl_path:    None,
        });
    }
    Ok(sessions)
}

/// Retrieve the text of the first user `part` in a session.
fn query_first_user_text(conn: &Connection, session_id: &str) -> Option<String> {
    // Parts of type "text" from the earliest message in the session.
    // We extract the "text" field from the JSON data column.
    let result: Option<String> = conn.query_row(
        "SELECT p.data
         FROM part p
         JOIN message m ON m.id = p.message_id
         WHERE m.session_id = ?1
           AND p.data LIKE '{\"type\":\"text\"%'
         ORDER BY p.time_created ASC
         LIMIT 1",
        [session_id],
        |row| row.get::<_, String>(0),
    ).optional().ok()?;

    let data = result?;
    let v: serde_json::Value = serde_json::from_str(&data).ok()?;
    v["text"].as_str().map(|s| s.to_string())
}

/// Return the last two path segments joined with "/", e.g. "/home/user/ai/proj" → "ai/proj".
fn path_last_two(path: &str) -> String {
    let parts: Vec<&str> = path.trim_end_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    match parts.len() {
        0 => path.to_string(),
        1 => parts[0].to_string(),
        n => parts[n - 2..].join("/"),
    }
}
