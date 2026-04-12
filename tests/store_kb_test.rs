use cc_speedy::store::{LearningPoint, save_learnings, load_learnings, get_setting, set_setting};
use rusqlite::Connection;

fn make_in_memory_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         CREATE TABLE IF NOT EXISTS learnings (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             session_id TEXT NOT NULL,
             category TEXT NOT NULL,
             point TEXT NOT NULL,
             captured_at INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS learnings_session ON learnings (session_id);
         CREATE TABLE IF NOT EXISTS settings (
             key TEXT PRIMARY KEY,
             value TEXT NOT NULL
         );",
    ).unwrap();
    conn
}

#[test]
fn test_save_and_load_learnings() {
    let conn = make_in_memory_db();
    let points = vec![
        LearningPoint { category: "decision_points".to_string(), point: "chose SQLite".to_string() },
        LearningPoint { category: "lessons_gotchas".to_string(), point: "watch out for lock".to_string() },
    ];
    save_learnings(&conn, "sess-001", &points).unwrap();
    let loaded = load_learnings(&conn, "sess-001").unwrap();
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0].category, "decision_points");
    assert_eq!(loaded[0].point, "chose SQLite");
}

#[test]
fn test_load_learnings_empty_returns_empty_vec() {
    let conn = make_in_memory_db();
    let result = load_learnings(&conn, "no-such-session").unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_save_learnings_appends_not_replaces() {
    let conn = make_in_memory_db();
    let first = vec![LearningPoint { category: "lessons_gotchas".to_string(), point: "first lesson".to_string() }];
    let second = vec![LearningPoint { category: "lessons_gotchas".to_string(), point: "second lesson".to_string() }];
    save_learnings(&conn, "sess-002", &first).unwrap();
    save_learnings(&conn, "sess-002", &second).unwrap();
    let loaded = load_learnings(&conn, "sess-002").unwrap();
    assert_eq!(loaded.len(), 2);
}

#[test]
fn test_get_and_set_setting() {
    let conn = make_in_memory_db();
    assert!(get_setting(&conn, "obsidian_kb_path").is_none());
    set_setting(&conn, "obsidian_kb_path", "/tmp/vault").unwrap();
    assert_eq!(get_setting(&conn, "obsidian_kb_path").as_deref(), Some("/tmp/vault"));
}

#[test]
fn test_set_setting_overwrites() {
    let conn = make_in_memory_db();
    set_setting(&conn, "obsidian_kb_path", "/tmp/old").unwrap();
    set_setting(&conn, "obsidian_kb_path", "/tmp/new").unwrap();
    assert_eq!(get_setting(&conn, "obsidian_kb_path").as_deref(), Some("/tmp/new"));
}

#[test]
fn test_save_learnings_empty_slice_is_noop() {
    let conn = make_in_memory_db();
    save_learnings(&conn, "sess-empty", &[]).unwrap();
    let loaded = load_learnings(&conn, "sess-empty").unwrap();
    assert!(loaded.is_empty());
}
