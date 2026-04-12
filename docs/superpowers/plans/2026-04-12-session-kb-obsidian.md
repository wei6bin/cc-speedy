# Session KB & Obsidian Export Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend Ctrl+R summary generation to also extract structured knowledge (decision points, lessons, tools), accumulate learning points across re-generations without overwriting, and auto-export a Markdown note to an Obsidian vault; add a settings panel (`s` key) to configure the vault path.

**Architecture:** Single `claude --print` call returns factual sections + learning sections separated by `<!-- LEARNINGS -->`. Factual part overwrites the `summaries` table; learning bullets are appended as rows to a new `learnings` table, passing existing bullets back to Claude on re-generation so only new ones are returned. A new `obsidian.rs` module writes the combined note to the vault path stored in a new `settings` table.

**Tech Stack:** Rust, rusqlite (already in use), ratatui + crossterm (already in use), chrono (already in use), tokio (already in use).

---

## File Map

| File | Change |
|------|--------|
| `src/store.rs` | Add `LearningPoint` struct; add `learnings` + `settings` tables; add `save_learnings`, `load_learnings`, `get_setting`, `set_setting` |
| `src/summary.rs` | Update `generate_summary` signature + prompt; add `parse_learning_output` helper; add `build_combined_display` helper |
| `src/obsidian.rs` | **New.** `export_to_obsidian` function |
| `src/settings.rs` | **New.** `AppSettings` struct + `load` + `save_obsidian_path` |
| `src/tui.rs` | Add `AppMode::Settings`; extend `AppState`; update `spawn_summary_generation`; add settings panel draw + key handlers; update help bar |
| `src/lib.rs` | Expose `obsidian` and `settings` modules |
| `tests/store_kb_test.rs` | **New.** Tests for learnings + settings DB functions |
| `tests/obsidian_test.rs` | **New.** Tests for Obsidian file export |
| `tests/summary_kb_test.rs` | **New.** Tests for enriched prompt parsing |

---

## Task 1: LearningPoint struct + DB schema in store.rs

**Files:**
- Modify: `src/store.rs`
- Test: `tests/store_kb_test.rs`

- [ ] **Step 1: Write failing tests**

Create `tests/store_kb_test.rs`:

```rust
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
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cargo test store_kb
```

Expected: compile error — `LearningPoint`, `save_learnings`, etc. not found.

- [ ] **Step 3: Add LearningPoint struct and new tables to store.rs**

In `src/store.rs`, add after the `use` block:

```rust
#[derive(Debug, Clone)]
pub struct LearningPoint {
    pub category: String,  // "decision_points" | "lessons_gotchas" | "tools_commands"
    pub point:    String,
}
```

In `open_db()`, extend the `execute_batch` SQL string (append to the existing string):

```rust
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
```

- [ ] **Step 4: Add save_learnings, load_learnings, get_setting, set_setting to store.rs**

Append to end of `src/store.rs`:

```rust
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
```

- [ ] **Step 5: Run tests to confirm they pass**

```bash
cargo test store_kb
```

Expected: all 5 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/store.rs tests/store_kb_test.rs
git commit -m "feat: add learnings and settings tables + store CRUD functions"
```

---

## Task 2: Enriched generate_summary — prompt, parsing, combined display

**Files:**
- Modify: `src/summary.rs`
- Test: `tests/summary_kb_test.rs`

- [ ] **Step 1: Write failing tests**

Create `tests/summary_kb_test.rs`:

```rust
use cc_speedy::summary::{parse_learning_output, build_combined_display};
use cc_speedy::store::LearningPoint;

#[test]
fn test_parse_learning_output_extracts_bullets() {
    let md = "\
## Decision points
- chose SQLite over flat files: simpler migration
- used async spawn: keeps TUI responsive

## Lessons & gotchas
- lock poisoning crashes the thread if not handled

## Tools & commands discovered
- cargo test sessions: runs a single test file
";
    let points = parse_learning_output(md);
    assert_eq!(points.len(), 4);
    assert_eq!(points[0].category, "decision_points");
    assert_eq!(points[0].point, "chose SQLite over flat files: simpler migration");
    assert_eq!(points[2].category, "lessons_gotchas");
    assert_eq!(points[3].category, "tools_commands");
}

#[test]
fn test_parse_learning_output_skips_none() {
    let md = "\
## Decision points
- none

## Lessons & gotchas
- none

## Tools & commands discovered
- none
";
    let points = parse_learning_output(md);
    assert!(points.is_empty(), "should skip 'none' bullets");
}

#[test]
fn test_parse_learning_output_unknown_heading_ignored() {
    let md = "\
## Foobar section
- something

## Decision points
- real point
";
    let points = parse_learning_output(md);
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].category, "decision_points");
}

#[test]
fn test_build_combined_display_empty_learnings() {
    let combined = build_combined_display("## What was done\n- fixed bug", &[]);
    assert_eq!(combined, "## What was done\n- fixed bug");
}

#[test]
fn test_build_combined_display_includes_learnings() {
    let learnings = vec![
        LearningPoint { category: "decision_points".to_string(), point: "used tokio::spawn".to_string() },
        LearningPoint { category: "lessons_gotchas".to_string(), point: "mutex guard must be dropped".to_string() },
    ];
    let combined = build_combined_display("## What was done\n- fixed bug", &learnings);
    assert!(combined.contains("Knowledge Capture"), "should have knowledge section header");
    assert!(combined.contains("used tokio::spawn"));
    assert!(combined.contains("mutex guard must be dropped"));
    assert!(combined.contains("## Decision points"));
    assert!(combined.contains("## Lessons & gotchas"));
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cargo test summary_kb
```

Expected: compile error — `parse_learning_output` and `build_combined_display` not found.

- [ ] **Step 3: Add parse_learning_output and build_combined_display to summary.rs**

Append to `src/summary.rs`:

```rust
/// Parse the learning section of the enriched prompt output into structured points.
/// Recognises headings "## Decision points", "## Lessons & gotchas", "## Tools & commands discovered".
/// Bullets containing only "none" (case-insensitive) are skipped.
pub fn parse_learning_output(learning_md: &str) -> Vec<crate::store::LearningPoint> {
    let mut points = Vec::new();
    let mut current_category: Option<&'static str> = None;

    for line in learning_md.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("## ") {
            let heading = trimmed.trim_start_matches("## ").to_lowercase();
            current_category = match heading.as_str() {
                "decision points" | "decision_points" => Some("decision_points"),
                "lessons & gotchas" | "lessons_&_gotchas" | "lessons and gotchas" => Some("lessons_gotchas"),
                "tools & commands discovered" | "tools_&_commands_discovered" | "tools and commands discovered" => Some("tools_commands"),
                _ => None,
            };
        } else if trimmed.starts_with("- ") {
            if let Some(cat) = current_category {
                let point = trimmed.trim_start_matches("- ").trim().to_string();
                if !point.is_empty() && point.to_lowercase() != "none" {
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
```

- [ ] **Step 4: Run tests to confirm they pass**

```bash
cargo test summary_kb
```

Expected: all 5 tests pass.

- [ ] **Step 5: Update generate_summary signature and prompt**

Replace the existing `pub async fn generate_summary` in `src/summary.rs` with:

```rust
pub async fn generate_summary(
    messages: &[Message],
    existing_learnings: &[crate::store::LearningPoint],
) -> Result<(String, Vec<crate::store::LearningPoint>)> {
    // Take last 50 messages
    let snippet: String = messages.iter().rev().take(50).rev()
        .map(|m| format!("{}: {}", m.role, m.text.chars().take(200).collect::<String>()))
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
        SECTION 1 — output these headings and bullets only:\n\
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
        SECTION 2 — extract ONLY points not already in EXISTING LEARNINGS:\n\
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

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        tokio::process::Command::new("claude")
            .args(["--print", &prompt])
            .output(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("claude --print timed out after 60 seconds"))?
    .map_err(|e| anyhow::anyhow!("failed to run `claude`: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("claude --print failed: {}", stderr);
    }

    let text = String::from_utf8(output.stdout)
        .map_err(|e| anyhow::anyhow!("claude output was not valid UTF-8: {}", e))?;
    let text = text.trim();

    // Split on the delimiter
    let (factual, learning_md) = match text.split_once("<!-- LEARNINGS -->") {
        Some((f, l)) => (f.trim().to_string(), l.trim().to_string()),
        None => (text.to_string(), String::new()),  // graceful degradation
    };

    let new_points = parse_learning_output(&learning_md);
    Ok((factual, new_points))
}
```

- [ ] **Step 6: Fix the run_hook() call site**

In `src/summary.rs`, `run_hook()` calls `generate_summary`. Update that call:

```rust
    let (summary_text, _new_points) = generate_summary(&messages, &[]).await?;
    crate::store::save_summary(&conn, &session_id, "cc", &summary_text)?;
```

- [ ] **Step 7: Verify everything compiles**

```bash
cargo build 2>&1 | head -30
```

Expected: no errors (warnings about unused `_new_points` are fine).

- [ ] **Step 8: Run all tests**

```bash
cargo test
```

Expected: all existing tests + new summary_kb tests pass.

- [ ] **Step 9: Commit**

```bash
git add src/summary.rs tests/summary_kb_test.rs
git commit -m "feat: enrich generate_summary with knowledge extraction sections"
```

---

## Task 3: obsidian.rs — write Obsidian vault files

**Files:**
- Create: `src/obsidian.rs`
- Test: `tests/obsidian_test.rs`

- [ ] **Step 1: Write failing tests**

Create `tests/obsidian_test.rs`:

```rust
use cc_speedy::obsidian::export_to_obsidian;
use cc_speedy::store::LearningPoint;
use cc_speedy::unified::{UnifiedSession, SessionSource};
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use tempfile::TempDir;

fn make_session(msg_count: usize) -> UnifiedSession {
    UnifiedSession {
        session_id:     "abc12345-test".to_string(),
        project_name:   "cc-speedy".to_string(),
        project_path:   "/home/user/ai/cc-speedy".to_string(),
        modified:       UNIX_EPOCH + Duration::from_secs(1_700_000_000),
        message_count:  msg_count,
        first_user_msg: "hello".to_string(),
        summary:        "Fix the bug".to_string(),
        git_branch:     "main".to_string(),
        source:         SessionSource::ClaudeCode,
        jsonl_path:     None,
    }
}

#[test]
fn test_export_writes_markdown_file() {
    let tmp = TempDir::new().unwrap();
    let session = make_session(10);
    let learnings = vec![
        LearningPoint { category: "decision_points".to_string(), point: "used tokio::spawn".to_string() },
        LearningPoint { category: "lessons_gotchas".to_string(), point: "watch lock order".to_string() },
    ];
    export_to_obsidian(&session, "## What was done\n- fixed bug", &learnings, tmp.path().to_str().unwrap()).unwrap();

    // File should exist
    let files: Vec<_> = std::fs::read_dir(tmp.path()).unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(files.len(), 1);
    let content = std::fs::read_to_string(files[0].path()).unwrap();
    assert!(content.contains("session_id: abc12345-test"));
    assert!(content.contains("project: /home/user/ai/cc-speedy"));
    assert!(content.contains("tags: [agent-session]"));
    assert!(content.contains("## What was done"));
    assert!(content.contains("## Decision points"));
    assert!(content.contains("used tokio::spawn"));
    assert!(content.contains("## Lessons & gotchas"));
    assert!(content.contains("watch lock order"));
}

#[test]
fn test_export_skips_sessions_with_few_messages() {
    let tmp = TempDir::new().unwrap();
    let session = make_session(3);  // below threshold of 5
    export_to_obsidian(&session, "summary", &[], tmp.path().to_str().unwrap()).unwrap();
    let files: Vec<_> = std::fs::read_dir(tmp.path()).unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert!(files.is_empty(), "should not write file for session with < 5 messages");
}

#[test]
fn test_export_filename_format() {
    let tmp = TempDir::new().unwrap();
    let session = make_session(10);
    export_to_obsidian(&session, "summary", &[], tmp.path().to_str().unwrap()).unwrap();
    let files: Vec<_> = std::fs::read_dir(tmp.path()).unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(files.len(), 1);
    let name = files[0].file_name();
    let name_str = name.to_string_lossy();
    // Format: YYYY-MM-DD-<project>-<id8>.md
    assert!(name_str.ends_with(".md"));
    assert!(name_str.contains("abc1234"), "should contain first 8 chars of session_id: {}", name_str);
    assert!(name_str.contains("ai-cc-speedy"), "should contain project slug: {}", name_str);
}

#[test]
fn test_export_overwrites_existing_file() {
    let tmp = TempDir::new().unwrap();
    let session = make_session(10);
    export_to_obsidian(&session, "old content", &[], tmp.path().to_str().unwrap()).unwrap();
    export_to_obsidian(&session, "new content", &[], tmp.path().to_str().unwrap()).unwrap();
    let files: Vec<_> = std::fs::read_dir(tmp.path()).unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(files.len(), 1, "should not create two files on re-export");
    let content = std::fs::read_to_string(files[0].path()).unwrap();
    assert!(content.contains("new content"));
    assert!(!content.contains("old content"));
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cargo test obsidian
```

Expected: compile error — module `obsidian` not found.

- [ ] **Step 3: Create src/obsidian.rs**

```rust
use anyhow::Result;
use crate::unified::UnifiedSession;
use crate::store::LearningPoint;

/// Write a combined Obsidian Markdown note for a session.
/// Skips sessions with fewer than 5 messages to keep the vault clean.
/// Overwrites the file if it already exists (learning rows in the DB always accumulate).
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

    // First 8 alphanumeric chars of session_id
    let id_prefix: String = session.session_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(8)
        .collect();

    let filename = format!("{}-{}-{}.md", date_str, project_slug, id_prefix);
    let file_path = std::path::Path::new(vault_path).join(&filename);

    let front_matter = format!(
        "---\ndate: {}\nproject: {}\nsession_id: {}\ntags: [agent-session]\n---\n\n",
        date_str,
        session.project_path,
        session.session_id,
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
```

- [ ] **Step 4: Add obsidian module to lib.rs**

In `src/lib.rs`, add:

```rust
pub mod obsidian;
```

- [ ] **Step 5: Run tests to confirm they pass**

```bash
cargo test obsidian
```

Expected: all 4 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/obsidian.rs src/lib.rs tests/obsidian_test.rs
git commit -m "feat: add obsidian.rs — export session KB notes to Obsidian vault"
```

---

## Task 4: settings.rs module

**Files:**
- Create: `src/settings.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Create src/settings.rs**

```rust
use anyhow::Result;
use rusqlite::Connection;

#[derive(Debug, Clone, Default)]
pub struct AppSettings {
    pub obsidian_kb_path: Option<String>,
}

/// Load all settings from DB into AppSettings.
pub fn load(conn: &Connection) -> AppSettings {
    AppSettings {
        obsidian_kb_path: crate::store::get_setting(conn, "obsidian_kb_path"),
    }
}

/// Validate that path exists and is a directory, then persist to DB.
pub fn save_obsidian_path(conn: &Connection, path: &str) -> Result<()> {
    let meta = std::fs::metadata(path)
        .map_err(|_| anyhow::anyhow!("Path does not exist: {}", path))?;
    if !meta.is_dir() {
        anyhow::bail!("Path is not a directory: {}", path);
    }
    crate::store::set_setting(conn, "obsidian_kb_path", path)?;
    Ok(())
}
```

- [ ] **Step 2: Add settings module to lib.rs**

In `src/lib.rs`, add:

```rust
pub mod settings;
```

- [ ] **Step 3: Verify compilation**

```bash
cargo build 2>&1 | head -20
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/settings.rs src/lib.rs
git commit -m "feat: add settings.rs — AppSettings struct with obsidian path persistence"
```

---

## Task 5: TUI — wire spawn_summary_generation to new generate_summary

**Files:**
- Modify: `src/tui.rs`

- [ ] **Step 1: Add settings and new fields to AppState**

In `src/tui.rs`, update the `AppState` struct to add settings fields:

```rust
struct AppState {
    sessions: Vec<UnifiedSession>,
    filtered: Vec<usize>,
    list_state: ListState,
    filter: String,
    mode: AppMode,
    rename_input: String,
    summaries: Arc<Mutex<std::collections::HashMap<String, String>>>,
    summary_generated_at: Arc<Mutex<std::collections::HashMap<String, i64>>>,
    generating: Arc<Mutex<std::collections::HashSet<String>>>,
    focus: Focus,
    preview_scroll: u16,
    status_msg: Option<(String, Instant)>,
    source_filter: Option<SessionSource>,
    pinned: std::collections::HashSet<String>,
    db: Arc<Mutex<rusqlite::Connection>>,
    settings: crate::settings::AppSettings,
    // Settings panel state
    settings_editing: bool,
    settings_input: String,
    settings_error: Option<String>,
}
```

- [ ] **Step 2: Load settings in AppState::new**

In `AppState::new`, after the `let pinned = ...` line, add:

```rust
        let settings = crate::settings::load(&conn);
```

And in the `Ok(Self { ... })` struct literal, add:

```rust
            settings,
            settings_editing: false,
            settings_input: String::new(),
            settings_error: None,
```

- [ ] **Step 3: Add Settings to AppMode enum**

Change:

```rust
#[derive(PartialEq)]
enum AppMode { Normal, Filter, Rename, PinMenu }
```

To:

```rust
#[derive(PartialEq)]
enum AppMode { Normal, Filter, Rename, PinMenu, Settings }
```

- [ ] **Step 4: Update spawn_summary_generation signature and body**

Replace the existing `fn spawn_summary_generation(...)` with:

```rust
fn spawn_summary_generation(
    id: String,
    jsonl: Option<String>,
    source: SessionSource,
    session: UnifiedSession,                                              // for Obsidian export
    existing_learnings: Vec<crate::store::LearningPoint>,                 // accumulated so far
    obsidian_path: Option<String>,                                        // None = not configured
    summaries: Arc<Mutex<std::collections::HashMap<String, String>>>,
    summary_generated_at: Arc<Mutex<std::collections::HashMap<String, i64>>>,
    generating: Arc<Mutex<std::collections::HashSet<String>>>,
    db: Arc<Mutex<rusqlite::Connection>>,
) {
    generating.lock().unwrap_or_else(|e| e.into_inner()).insert(id.clone());
    tokio::spawn(async move {
        let msgs = match source {
            SessionSource::ClaudeCode => {
                let Some(jsonl_path) = jsonl else {
                    generating.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
                    return;
                };
                tokio::task::spawn_blocking({
                    let p = jsonl_path.clone();
                    move || crate::sessions::parse_messages(std::path::Path::new(&p))
                }).await.ok().and_then(|r| r.ok())
            }
            SessionSource::OpenCode => {
                let session_id = id.clone();
                tokio::task::spawn_blocking(move || {
                    crate::opencode_sessions::parse_opencode_messages(&session_id)
                }).await.ok().and_then(|r| r.ok())
            }
            SessionSource::Copilot => {
                let session_id = id.clone();
                tokio::task::spawn_blocking(move || {
                    crate::copilot_sessions::parse_copilot_messages(&session_id)
                }).await.ok().and_then(|r| r.ok())
            }
        };

        if let Some(msgs) = msgs {
            let src_str = match source {
                SessionSource::ClaudeCode => "cc",
                SessionSource::OpenCode   => "oc",
                SessionSource::Copilot    => "co",
            };
            match crate::summary::generate_summary(&msgs, &existing_learnings).await {
                Ok((factual, new_points)) => {
                    // 1. Persist factual summary (overwrites)
                    let ts = crate::store::save_summary(
                        &db.lock().unwrap_or_else(|e| e.into_inner()),
                        &id, src_str, &factual,
                    ).unwrap_or_else(|_| {
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64
                    });

                    // 2. Append new learning points (never overwrites old ones)
                    if !new_points.is_empty() {
                        let _ = crate::store::save_learnings(
                            &db.lock().unwrap_or_else(|e| e.into_inner()),
                            &id, &new_points,
                        );
                    }

                    // 3. Load ALL learnings (existing + new) for combined display
                    let all_learnings = crate::store::load_learnings(
                        &db.lock().unwrap_or_else(|e| e.into_inner()),
                        &id,
                    ).unwrap_or_default();

                    // 4. Build combined display string
                    let combined = crate::summary::build_combined_display(&factual, &all_learnings);

                    // 5. Update in-memory cache with combined display
                    summaries.lock().unwrap_or_else(|e| e.into_inner()).insert(id.clone(), combined);
                    summary_generated_at.lock().unwrap_or_else(|e| e.into_inner()).insert(id.clone(), ts);

                    // 6. Export to Obsidian (non-fatal)
                    if let Some(ref vault_path) = obsidian_path {
                        let _ = crate::obsidian::export_to_obsidian(
                            &session, &factual, &all_learnings, vault_path,
                        );
                    }
                }
                Err(e) => {
                    summaries
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert(id.clone(), format!("Error generating summary: {}", e));
                }
            }
        }
        generating.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
    });
}
```

- [ ] **Step 5: Update the Ctrl+R key handler to pass the new params**

Find the `// Ctrl+R: regenerate summary` block in `run_event_loop`. Replace it with:

```rust
                    // Ctrl+R: regenerate summary
                    (AppMode::Normal, KeyModifiers::CONTROL, KeyCode::Char('r')) => {
                        if let Some(s) = app.selected_session() {
                            let id      = s.session_id.clone();
                            let jsonl   = s.jsonl_path.clone();
                            let source  = s.source.clone();
                            let session = s.clone();
                            let summaries        = app.summaries.clone();
                            let generated_at     = app.summary_generated_at.clone();
                            let generating       = app.generating.clone();
                            let db               = app.db.clone();
                            let obsidian_path    = app.settings.obsidian_kb_path.clone();

                            // Load existing learnings before clearing cache
                            let existing_learnings = crate::store::load_learnings(
                                &app.db.lock().unwrap_or_else(|e| e.into_inner()),
                                &id,
                            ).unwrap_or_default();

                            // Clear cached factual summary (learning rows in DB are kept)
                            app.summaries.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
                            app.summary_generated_at.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);

                            spawn_summary_generation(
                                id, jsonl, source, session,
                                existing_learnings, obsidian_path,
                                summaries, generated_at, generating, db,
                            );
                        }
                    }
```

- [ ] **Step 6: Also load combined display on startup for sessions that already have learnings**

In `AppState::new`, replace the existing `let summaries_map = crate::store::load_all_summaries(&conn)?;` line with:

```rust
        let mut summaries_map = crate::store::load_all_summaries(&conn)?;
        // For sessions that have accumulated learnings, build the combined display string
        for (sid, factual) in summaries_map.iter_mut() {
            if let Ok(learnings) = crate::store::load_learnings(&conn, sid) {
                if !learnings.is_empty() {
                    *factual = crate::summary::build_combined_display(factual, &learnings);
                }
            }
        }
```

- [ ] **Step 7: Verify compilation**

```bash
cargo build 2>&1 | head -30
```

Expected: no errors.

- [ ] **Step 8: Run all tests**

```bash
cargo test
```

Expected: all tests pass.

- [ ] **Step 9: Commit**

```bash
git add src/tui.rs
git commit -m "feat: wire enriched generate_summary into TUI — accumulate learnings, export to Obsidian"
```

---

## Task 6: TUI — Settings panel (s key)

**Files:**
- Modify: `src/tui.rs`
- Modify: `src/theme.rs`

- [ ] **Step 1: Add a settings border color to theme.rs**

In `src/theme.rs`, add after the `BORDER_TOP` line:

```rust
pub const BORDER_SETTINGS: Color = Color::Rgb(128, 0, 128);  // #800080  magenta — settings popup
```

- [ ] **Step 2: Add draw_settings_popup function to tui.rs**

Add this function after `draw_pin_popup`:

```rust
fn draw_settings_popup(f: &mut ratatui::Frame, app: &AppState, area: Rect) {
    let popup_area = centered_rect(70, 10, area);
    f.render_widget(Clear, popup_area);

    let obsidian_display = if app.settings_editing {
        format!("▶ {}|", app.settings_input)
    } else {
        let val = app.settings.obsidian_kb_path.as_deref().unwrap_or("(not set)");
        format!("  {}", val)
    };

    let error_line = if let Some(ref err) = app.settings_error {
        format!("\n  ✗ {}", err)
    } else {
        String::new()
    };

    let hint = if app.settings_editing {
        "[Enter] Save   [Esc] Cancel"
    } else {
        "[Enter] Edit   [Esc] Close"
    };

    let content = format!(
        "\n  Obsidian KB path\n  {}{}\n\n  {}",
        obsidian_display, error_line, hint
    );

    let popup = Paragraph::new(content)
        .block(
            Block::default()
                .border_type(theme::BORDER_TYPE)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::BORDER_SETTINGS))
                .title(Span::styled(" Settings ", theme::title_style())),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(popup, popup_area);
}
```

- [ ] **Step 3: Call draw_settings_popup in draw()**

In the `draw` function, after the `if app.mode == AppMode::PinMenu` block, add:

```rust
    if app.mode == AppMode::Settings {
        draw_settings_popup(f, app, area);
    }
```

- [ ] **Step 4: Handle Settings mode in the top bar title**

In `draw`, update the `(bar_text, bar_title)` match to handle `AppMode::Settings`:

```rust
    let (bar_text, bar_title) = match &app.mode {
        AppMode::Filter   => (format!("> {}|", app.filter), " Filter "),
        AppMode::Rename   => (format!("rename: {}|", app.rename_input), " Rename  [Enter: confirm  Esc: cancel] "),
        AppMode::PinMenu  => ("".to_string(), " cc-speedy "),
        AppMode::Settings => ("".to_string(), " cc-speedy — Settings "),
        AppMode::Normal   => {
            let hint = if app.filter.is_empty() {
                "  (press / to filter)".to_string()
            } else {
                format!("  filter: {}", app.filter)
            };
            (hint, " cc-speedy ")
        }
    };
```

- [ ] **Step 5: Add key handlers for Settings mode**

In `run_event_loop`, add these match arms **before** the `_ => {}` catch-all:

```rust
                    // --- Settings mode ---
                    (AppMode::Normal, KeyModifiers::NONE, KeyCode::Char('s')) => {
                        // Pre-fill input with current value
                        app.settings_input = app.settings.obsidian_kb_path
                            .clone()
                            .unwrap_or_default();
                        app.settings_error = None;
                        app.settings_editing = false;
                        app.mode = AppMode::Settings;
                    }
                    (AppMode::Settings, _, KeyCode::Esc) => {
                        if app.settings_editing {
                            app.settings_editing = false;
                            app.settings_error = None;
                        } else {
                            app.mode = AppMode::Normal;
                        }
                    }
                    (AppMode::Settings, _, KeyCode::Enter) => {
                        if !app.settings_editing {
                            // Enter edit mode
                            app.settings_editing = true;
                            app.settings_error = None;
                        } else {
                            // Validate and save
                            let path = app.settings_input.trim().to_string();
                            let result = crate::settings::save_obsidian_path(
                                &app.db.lock().unwrap_or_else(|e| e.into_inner()),
                                &path,
                            );
                            match result {
                                Ok(()) => {
                                    app.settings.obsidian_kb_path = Some(path);
                                    app.settings_editing = false;
                                    app.settings_error = None;
                                    app.status_msg = Some(("Obsidian path saved".to_string(), Instant::now()));
                                    app.mode = AppMode::Normal;
                                }
                                Err(e) => {
                                    app.settings_error = Some(e.to_string());
                                }
                            }
                        }
                    }
                    (AppMode::Settings, _, KeyCode::Backspace) if app.settings_editing => {
                        app.settings_input.pop();
                    }
                    (AppMode::Settings, _, KeyCode::Char(c)) if app.settings_editing => {
                        app.settings_input.push(c);
                    }
```

- [ ] **Step 6: Update help bar to include 's'**

Find the two occurrences of the help bar string and add `s: settings` to each:

Old string (appears twice):
```
" 1:CC  2:OC  3:CO  0:all  /: filter  Enter: resume  n: new  Ctrl+Y/N: yolo  Tab  j/k  r  c  x: pin  Ctrl+R  q"
```

New string:
```
" 1:CC  2:OC  3:CO  0:all  /: filter  Enter: resume  n: new  Ctrl+Y/N: yolo  Tab  j/k  r  c  x: pin  s: settings  Ctrl+R  q"
```

- [ ] **Step 7: Verify compilation**

```bash
cargo build 2>&1 | head -30
```

Expected: no errors.

- [ ] **Step 8: Run all tests**

```bash
cargo test
```

Expected: all tests pass.

- [ ] **Step 9: Commit**

```bash
git add src/tui.rs src/theme.rs
git commit -m "feat: add Settings panel (s key) with Obsidian path configuration and validation"
```

---

## Task 7: Final smoke test

- [ ] **Step 1: Run full test suite**

```bash
cargo test 2>&1
```

Expected: all tests pass, zero failures.

- [ ] **Step 2: Run clippy**

```bash
cargo clippy 2>&1 | grep "^error" | head -20
```

Expected: no errors (warnings ok).

- [ ] **Step 3: Build release binary**

```bash
cargo build --release 2>&1 | tail -5
```

Expected: `Finished release` with no errors.

- [ ] **Step 4: Commit if any clippy fixes were needed**

```bash
git add -p
git commit -m "fix: clippy warnings from KB feature"
```
