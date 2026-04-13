use crate::sessions::Message;
use crate::unified::{SessionSource, UnifiedSession};
use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};
use std::path::PathBuf;
use std::time::{Duration, UNIX_EPOCH};

/// Path to the OpenCode SQLite database (~/.local/share/opencode/opencode.db).
/// Returns None if the data_local_dir cannot be determined.
pub fn opencode_db_path() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| d.join("opencode").join("opencode.db"))
}

/// Read all messages for an OpenCode session from the real DB.
/// Returns Ok(vec![]) if the DB does not exist.
/// Message roles come from `message.data` JSON `role` field.
/// Message text comes from `part.data` JSON `text` field (type="text" parts only).
pub fn parse_opencode_messages(session_id: &str) -> Result<Vec<Message>> {
    let path = match opencode_db_path() {
        Some(p) if p.exists() => p,
        _ => return Ok(vec![]),
    };
    let conn = Connection::open_with_flags(
        &path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    parse_opencode_messages_from_conn(&conn, session_id)
}

/// Query messages from an open connection (also used in tests).
pub fn parse_opencode_messages_from_conn(
    conn: &Connection,
    session_id: &str,
) -> Result<Vec<Message>> {
    // Each message has a role stored in its JSON data field.
    // Text content lives in part rows whose data JSON has type="text".
    // We group all text parts per message, ordered by message creation time.
    let mut stmt = conn.prepare(
        "
        SELECT
            json_extract(m.data, '$.role') AS role,
            p.data AS part_data
        FROM part p
        JOIN message m ON m.id = p.message_id
        WHERE m.session_id = ?1
          AND p.data LIKE '{\"type\":\"text\"%'
        ORDER BY m.time_created ASC, p.time_created ASC
    ",
    )?;

    let rows: Vec<(String, String)> = stmt
        .query_map([session_id], |row| {
            Ok((
                row.get::<_, String>(0).unwrap_or_default(),
                row.get::<_, String>(1)?,
            ))
        })?
        .filter_map(|r| match r {
            Ok(v) => Some(v),
            Err(e) => {
                #[cfg(debug_assertions)]
                eprintln!("cc-speedy: skipping malformed part row: {}", e);
                None
            }
        })
        .collect();

    let mut messages = Vec::with_capacity(rows.len());
    for (role, part_data) in rows {
        let v: serde_json::Value = match serde_json::from_str(&part_data) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let text = match v["text"].as_str() {
            Some(t) if !t.is_empty() => t.to_string(),
            _ => continue,
        };
        messages.push(Message { role, text });
    }
    Ok(messages)
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
    let mut stmt = conn.prepare(
        "
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
    ",
    )?;

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
        .filter_map(|r| match r {
            Ok(v) => Some(v),
            Err(e) => {
                #[cfg(debug_assertions)]
                eprintln!("cc-speedy: skipping malformed session row: {}", e);
                None
            }
        })
        .collect();

    let mut sessions = Vec::with_capacity(rows.len());
    for (id, title, time_updated_ms, worktree, message_count) in rows {
        let first_user_msg = query_first_user_text(conn, &id)
            .unwrap_or_default()
            .chars()
            .take(80)
            .collect();

        let modified = UNIX_EPOCH + Duration::from_millis(time_updated_ms.max(0) as u64);

        let project_name = crate::util::path_last_n(&worktree, 2);

        sessions.push(UnifiedSession {
            session_id: id,
            project_name,
            project_path: worktree,
            modified,
            message_count,
            first_user_msg,
            summary: title,
            git_branch: String::new(),
            source: SessionSource::OpenCode,
            jsonl_path: opencode_db_path().map(|p| p.to_string_lossy().into_owned()),
            archived: false,
        });
    }
    Ok(sessions)
}

/// Retrieve the text of the first user `part` in a session.
fn query_first_user_text(conn: &Connection, session_id: &str) -> Option<String> {
    // Parts of type "text" from the earliest message in the session.
    // We extract the "text" field from the JSON data column.
    let result: Option<String> = conn
        .query_row(
            "SELECT p.data
         FROM part p
         JOIN message m ON m.id = p.message_id
         WHERE m.session_id = ?1
           AND json_extract(m.data, '$.role') = 'user'
           AND p.data LIKE '{\"type\":\"text\"%'
         ORDER BY p.time_created ASC
         LIMIT 1",
            [session_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .ok()?;

    let data = result?;
    let v: serde_json::Value = serde_json::from_str(&data).ok()?;
    v["text"].as_str().map(|s| s.to_string())
}
