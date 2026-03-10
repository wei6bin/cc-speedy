# OpenCode Integration Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend cc-speedy to read, display, and resume OpenCode sessions alongside Claude Code sessions in a single unified TUI.

**Architecture:** Add `opencode_sessions.rs` (SQLite reader via `rusqlite`), `unified.rs` (shared `UnifiedSession` type), extend `summary.rs` with an OpenCode path, extend `tmux.rs` with `oc-` prefixed resume, and update `tui.rs` for source badges and source filtering. All existing Claude Code behaviour is preserved unchanged.

**Tech Stack:** Rust 2021, rusqlite 0.31 (bundled), ratatui 0.29, crossterm 0.28, tokio 1, serde_json 1, dirs 6

**Spec:** `docs/plans/2026-03-10-opencode-integration-design.md`

---

## Chunk 1: Dependencies, Unified Type, and Summary Path

### Task 1: Add rusqlite dependency

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add rusqlite to Cargo.toml**

  In the `[dependencies]` section, add:

  ```toml
  rusqlite = { version = "0.31", features = ["bundled"] }
  ```

- [ ] **Step 2: Verify it compiles**

  Run: `~/.cargo/bin/cargo build 2>&1 | grep "^error"`
  Expected: no output (no errors)

- [ ] **Step 3: Commit**

  ```bash
  git add Cargo.toml Cargo.lock
  git commit -m "feat: add rusqlite bundled dependency for OpenCode SQLite support"
  ```

---

### Task 2: Add `unified.rs` — shared session type

**Files:**
- Create: `src/unified.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Write failing test**

  Create `tests/unified_test.rs`:

  ```rust
  use cc_speedy::sessions::Session;
  use cc_speedy::unified::{UnifiedSession, SessionSource};
  use std::time::SystemTime;

  fn make_cc_session() -> Session {
      Session {
          session_id: "abc-123".to_string(),
          project_name: "ai/myproj".to_string(),
          project_path: "/home/user/ai/myproj".to_string(),
          modified: SystemTime::UNIX_EPOCH,
          message_count: 10,
          first_user_msg: "fix the bug".to_string(),
          jsonl_path: "/home/user/.claude/projects/x/abc-123.jsonl".to_string(),
          summary: "Fixed auth bug".to_string(),
          git_branch: "main".to_string(),
      }
  }

  #[test]
  fn test_from_cc_session_sets_source() {
      let s: UnifiedSession = make_cc_session().into();
      assert!(matches!(s.source, SessionSource::ClaudeCode));
  }

  #[test]
  fn test_from_cc_session_preserves_fields() {
      let s: UnifiedSession = make_cc_session().into();
      assert_eq!(s.session_id, "abc-123");
      assert_eq!(s.project_path, "/home/user/ai/myproj");
      assert_eq!(s.message_count, 10);
      assert_eq!(s.summary, "Fixed auth bug");
      assert_eq!(s.jsonl_path, Some("/home/user/.claude/projects/x/abc-123.jsonl".to_string()));
  }

  #[test]
  fn test_opencode_session_has_no_jsonl_path() {
      let s = UnifiedSession {
          session_id: "ses_abc".to_string(),
          project_name: "ai/myproj".to_string(),
          project_path: "/home/user/ai/myproj".to_string(),
          modified: SystemTime::UNIX_EPOCH,
          message_count: 5,
          first_user_msg: "build feature".to_string(),
          summary: "".to_string(),
          git_branch: "".to_string(),
          source: SessionSource::OpenCode,
          jsonl_path: None,
      };
      assert!(s.jsonl_path.is_none());
  }
  ```

- [ ] **Step 2: Run test to verify it fails**

  Run: `~/.cargo/bin/cargo test --test unified_test 2>&1 | tail -5`
  Expected: compile error — `cc_speedy::unified` not found

- [ ] **Step 3: Implement `src/unified.rs`**

  ```rust
  use std::time::SystemTime;
  use crate::sessions::Session;

  #[derive(Debug, Clone, PartialEq)]
  pub enum SessionSource {
      ClaudeCode,
      OpenCode,
  }

  #[derive(Debug, Clone)]
  pub struct UnifiedSession {
      pub session_id:    String,
      pub project_name:  String,
      pub project_path:  String,
      pub modified:      SystemTime,
      pub message_count: usize,
      pub first_user_msg: String,
      pub summary:       String,
      pub git_branch:    String,
      pub source:        SessionSource,
      /// Some(path) for Claude Code sessions; None for OpenCode sessions.
      pub jsonl_path:    Option<String>,
  }

  impl From<Session> for UnifiedSession {
      fn from(s: Session) -> Self {
          UnifiedSession {
              session_id:    s.session_id,
              project_name:  s.project_name,
              project_path:  s.project_path,
              modified:      s.modified,
              message_count: s.message_count,
              first_user_msg: s.first_user_msg,
              summary:       s.summary,
              git_branch:    s.git_branch,
              source:        SessionSource::ClaudeCode,
              jsonl_path:    Some(s.jsonl_path),
          }
      }
  }
  ```

- [ ] **Step 4: Register module in `src/lib.rs`**

  Add to `src/lib.rs`:
  ```rust
  pub mod unified;
  ```

- [ ] **Step 5: Run tests**

  Run: `~/.cargo/bin/cargo test --test unified_test 2>&1 | tail -10`
  Expected: 3 tests PASS

- [ ] **Step 6: Commit**

  ```bash
  git add src/unified.rs src/lib.rs tests/unified_test.rs
  git commit -m "feat: add UnifiedSession type with ClaudeCode/OpenCode source enum"
  ```

---

### Task 3: Extend `summary.rs` with OpenCode summary path

**Files:**
- Modify: `src/summary.rs`
- Modify: `tests/summary_test.rs`

- [ ] **Step 1: Write failing test**

  Append to `tests/summary_test.rs`:

  ```rust
  #[test]
  fn test_opencode_summary_path_uses_local_share() {
      let path = cc_speedy::summary::opencode_summary_path("ses_abc123");
      let path_str = path.to_string_lossy();
      assert!(path_str.contains("opencode"), "path should contain 'opencode': {}", path_str);
      assert!(path_str.contains("ses_abc123"), "path should contain session id: {}", path_str);
      assert!(path_str.ends_with(".md"));
  }

  #[test]
  fn test_opencode_summary_path_sanitizes_id() {
      // path traversal attempt should be neutralised
      let path = cc_speedy::summary::opencode_summary_path("../../etc/passwd");
      let path_str = path.to_string_lossy();
      assert!(!path_str.contains(".."), "path should not contain '..': {}", path_str);
  }
  ```

- [ ] **Step 2: Run test to verify it fails**

  Run: `~/.cargo/bin/cargo test test_opencode_summary_path 2>&1 | tail -5`
  Expected: compile error — `opencode_summary_path` not found

- [ ] **Step 3: Add `opencode_summary_path` to `src/summary.rs`**

  Append to `src/summary.rs` (after the existing `summary_path` function):

  ```rust
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
  ```

  Note: `dirs::data_local_dir()` returns `~/.local/share` on Linux and
  `~/Library/Application Support` on macOS — matching OpenCode's own storage location.

- [ ] **Step 4: Run tests**

  Run: `~/.cargo/bin/cargo test test_opencode_summary_path 2>&1 | tail -10`
  Expected: 2 tests PASS

- [ ] **Step 5: Commit**

  ```bash
  git add src/summary.rs tests/summary_test.rs
  git commit -m "feat: add opencode_summary_path for OpenCode session summaries"
  ```

---

## Chunk 2: OpenCode SQLite Session Reader

### Task 4: `opencode_sessions.rs` — DB path and connection helper

**Files:**
- Create: `src/opencode_sessions.rs`
- Modify: `src/lib.rs`
- Create: `tests/opencode_sessions_test.rs`

- [ ] **Step 1: Write failing test for DB path**

  Create `tests/opencode_sessions_test.rs`:

  ```rust
  use cc_speedy::opencode_sessions::opencode_db_path;

  #[test]
  fn test_opencode_db_path_ends_with_db_file() {
      if let Some(p) = opencode_db_path() {
          let s = p.to_string_lossy();
          assert!(s.ends_with("opencode.db"), "expected path to end with opencode.db, got: {}", s);
          assert!(s.contains("opencode"), "expected path to contain 'opencode': {}", s);
      }
      // If None: opencode not installed; that's acceptable — test passes
  }
  ```

- [ ] **Step 2: Run test to verify it fails**

  Run: `~/.cargo/bin/cargo test test_opencode_db_path 2>&1 | tail -5`
  Expected: compile error — `cc_speedy::opencode_sessions` not found

- [ ] **Step 3: Implement `src/opencode_sessions.rs` stub with `opencode_db_path`**

  ```rust
  use std::path::PathBuf;

  /// Path to the OpenCode SQLite database.
  /// Returns None if the data_local_dir cannot be determined.
  pub fn opencode_db_path() -> Option<PathBuf> {
      dirs::data_local_dir().map(|d| d.join("opencode").join("opencode.db"))
  }
  ```

- [ ] **Step 4: Register module in `src/lib.rs`**

  Add to `src/lib.rs`:
  ```rust
  pub mod opencode_sessions;
  ```

- [ ] **Step 5: Run tests**

  Run: `~/.cargo/bin/cargo test test_opencode_db_path 2>&1 | tail -10`
  Expected: 1 test PASS

- [ ] **Step 6: Commit**

  ```bash
  git add src/opencode_sessions.rs src/lib.rs tests/opencode_sessions_test.rs
  git commit -m "feat: opencode_sessions module stub with db path helper"
  ```

---

### Task 5: `list_opencode_sessions()` — query sessions from SQLite

**Files:**
- Modify: `src/opencode_sessions.rs`
- Modify: `tests/opencode_sessions_test.rs`

- [ ] **Step 1: Write failing test using an in-memory SQLite fixture**

  Append to `tests/opencode_sessions_test.rs`:

  ```rust
  use rusqlite::Connection;
  use cc_speedy::opencode_sessions::query_sessions_from_conn;

  fn setup_fixture_db() -> Connection {
      let conn = Connection::open_in_memory().unwrap();
      conn.execute_batch("
          CREATE TABLE project (
              id TEXT PRIMARY KEY,
              worktree TEXT NOT NULL,
              time_created INTEGER,
              time_updated INTEGER
          );
          CREATE TABLE session (
              id TEXT PRIMARY KEY,
              project_id TEXT NOT NULL,
              parent_id TEXT,
              title TEXT,
              time_updated INTEGER NOT NULL,
              time_archived INTEGER,
              summary_diffs TEXT
          );
          CREATE TABLE message (
              id TEXT PRIMARY KEY,
              session_id TEXT NOT NULL,
              time_created INTEGER
          );
          CREATE TABLE part (
              id TEXT PRIMARY KEY,
              message_id TEXT NOT NULL,
              session_id TEXT NOT NULL,
              time_created INTEGER,
              data TEXT
          );

          INSERT INTO project VALUES ('proj1', '/home/user/ai/myproj', 1000, 2000);

          -- top-level session (should appear)
          INSERT INTO session VALUES (
              'ses_aaa', 'proj1', NULL, 'my title',
              1741600000000, NULL, NULL
          );
          -- sub-agent session (parent_id set — should be filtered out)
          INSERT INTO session VALUES (
              'ses_bbb', 'proj1', 'ses_aaa', 'subagent',
              1741600001000, NULL, NULL
          );
          -- archived session (should be filtered out)
          INSERT INTO session VALUES (
              'ses_ccc', 'proj1', NULL, 'old',
              1741599000000, 1741600000000, NULL
          );

          INSERT INTO message VALUES ('msg1', 'ses_aaa', 1741600000001);
          INSERT INTO message VALUES ('msg2', 'ses_aaa', 1741600000002);

          INSERT INTO part VALUES (
              'prt1', 'msg1', 'ses_aaa', 1741600000001,
              '{\"type\":\"text\",\"text\":\"help me write tests\"}'
          );
      ").unwrap();
      conn
  }

  #[test]
  fn test_query_returns_top_level_sessions_only() {
      let conn = setup_fixture_db();
      let sessions = query_sessions_from_conn(&conn).unwrap();
      assert_eq!(sessions.len(), 1, "expected 1 session, got: {:?}", sessions.iter().map(|s| &s.session_id).collect::<Vec<_>>());
      assert_eq!(sessions[0].session_id, "ses_aaa");
  }

  #[test]
  fn test_query_session_title_and_project_path() {
      let conn = setup_fixture_db();
      let sessions = query_sessions_from_conn(&conn).unwrap();
      assert_eq!(sessions[0].summary, "my title");
      assert_eq!(sessions[0].project_path, "/home/user/ai/myproj");
  }

  #[test]
  fn test_query_message_count() {
      let conn = setup_fixture_db();
      let sessions = query_sessions_from_conn(&conn).unwrap();
      assert_eq!(sessions[0].message_count, 2);
  }

  #[test]
  fn test_query_first_user_msg_extracted_from_parts() {
      let conn = setup_fixture_db();
      let sessions = query_sessions_from_conn(&conn).unwrap();
      assert_eq!(sessions[0].first_user_msg, "help me write tests");
  }
  ```

- [ ] **Step 2: Run tests to verify they fail**

  Run: `~/.cargo/bin/cargo test --test opencode_sessions_test 2>&1 | tail -5`
  Expected: compile error — `query_sessions_from_conn` not found

- [ ] **Step 3: Implement `query_sessions_from_conn` and `list_opencode_sessions` in `src/opencode_sessions.rs`**

  Replace the contents of `src/opencode_sessions.rs` with:

  ```rust
  use anyhow::Result;
  use rusqlite::{Connection, OptionalExtension};
  use std::path::PathBuf;
  use std::time::{Duration, UNIX_EPOCH};
  use crate::unified::{UnifiedSession, SessionSource};

  /// Path to the OpenCode SQLite database (~/.local/share/opencode/opencode.db).
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
  ```

- [ ] **Step 4: Run tests**

  Run: `~/.cargo/bin/cargo test --test opencode_sessions_test 2>&1 | tail -15`
  Expected: 5 tests PASS (db_path + 4 query tests)

- [ ] **Step 5: Commit**

  ```bash
  git add src/opencode_sessions.rs tests/opencode_sessions_test.rs
  git commit -m "feat: OpenCode SQLite session reader with query_sessions_from_conn"
  ```

---

### Task 6: `unified.rs` — `list_all_sessions()` merge function

**Files:**
- Modify: `src/unified.rs`
- Modify: `tests/unified_test.rs`

- [ ] **Step 1: Write failing test**

  Append to `tests/unified_test.rs`:

  ```rust
  use cc_speedy::unified::list_all_sessions;

  #[test]
  fn test_list_all_sessions_does_not_panic() {
      // Smoke test: just ensure it returns without panicking.
      // Real DB may or may not exist on the test machine.
      let result = list_all_sessions();
      assert!(result.is_ok(), "list_all_sessions returned error: {:?}", result);
  }
  ```

- [ ] **Step 2: Run test to verify it fails**

  Run: `~/.cargo/bin/cargo test test_list_all_sessions 2>&1 | tail -5`
  Expected: compile error — `list_all_sessions` not found

- [ ] **Step 3: Add `list_all_sessions` to `src/unified.rs`**

  Append to `src/unified.rs`:

  ```rust
  use anyhow::Result;
  use crate::sessions::list_sessions;
  use crate::opencode_sessions::list_opencode_sessions;

  /// Merge Claude Code and OpenCode sessions into a single list sorted by recency.
  pub fn list_all_sessions() -> Result<Vec<UnifiedSession>> {
      let cc = list_sessions()
          .unwrap_or_default()
          .into_iter()
          .map(UnifiedSession::from)
          .collect::<Vec<_>>();

      let oc = list_opencode_sessions().unwrap_or_default();

      let mut all: Vec<UnifiedSession> = cc.into_iter().chain(oc).collect();
      all.sort_by(|a, b| b.modified.cmp(&a.modified));
      Ok(all)
  }
  ```

- [ ] **Step 4: Run tests**

  Run: `~/.cargo/bin/cargo test test_list_all_sessions 2>&1 | tail -10`
  Expected: 1 test PASS

- [ ] **Step 5: Run full test suite**

  Run: `~/.cargo/bin/cargo test 2>&1 | tail -15`
  Expected: all tests PASS

- [ ] **Step 6: Commit**

  ```bash
  git add src/unified.rs tests/unified_test.rs
  git commit -m "feat: list_all_sessions merges Claude Code and OpenCode sessions by recency"
  ```

---

## Chunk 3: TUI and Tmux Changes

### Task 7: Extend `tmux.rs` with OpenCode resume and `cc-`/`oc-` prefixed session names

**Files:**
- Modify: `src/tmux.rs`
- Modify: `tests/tmux_test.rs`

- [ ] **Step 1: Write failing tests**

  Append to `tests/tmux_test.rs`:

  ```rust
  use cc_speedy::tmux::{cc_session_name, oc_session_name};

  #[test]
  fn test_cc_session_name_has_prefix() {
      let name = cc_session_name("/home/user/ai/myproj");
      assert!(name.starts_with("cc-"), "expected cc- prefix, got: {}", name);
      assert!(name.contains("ai"), "expected path segment in name: {}", name);
  }

  #[test]
  fn test_oc_session_name_has_prefix() {
      let name = oc_session_name("/home/user/ai/myproj");
      assert!(name.starts_with("oc-"), "expected oc- prefix, got: {}", name);
  }

  #[test]
  fn test_cc_session_name_max_50_chars() {
      let long = "/a/b/c/this-is-a-very-long-project-directory-name-that-exceeds-limits";
      assert!(cc_session_name(long).len() <= 50);
  }

  #[test]
  fn test_oc_session_name_max_50_chars() {
      let long = "/a/b/c/this-is-a-very-long-project-directory-name-that-exceeds-limits";
      assert!(oc_session_name(long).len() <= 50);
  }
  ```

- [ ] **Step 2: Run tests to verify they fail**

  Run: `~/.cargo/bin/cargo test --test tmux_test test_cc_session_name test_oc_session_name 2>&1 | tail -5`
  Expected: compile error — functions not found

- [ ] **Step 3: Add `cc_session_name`, `oc_session_name`, and `resume_opencode_in_tmux` to `src/tmux.rs`**

  Append to `src/tmux.rs` (keep existing `session_name_from_path` and `resume_in_tmux` unchanged for now):

  ```rust
  /// Tmux session name for a Claude Code session: "cc-<last-2-path-segments>", max 50 chars.
  pub fn cc_session_name(project_path: &str) -> String {
      let base = session_name_from_path(project_path);
      format!("cc-{}", base).chars().take(50).collect()
  }

  /// Tmux session name for an OpenCode session: "oc-<last-2-path-segments>", max 50 chars.
  pub fn oc_session_name(project_path: &str) -> String {
      let base = session_name_from_path(project_path);
      format!("oc-{}", base).chars().take(50).collect()
  }

  /// Resume an OpenCode session in a named tmux session.
  /// Runs `opencode` in the project directory (OpenCode loads the most recent
  /// session for that directory automatically).
  pub fn resume_opencode_in_tmux(
      session_name: &str,
      project_path: &str,
      window_title: &str,
  ) -> anyhow::Result<()> {
      let new_session = |detach: bool| -> anyhow::Result<()> {
          let mut cmd = std::process::Command::new("tmux");
          cmd.arg("new-session");
          if detach { cmd.arg("-d"); }
          cmd.args(["-s", session_name, "-n", window_title, "-c", project_path, "opencode"]);
          let status = cmd.status()?;
          if !status.success() {
              anyhow::bail!("tmux new-session failed: {}", status);
          }
          Ok(())
      };

      if is_inside_tmux() {
          if session_exists(session_name) {
              let status = std::process::Command::new("tmux")
                  .args(["switch-client", "-t", session_name])
                  .status()?;
              if !status.success() {
                  anyhow::bail!("tmux switch-client failed: {}", status);
              }
              pin_window_title(session_name, window_title);
          } else {
              new_session(true)?;
              let status = std::process::Command::new("tmux")
                  .args(["switch-client", "-t", session_name])
                  .status()?;
              if !status.success() {
                  anyhow::bail!("tmux switch-client failed: {}", status);
              }
              pin_window_title(session_name, window_title);
          }
      } else {
          if !session_exists(session_name) {
              new_session(true)?;
          }
          pin_window_title(session_name, window_title);
          let status = std::process::Command::new("tmux")
              .args(["attach-session", "-t", session_name])
              .status()?;
          if !status.success() {
              anyhow::bail!("tmux attach-session failed: {}", status);
          }
      }
      Ok(())
  }
  ```

- [ ] **Step 4: Run tests**

  Run: `~/.cargo/bin/cargo test --test tmux_test 2>&1 | tail -15`
  Expected: all tmux tests PASS (including existing ones)

- [ ] **Step 5: Commit**

  ```bash
  git add src/tmux.rs tests/tmux_test.rs
  git commit -m "feat: add cc_/oc_ prefixed session names and resume_opencode_in_tmux"
  ```

---

### Task 8: Update `tui.rs` to use `UnifiedSession`, add source badge and source filter

**Files:**
- Modify: `src/tui.rs`

This task has no new tests (TUI code is UI-only; existing integration is verified by running the binary). The changes are:

1. Replace `use crate::sessions::{list_sessions, Session}` with `use crate::unified::{list_all_sessions, UnifiedSession, SessionSource}`
2. Replace all `Session` type annotations with `UnifiedSession`
3. Update `list_state` / `AppState` to hold `Vec<UnifiedSession>`
4. Add a `source_filter: Option<SessionSource>` field to `AppState`
5. Update `apply_filter` to also apply `source_filter`
6. Update `draw_list` to show `[CC]` / `[OC]` badge with different colours
7. Add key handlers `'1'`, `'2'`, `'0'` for source filter
8. Update `Enter` / `Ctrl+Y` dispatch to call the right resume function (`resume_in_tmux` for CC, `resume_opencode_in_tmux` for OC)
9. Update summary path lookup to call `opencode_summary_path` for OC sessions
10. Update status bar hint to include `1:CC  2:OC  0:all`

- [ ] **Step 1: Replace session import and type at top of `src/tui.rs`**

  Replace:
  ```rust
  use crate::sessions::{list_sessions, Session};
  use crate::summary::{read_summary, summary_path};
  ```
  With:
  ```rust
  use crate::unified::{list_all_sessions, UnifiedSession, SessionSource};
  use crate::summary::{read_summary, summary_path, opencode_summary_path};
  ```

- [ ] **Step 2: Update `AppState` struct**

  Replace the `sessions` and `filtered` field types, and add `source_filter`:

  ```rust
  struct AppState {
      sessions: Vec<UnifiedSession>,
      filtered: Vec<usize>,
      list_state: ListState,
      filter: String,
      mode: AppMode,
      rename_input: String,
      summaries: Arc<Mutex<std::collections::HashMap<String, String>>>,
      generating: Arc<Mutex<std::collections::HashSet<String>>>,
      focus: Focus,
      preview_scroll: u16,
      status_msg: Option<(String, Instant)>,
      source_filter: Option<SessionSource>,  // None = all, Some(CC) = CC only, Some(OC) = OC only
  }
  ```

  Update `AppState::new` to initialise `source_filter: None`.

- [ ] **Step 3: Update `apply_filter` to respect `source_filter`**

  Replace the filter predicate closure with:

  ```rust
  fn apply_filter(&mut self) {
      let q = self.filter.to_lowercase();
      self.filtered = self
          .sessions
          .iter()
          .enumerate()
          .filter(|(_, s)| {
              // Source filter
              if let Some(ref sf) = self.source_filter {
                  if &s.source != sf { return false; }
              }
              // Text filter
              q.is_empty()
                  || s.project_name.to_lowercase().contains(&q)
                  || s.summary.to_lowercase().contains(&q)
          })
          .map(|(i, _)| i)
          .collect();
      if !self.filtered.is_empty() {
          self.list_state.select(Some(0));
      } else {
          self.list_state.select(None);
      }
  }
  ```

- [ ] **Step 4: Update `run()` to call `list_all_sessions()`**

  Replace:
  ```rust
  let sessions = list_sessions()?;
  ```
  With:
  ```rust
  let sessions = list_all_sessions()?;
  ```

- [ ] **Step 5: Update summary path lookup in `run()` pre-load block**

  Replace:
  ```rust
  let path = summary_path(&session.session_id);
  ```
  With:
  ```rust
  let path = match session.source {
      SessionSource::ClaudeCode => summary_path(&session.session_id),
      SessionSource::OpenCode   => opencode_summary_path(&session.session_id),
  };
  ```

- [ ] **Step 6: Add source filter key handlers `'1'`, `'2'`, `'0'` in `run_event_loop`**

  Inside the `(AppMode::Normal, _, ...)` match arm block, add:

  ```rust
  (AppMode::Normal, KeyModifiers::NONE, KeyCode::Char('1')) => {
      app.source_filter = Some(SessionSource::ClaudeCode);
      app.apply_filter();
  }
  (AppMode::Normal, KeyModifiers::NONE, KeyCode::Char('2')) => {
      app.source_filter = Some(SessionSource::OpenCode);
      app.apply_filter();
  }
  (AppMode::Normal, KeyModifiers::NONE, KeyCode::Char('0')) => {
      app.source_filter = None;
      app.apply_filter();
  }
  ```

- [ ] **Step 7: Update `Enter` dispatch to route by source**

  Replace the `(AppMode::Normal, _, KeyCode::Enter)` arm with:

  ```rust
  (AppMode::Normal, _, KeyCode::Enter) => {
      if let Some(s) = app.selected_session() {
          let path  = s.project_path.clone();
          let id    = s.session_id.clone();
          let title = window_title_from_session(s);
          match s.source {
              SessionSource::ClaudeCode => {
                  let name = crate::tmux::cc_session_name(&path);
                  return crate::tmux::resume_in_tmux(&name, &path, &id, false, &title);
              }
              SessionSource::OpenCode => {
                  let name = crate::tmux::oc_session_name(&path);
                  return crate::tmux::resume_opencode_in_tmux(&name, &path, &title);
              }
          }
      }
  }
  ```

- [ ] **Step 8: Update `Ctrl+Y` (yolo) dispatch to route by source**

  Replace the `(AppMode::Normal, KeyModifiers::CONTROL, KeyCode::Char('y'))` arm with:

  ```rust
  (AppMode::Normal, KeyModifiers::CONTROL, KeyCode::Char('y')) => {
      if let Some(s) = app.selected_session() {
          let path  = s.project_path.clone();
          let id    = s.session_id.clone();
          let title = window_title_from_session(s);
          match s.source {
              SessionSource::ClaudeCode => {
                  let name = crate::tmux::cc_session_name(&path);
                  return crate::tmux::resume_in_tmux(&name, &path, &id, true, &title);
              }
              SessionSource::OpenCode => {
                  // OpenCode has no --dangerously-skip-permissions; fall back to normal resume
                  let name = crate::tmux::oc_session_name(&path);
                  return crate::tmux::resume_opencode_in_tmux(&name, &path, &title);
              }
          }
      }
  }
  ```

- [ ] **Step 9: Add `[CC]` / `[OC]` badge in `draw_list`**

  In `draw_list`, update the `Line::from` construction to prepend a coloured badge span:

  ```rust
  let (badge_text, badge_color) = match s.source {
      SessionSource::ClaudeCode => ("[CC]", Color::Green),
      SessionSource::OpenCode   => ("[OC]", Color::Cyan),
  };
  let line = Line::from(vec![
      Span::styled(format!("{} ", dt), Style::default().fg(Color::DarkGray)),
      Span::styled(format!("{} ", badge_text), Style::default().fg(badge_color)),
      Span::raw(format!("{:<22}", label)),
      Span::styled(format!("{:>4} ", s.message_count), Style::default().fg(Color::DarkGray)),
      Span::styled(folder, Style::default().fg(Color::DarkGray)),
  ]);
  ```

- [ ] **Step 10: Update summary generation in `spawn_summary_generation` to use correct path**

  The function signature must accept a `source: SessionSource`. Update the call sites and the function to pick the right path:

  ```rust
  fn spawn_summary_generation(
      id: String,
      jsonl: Option<String>,   // None for OC sessions
      source: SessionSource,
      summaries: Arc<Mutex<std::collections::HashMap<String, String>>>,
      generating: Arc<Mutex<std::collections::HashSet<String>>>,
  ) {
      generating.lock().expect("generating mutex poisoned").insert(id.clone());
      tokio::spawn(async move {
          // For OC sessions without a jsonl, query the DB for messages
          // For now: skip auto-generation if no jsonl (manual Ctrl+R only)
          let Some(jsonl_path) = jsonl else {
              generating.lock().expect("generating mutex poisoned").remove(&id);
              return;
          };
          let msgs = tokio::task::spawn_blocking({
              let p = jsonl_path.clone();
              move || crate::sessions::parse_messages(std::path::Path::new(&p))
          }).await.ok().and_then(|r| r.ok());
          if let Some(msgs) = msgs {
              match crate::summary::generate_summary(&msgs).await {
                  Ok(text) => {
                      let out = match source {
                          SessionSource::ClaudeCode => crate::summary::summary_path(&id),
                          SessionSource::OpenCode   => crate::summary::opencode_summary_path(&id),
                      };
                      let _ = crate::summary::write_summary(&out, &text);
                      summaries.lock().expect("summary mutex poisoned").insert(id.clone(), text);
                  }
                  Err(e) => {
                      summaries.lock().expect("summary mutex poisoned")
                          .insert(id.clone(), format!("Error: {}", e));
                  }
              }
          }
          generating.lock().expect("generating mutex poisoned").remove(&id);
      });
  }
  ```

- [ ] **Step 11: Update status bar hint string**

  Replace the status bar text with:
  ```rust
  " 1:CC  2:OC  0:all  /: filter  Enter: resume  Ctrl+Y: yolo  Tab  j/k  r  c  Ctrl+R  q"
  ```

- [ ] **Step 12: Verify compile**

  Run: `~/.cargo/bin/cargo build 2>&1 | grep "^error"`
  Expected: no output

- [ ] **Step 13: Run full test suite**

  Run: `~/.cargo/bin/cargo test 2>&1 | tail -20`
  Expected: all tests PASS

- [ ] **Step 14: Commit**

  ```bash
  git add src/tui.rs
  git commit -m "feat: unified TUI with [CC]/[OC] badges, source filter (1/2/0), routed resume"
  ```

---

### Task 9: Update `main.rs` — wire `list_all_sessions` as default

No changes needed to `main.rs` — the default `tui::run()` path already calls `list_all_sessions` via the updated `tui.rs`. This task is a verification checkpoint.

- [ ] **Step 1: Smoke test the binary**

  Run: `~/.cargo/bin/cargo run 2>&1 | head -3`
  Expected: TUI launches (exit immediately with `q`). If OpenCode DB exists, both `[CC]` and `[OC]` sessions appear.

- [ ] **Step 2: Verify all tests still pass**

  Run: `~/.cargo/bin/cargo test 2>&1 | grep "test result"`
  Expected: all suites report `ok`

- [ ] **Step 3: Commit**

  ```bash
  git add -A
  git commit -m "feat: cc-speedy v0.2.0 — unified Claude Code + OpenCode session browser"
  ```
