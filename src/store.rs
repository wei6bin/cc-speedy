use anyhow::Result;
use rusqlite::{Connection, params};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct LearningPoint {
    pub category: String,  // "decision_points" | "lessons_gotchas" | "tools_commands"
    pub point:    String,
}

pub fn db_path() -> PathBuf {
    dirs::data_local_dir()
        .expect("data_local_dir must be set")
        .join("cc-speedy")
        .join("data.db")
}

pub fn open_db() -> Result<Connection> {
    let path = db_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(&path)?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         CREATE TABLE IF NOT EXISTS summaries (
             session_id   TEXT PRIMARY KEY,
             source       TEXT NOT NULL,
             content      TEXT NOT NULL,
             generated_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
         );
         CREATE TABLE IF NOT EXISTS pinned (
             session_id TEXT PRIMARY KEY,
             pinned_at  INTEGER NOT NULL DEFAULT (strftime('%s','now'))
         );
         CREATE TABLE IF NOT EXISTS learnings (
             id          INTEGER PRIMARY KEY AUTOINCREMENT,
             session_id  TEXT    NOT NULL,
             category    TEXT    NOT NULL,
             point       TEXT    NOT NULL,
             captured_at INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS learnings_session ON learnings (session_id);
         CREATE TABLE IF NOT EXISTS settings (
             key   TEXT PRIMARY KEY,
             value TEXT NOT NULL
         );",
    )?;
    Ok(conn)
}

pub fn load_all_summaries(conn: &Connection) -> Result<HashMap<String, String>> {
    let mut stmt = conn.prepare("SELECT session_id, content FROM summaries")?;
    let map = stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(map)
}

pub fn load_all_generated_at(conn: &Connection) -> Result<HashMap<String, i64>> {
    let mut stmt = conn.prepare("SELECT session_id, generated_at FROM summaries")?;
    let map = stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(map)
}

/// Insert or replace a summary. Returns the `generated_at` unix timestamp used.
pub fn save_summary(conn: &Connection, session_id: &str, source: &str, content: &str) -> Result<i64> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    conn.execute(
        "INSERT INTO summaries (session_id, source, content, generated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(session_id) DO UPDATE SET
             content      = excluded.content,
             generated_at = excluded.generated_at",
        params![session_id, source, content, now],
    )?;
    Ok(now)
}

pub fn load_pinned(conn: &Connection) -> Result<HashSet<String>> {
    let mut stmt = conn.prepare("SELECT session_id FROM pinned")?;
    let set = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(set)
}

pub fn set_pinned(conn: &Connection, session_id: &str, pin: bool) -> Result<()> {
    if pin {
        conn.execute(
            "INSERT OR IGNORE INTO pinned (session_id) VALUES (?1)",
            params![session_id],
        )?;
    } else {
        conn.execute("DELETE FROM pinned WHERE session_id = ?1", params![session_id])?;
    }
    Ok(())
}

/// On first run (empty summaries table) import existing `.md` files and `pinned.json`.
pub fn migrate_from_files(conn: &Connection) -> Result<()> {
    let count: i64 =
        conn.query_row("SELECT COUNT(*) FROM summaries", [], |r| r.get(0))?;
    if count > 0 {
        return Ok(()); // already populated
    }

    let cc_dir = dirs::home_dir()
        .expect("HOME must be set")
        .join(".claude")
        .join("summaries");

    // CC summaries: ~/.claude/summaries/{id}.md
    if let Ok(entries) = std::fs::read_dir(&cc_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().map(|e| e == "md").unwrap_or(false) {
                let stem = match path.file_stem().and_then(|s| s.to_str()) {
                    Some(s) => s.to_string(),
                    None => continue,
                };
                if let Ok(content) = std::fs::read_to_string(&path) {
                    let ts = mtime_as_secs(&path);
                    conn.execute(
                        "INSERT OR IGNORE INTO summaries (session_id, source, content, generated_at)
                         VALUES (?1, 'cc', ?2, ?3)",
                        params![stem, content, ts],
                    )?;
                }
            }
        }
    }

    // OC summaries: ~/.local/share/opencode/summaries/{id}.md
    if let Some(oc_dir) =
        dirs::data_local_dir().map(|d| d.join("opencode").join("summaries"))
    {
        if let Ok(entries) = std::fs::read_dir(&oc_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().map(|e| e == "md").unwrap_or(false) {
                    let stem = match path.file_stem().and_then(|s| s.to_str()) {
                        Some(s) => s.to_string(),
                        None => continue,
                    };
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        let ts = mtime_as_secs(&path);
                        conn.execute(
                            "INSERT OR IGNORE INTO summaries (session_id, source, content, generated_at)
                             VALUES (?1, 'oc', ?2, ?3)",
                            params![stem, content, ts],
                        )?;
                    }
                }
            }
        }
    }

    // Pinned IDs from ~/.claude/summaries/pinned.json
    let pinned_json = cc_dir.join("pinned.json");
    if let Ok(data) = std::fs::read_to_string(&pinned_json) {
        if let Ok(ids) = serde_json::from_str::<Vec<String>>(&data) {
            for id in ids {
                let _ = set_pinned(conn, &id, true);
            }
        }
    }

    Ok(())
}

fn mtime_as_secs(path: &std::path::Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64
        })
}

/// Append new learning points for a session. Existing rows are never deleted.
pub fn save_learnings(conn: &Connection, session_id: &str, points: &[LearningPoint]) -> Result<()> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    for p in points {
        conn.execute(
            "INSERT INTO learnings (session_id, category, point, captured_at) VALUES (?1, ?2, ?3, ?4)",
            params![session_id, p.category, p.point, now],
        )?;
    }
    Ok(())
}

/// Load all accumulated learning points for a session, ordered by capture time.
pub fn load_learnings(conn: &Connection, session_id: &str) -> Result<Vec<LearningPoint>> {
    let mut stmt = conn.prepare(
        "SELECT category, point FROM learnings WHERE session_id = ?1 ORDER BY captured_at, id",
    )?;
    let points = stmt
        .query_map(params![session_id], |row| {
            Ok(LearningPoint {
                category: row.get(0)?,
                point:    row.get(1)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(points)
}

pub fn get_setting(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![key],
        |r| r.get::<_, String>(0),
    ).ok()
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}
