# Copilot CLI Session Integration — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add GitHub Copilot CLI as a third session source (`[CO]`) in cc-speedy so users can browse, filter, summarize, pin, and resume Copilot sessions from the same TUI alongside CC and OC sessions.

**Architecture:** A new `copilot_sessions.rs` module reads `~/.copilot/session-state/<uuid>/workspace.yaml` (flat key:value) for metadata and `events.jsonl` for messages, following the same `UnifiedSession` pattern as `opencode_sessions.rs`. `SessionSource::Copilot` is added to the shared enum, and every match arm in `tui.rs` and `tmux.rs` is extended with a Copilot branch.

**Tech Stack:** Rust, `chrono` (already in Cargo.toml) for ISO 8601 timestamp parsing, `serde_json` for events.jsonl, `tempfile` (already in dev-dependencies) for tests.

---

## File Map

| File | Action | Responsibility |
|---|---|---|
| `src/copilot_sessions.rs` | **Create** | Parse workspace.yaml + events.jsonl; list and message-parse Copilot sessions |
| `tests/copilot_sessions_test.rs` | **Create** | Integration tests for session listing and message parsing |
| `src/lib.rs` | **Modify** | Register `pub mod copilot_sessions` |
| `src/unified.rs` | **Modify** | Add `SessionSource::Copilot`; merge Copilot into `list_all_sessions()` |
| `src/theme.rs` | **Modify** | Add `CO_BADGE` color constant |
| `src/tmux.rs` | **Modify** | Add `copilot_session_name`, `new_copilot_session_name`, `resume_copilot_in_tmux`, `new_copilot_in_tmux` |
| `src/tui.rs` | **Modify** | Add source filter key `'3'`, `[CO]` badge, all Copilot match arms, update status bar |

---

## Task 1: Create `copilot_sessions.rs` stub + register module

**Files:**
- Create: `src/copilot_sessions.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Add module registration to `src/lib.rs`**

Open `src/lib.rs` and add one line after `pub mod opencode_sessions;`:

```rust
pub mod copilot_sessions;
```

- [ ] **Step 2: Create the stub `src/copilot_sessions.rs`**

```rust
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};
use crate::sessions::Message;
use crate::unified::{UnifiedSession, SessionSource};

pub fn copilot_sessions_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".copilot").join("session-state"))
}

pub fn list_copilot_sessions() -> Result<Vec<UnifiedSession>> {
    Ok(vec![])
}

pub fn list_copilot_sessions_from_dir(_base: &Path) -> Result<Vec<UnifiedSession>> {
    Ok(vec![])
}

pub fn parse_copilot_messages(_session_id: &str) -> Result<Vec<Message>> {
    Ok(vec![])
}

pub fn parse_copilot_messages_from_path(_path: &Path) -> Result<Vec<Message>> {
    Ok(vec![])
}
```

- [ ] **Step 3: Verify it compiles**

```bash
cargo build 2>&1
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/lib.rs src/copilot_sessions.rs
git commit -m "feat: add copilot_sessions stub module"
```

---

## Task 2: Implement message parsing (`parse_copilot_messages_from_path`)

**Files:**
- Modify: `src/copilot_sessions.rs`
- Create: `tests/copilot_sessions_test.rs`

- [ ] **Step 1: Write failing tests**

Create `tests/copilot_sessions_test.rs`:

```rust
use cc_speedy::copilot_sessions::parse_copilot_messages_from_path;
use tempfile::TempDir;
use std::fs;

#[test]
fn test_parse_messages_user_and_assistant() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("events.jsonl");
    fs::write(&path, concat!(
        "{\"type\":\"session.start\",\"data\":{}}\n",
        "{\"type\":\"user.message\",\"data\":{\"content\":\"Hello\"}}\n",
        "{\"type\":\"assistant.message\",\"data\":{\"content\":\"Hi there\"}}\n",
        "{\"type\":\"tool.execution_start\",\"data\":{}}\n",
    )).unwrap();
    let msgs = parse_copilot_messages_from_path(&path).unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].role, "user");
    assert_eq!(msgs[0].text, "Hello");
    assert_eq!(msgs[1].role, "assistant");
    assert_eq!(msgs[1].text, "Hi there");
}

#[test]
fn test_parse_messages_skips_empty_assistant_content() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("events.jsonl");
    fs::write(&path, concat!(
        "{\"type\":\"user.message\",\"data\":{\"content\":\"query\"}}\n",
        "{\"type\":\"assistant.message\",\"data\":{\"content\":\"\"}}\n",
        "{\"type\":\"assistant.message\",\"data\":{\"content\":\"answer\"}}\n",
    )).unwrap();
    let msgs = parse_copilot_messages_from_path(&path).unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[1].text, "answer");
}

#[test]
fn test_parse_messages_skips_non_message_events() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("events.jsonl");
    fs::write(&path, concat!(
        "{\"type\":\"session.start\",\"data\":{}}\n",
        "{\"type\":\"assistant.turn_start\",\"data\":{}}\n",
        "{\"type\":\"user.message\",\"data\":{\"content\":\"only msg\"}}\n",
        "{\"type\":\"tool.execution_complete\",\"data\":{}}\n",
    )).unwrap();
    let msgs = parse_copilot_messages_from_path(&path).unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].text, "only msg");
}
```

- [ ] **Step 2: Run to confirm they fail**

```bash
cargo test copilot_sessions 2>&1 | head -30
```

Expected: compile error or test failures on the stub.

- [ ] **Step 3: Implement `parse_copilot_messages_from_path` in `src/copilot_sessions.rs`**

Replace the two stub functions at the bottom with:

```rust
pub fn parse_copilot_messages(session_id: &str) -> Result<Vec<Message>> {
    let path = match copilot_sessions_dir() {
        Some(d) => d.join(session_id).join("events.jsonl"),
        None => return Ok(vec![]),
    };
    if !path.exists() {
        return Ok(vec![]);
    }
    parse_copilot_messages_from_path(&path)
}

pub fn parse_copilot_messages_from_path(path: &Path) -> Result<Vec<Message>> {
    let content = std::fs::read_to_string(path)?;
    let mut messages = Vec::new();
    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        let role = match v["type"].as_str().unwrap_or("") {
            "user.message" => "user",
            "assistant.message" => "assistant",
            _ => continue,
        };
        let text = v["data"]["content"].as_str().unwrap_or("");
        if text.is_empty() { continue; }
        messages.push(Message { role: role.to_string(), text: text.to_string() });
    }
    Ok(messages)
}
```

- [ ] **Step 4: Run tests to confirm they pass**

```bash
cargo test test_parse_messages 2>&1
```

Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/copilot_sessions.rs tests/copilot_sessions_test.rs
git commit -m "feat: implement copilot events.jsonl message parser"
```

---

## Task 3: Implement session listing (`list_copilot_sessions_from_dir`)

**Files:**
- Modify: `src/copilot_sessions.rs`
- Modify: `tests/copilot_sessions_test.rs`

- [ ] **Step 1: Write failing tests — append to `tests/copilot_sessions_test.rs`**

```rust
use cc_speedy::copilot_sessions::list_copilot_sessions_from_dir;

fn make_session(base: &TempDir, id: &str, yaml: &str, jsonl: &str) {
    let dir = base.path().join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("workspace.yaml"), yaml).unwrap();
    fs::write(dir.join("events.jsonl"), jsonl).unwrap();
}

const FOUR_MSGS: &str = concat!(
    "{\"type\":\"user.message\",\"data\":{\"content\":\"q1\"}}\n",
    "{\"type\":\"assistant.message\",\"data\":{\"content\":\"a1\"}}\n",
    "{\"type\":\"user.message\",\"data\":{\"content\":\"q2\"}}\n",
    "{\"type\":\"assistant.message\",\"data\":{\"content\":\"a2\"}}\n",
);

const THREE_MSGS: &str = concat!(
    "{\"type\":\"user.message\",\"data\":{\"content\":\"q1\"}}\n",
    "{\"type\":\"assistant.message\",\"data\":{\"content\":\"a1\"}}\n",
    "{\"type\":\"user.message\",\"data\":{\"content\":\"q2\"}}\n",
);

#[test]
fn test_list_sessions_filters_under_4_messages() {
    let tmp = TempDir::new().unwrap();
    make_session(&tmp, "sess-a",
        "id: sess-a\ncwd: /home/user/proj\nupdated_at: 2026-01-01T00:00:00Z\n",
        THREE_MSGS,
    );
    make_session(&tmp, "sess-b",
        "id: sess-b\ncwd: /home/user/proj2\nupdated_at: 2026-01-02T00:00:00Z\n",
        FOUR_MSGS,
    );
    let sessions = list_copilot_sessions_from_dir(tmp.path()).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_id, "sess-b");
}

#[test]
fn test_list_sessions_skips_dirs_without_workspace_yaml() {
    let tmp = TempDir::new().unwrap();
    // Dir without workspace.yaml (old format)
    let legacy = tmp.path().join("legacy-session");
    fs::create_dir_all(&legacy).unwrap();
    fs::write(legacy.join("events.jsonl"), FOUR_MSGS).unwrap();
    // Dir with workspace.yaml
    make_session(&tmp, "valid-session",
        "id: valid-session\ncwd: /home/user/proj\nupdated_at: 2026-01-01T00:00:00Z\n",
        FOUR_MSGS,
    );
    let sessions = list_copilot_sessions_from_dir(tmp.path()).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_id, "valid-session");
}

#[test]
fn test_session_title_name_takes_priority_over_summary() {
    let tmp = TempDir::new().unwrap();
    make_session(&tmp, "sess-x",
        "id: sess-x\ncwd: /home/user/proj\nupdated_at: 2026-01-01T00:00:00Z\nname: my-name\nsummary: my-summary\n",
        FOUR_MSGS,
    );
    let sessions = list_copilot_sessions_from_dir(tmp.path()).unwrap();
    assert_eq!(sessions[0].summary, "my-name");
}

#[test]
fn test_session_title_falls_back_to_summary() {
    let tmp = TempDir::new().unwrap();
    make_session(&tmp, "sess-y",
        "id: sess-y\ncwd: /home/user/proj\nupdated_at: 2026-01-01T00:00:00Z\nsummary: my-summary\n",
        FOUR_MSGS,
    );
    let sessions = list_copilot_sessions_from_dir(tmp.path()).unwrap();
    assert_eq!(sessions[0].summary, "my-summary");
}

#[test]
fn test_session_git_branch_extracted() {
    let tmp = TempDir::new().unwrap();
    make_session(&tmp, "sess-z",
        "id: sess-z\ncwd: /home/user/repo\nupdated_at: 2026-01-01T00:00:00Z\nbranch: feature-x\n",
        FOUR_MSGS,
    );
    let sessions = list_copilot_sessions_from_dir(tmp.path()).unwrap();
    assert_eq!(sessions[0].git_branch, "feature-x");
}

#[test]
fn test_session_first_user_message_truncated_to_80_chars() {
    let tmp = TempDir::new().unwrap();
    let long_msg = "a".repeat(200);
    let jsonl = format!(
        "{{\"type\":\"user.message\",\"data\":{{\"content\":\"{}\"}}}}\n\
         {{\"type\":\"assistant.message\",\"data\":{{\"content\":\"a1\"}}}}\n\
         {{\"type\":\"user.message\",\"data\":{{\"content\":\"q2\"}}}}\n\
         {{\"type\":\"assistant.message\",\"data\":{{\"content\":\"a2\"}}}}\n",
        long_msg
    );
    make_session(&tmp, "sess-long",
        "id: sess-long\ncwd: /home/user/proj\nupdated_at: 2026-01-01T00:00:00Z\n",
        &jsonl,
    );
    let sessions = list_copilot_sessions_from_dir(tmp.path()).unwrap();
    assert_eq!(sessions[0].first_user_msg.len(), 80);
}

#[test]
fn test_list_returns_empty_for_nonexistent_dir() {
    let result = list_copilot_sessions_from_dir(std::path::Path::new("/nonexistent/path/xyz"));
    assert!(result.unwrap().is_empty());
}
```

- [ ] **Step 2: Run to confirm failures**

```bash
cargo test test_list_sessions 2>&1 | head -30
```

Expected: compile errors or test failures.

- [ ] **Step 3: Implement the full session listing in `src/copilot_sessions.rs`**

Replace the entire file content with:

```rust
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};
use crate::sessions::Message;
use crate::unified::{UnifiedSession, SessionSource};

pub fn copilot_sessions_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".copilot").join("session-state"))
}

struct WorkspaceYaml {
    id: String,
    cwd: String,
    summary: Option<String>,
    name: Option<String>,
    updated_at: Option<String>,
    branch: Option<String>,
}

fn parse_workspace_yaml(path: &Path) -> Option<WorkspaceYaml> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut id = String::new();
    let mut cwd = String::new();
    let mut summary = None;
    let mut name = None;
    let mut updated_at = None;
    let mut branch = None;

    for line in content.lines() {
        // split_once(':') splits on the FIRST colon only, so ISO timestamps
        // (e.g. "2026-03-12T01:18:55Z") come through intact in the value.
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim();
            let val = v.trim().to_string();
            if val.is_empty() { continue; }
            match key {
                "id"         => id = val,
                "cwd"        => cwd = val,
                "summary"    => summary = Some(val),
                "name"       => name = Some(val),
                "updated_at" => updated_at = Some(val),
                "branch"     => branch = Some(val),
                _            => {}
            }
        }
    }

    if id.is_empty() || cwd.is_empty() { return None; }
    Some(WorkspaceYaml { id, cwd, summary, name, updated_at, branch })
}

fn parse_iso8601(s: &str) -> Option<std::time::SystemTime> {
    use chrono::DateTime;
    let dt = DateTime::parse_from_rfc3339(s).ok()?;
    let secs = dt.timestamp();
    if secs < 0 { return None; }
    Some(UNIX_EPOCH + Duration::from_secs(secs as u64))
}

fn count_messages_and_first(path: &Path) -> (usize, String) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return (0, String::new()),
    };
    let mut count = 0usize;
    let mut first_user = String::new();
    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        let t = v["type"].as_str().unwrap_or("");
        match t {
            "user.message" | "assistant.message" => {
                count += 1;
                if t == "user.message" && first_user.is_empty() {
                    if let Some(text) = v["data"]["content"].as_str() {
                        first_user = text.chars().take(80).collect();
                    }
                }
            }
            _ => {}
        }
    }
    (count, first_user)
}

pub fn list_copilot_sessions() -> Result<Vec<UnifiedSession>> {
    let base = match copilot_sessions_dir() {
        Some(p) if p.exists() => p,
        _ => return Ok(vec![]),
    };
    list_copilot_sessions_from_dir(&base)
}

pub fn list_copilot_sessions_from_dir(base: &Path) -> Result<Vec<UnifiedSession>> {
    let entries = match std::fs::read_dir(base) {
        Ok(e) => e,
        Err(_) => return Ok(vec![]),
    };
    let mut sessions = Vec::new();
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let ws = match parse_workspace_yaml(&path.join("workspace.yaml")) {
            Some(ws) => ws,
            None => continue,
        };
        let (message_count, first_user_msg) =
            count_messages_and_first(&path.join("events.jsonl"));
        if message_count < 4 { continue; }
        let modified = ws.updated_at
            .as_deref()
            .and_then(parse_iso8601)
            .unwrap_or(UNIX_EPOCH);
        let title = ws.name.or(ws.summary).unwrap_or_default();
        let project_name = crate::util::path_last_n(&ws.cwd, 2);
        sessions.push(UnifiedSession {
            session_id:    ws.id,
            project_name,
            project_path:  ws.cwd,
            modified,
            message_count,
            first_user_msg,
            summary:       title,
            git_branch:    ws.branch.unwrap_or_default(),
            source:        SessionSource::Copilot,
            jsonl_path:    None,
        });
    }
    Ok(sessions)
}

pub fn parse_copilot_messages(session_id: &str) -> Result<Vec<Message>> {
    let path = match copilot_sessions_dir() {
        Some(d) => d.join(session_id).join("events.jsonl"),
        None => return Ok(vec![]),
    };
    if !path.exists() { return Ok(vec![]); }
    parse_copilot_messages_from_path(&path)
}

pub fn parse_copilot_messages_from_path(path: &Path) -> Result<Vec<Message>> {
    let content = std::fs::read_to_string(path)?;
    let mut messages = Vec::new();
    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        let role = match v["type"].as_str().unwrap_or("") {
            "user.message"      => "user",
            "assistant.message" => "assistant",
            _                   => continue,
        };
        let text = v["data"]["content"].as_str().unwrap_or("");
        if text.is_empty() { continue; }
        messages.push(Message { role: role.to_string(), text: text.to_string() });
    }
    Ok(messages)
}
```

- [ ] **Step 4: Run all copilot tests**

```bash
cargo test copilot_sessions 2>&1
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/copilot_sessions.rs tests/copilot_sessions_test.rs
git commit -m "feat: implement copilot session listing from workspace.yaml + events.jsonl"
```

---

## Task 4: Add tmux functions for Copilot

**Files:**
- Modify: `src/tmux.rs`
- Modify: `tests/tmux_test.rs`

- [ ] **Step 1: Write failing tests — append to `tests/tmux_test.rs`**

```rust
use cc_speedy::tmux::copilot_session_name;

#[test]
fn test_copilot_session_name_has_co_prefix() {
    let name = copilot_session_name("/home/user/ai/myproj");
    assert!(name.starts_with("co-"), "expected co- prefix, got: {}", name);
    assert!(name.contains("ai"), "expected path segment in name: {}", name);
}

#[test]
fn test_copilot_session_name_max_50_chars() {
    let long = "/a/b/c/this-is-a-very-long-project-directory-name-that-exceeds-limits";
    assert!(copilot_session_name(long).len() <= 50);
}
```

- [ ] **Step 2: Run to confirm failure**

```bash
cargo test test_copilot_session_name 2>&1 | head -20
```

Expected: compile error — `copilot_session_name` not found.

- [ ] **Step 3: Add the four tmux functions to `src/tmux.rs`**

Append to the end of `src/tmux.rs`:

```rust
/// Tmux session name for a Copilot session: "co-<last-2-path-segments>", max 50 chars.
pub fn copilot_session_name(project_path: &str) -> String {
    let base = session_name_from_path(project_path);
    format!("co-{}", base).chars().take(50).collect()
}

/// Unique tmux session name for a brand-new Copilot conversation.
pub fn new_copilot_session_name(project_path: &str) -> String {
    let base = session_name_from_path(project_path);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("co-new-{}-{}", base, ts % 100_000)
        .chars()
        .take(50)
        .collect()
}

/// Resume a Copilot session in a named tmux session.
/// `yolo = true` adds `--allow-all` (Copilot's equivalent of --dangerously-skip-permissions).
pub fn resume_copilot_in_tmux(
    session_name: &str,
    project_path: &str,
    session_id: &str,
    yolo: bool,
    window_title: &str,
) -> Result<()> {
    let mut args = vec!["copilot"];
    if yolo {
        args.push("--allow-all");
    }
    args.extend_from_slice(&["--resume", session_id]);
    resume_in_tmux_with_cmd(session_name, project_path, window_title, &args)
}

/// Start a fresh Copilot conversation in a new tmux session.
pub fn new_copilot_in_tmux(
    session_name: &str,
    project_path: &str,
    window_title: &str,
) -> Result<()> {
    resume_in_tmux_with_cmd(session_name, project_path, window_title, &["copilot"])
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test test_copilot_session_name 2>&1
```

Expected: 2 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/tmux.rs tests/tmux_test.rs
git commit -m "feat: add copilot tmux session name and resume functions"
```

---

## Task 5: Wire Copilot into `SessionSource`, `unified.rs`, `theme.rs`, and `tui.rs`

This task makes all the Rust enum changes at once — the compiler enforces exhaustive match coverage, so all arms must be updated together.

**Files:**
- Modify: `src/unified.rs`
- Modify: `src/theme.rs`
- Modify: `src/tui.rs`

- [ ] **Step 1: Add `SessionSource::Copilot` to `src/unified.rs`**

In `src/unified.rs`, change the `SessionSource` enum from:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum SessionSource {
    ClaudeCode,
    OpenCode,
}
```

to:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum SessionSource {
    ClaudeCode,
    OpenCode,
    Copilot,
}
```

Then update `list_all_sessions()`:

```rust
pub fn list_all_sessions() -> Result<Vec<UnifiedSession>> {
    let cc = list_sessions()
        .unwrap_or_default()
        .into_iter()
        .map(UnifiedSession::from)
        .collect::<Vec<_>>();

    let oc = list_opencode_sessions().unwrap_or_default();
    let co = crate::copilot_sessions::list_copilot_sessions().unwrap_or_default();

    let mut all: Vec<UnifiedSession> = cc.into_iter().chain(oc).chain(co).collect();
    all.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(all)
}
```

- [ ] **Step 2: Check what now fails to compile**

```bash
cargo build 2>&1 | grep "error\[" | head -20
```

Expected: several non-exhaustive match errors in `tui.rs`. Note every location — you'll fix them all in Steps 3-6.

- [ ] **Step 3: Add `CO_BADGE` to `src/theme.rs`**

After the `OC_BADGE` line in `src/theme.rs`:

```rust
pub const OC_BADGE: Color = Color::Rgb(30, 144, 255); // #1e90ff  btop blue
```

Add:

```rust
pub const CO_BADGE: Color = Color::Rgb(255, 140, 0);  // #ff8c00  orange
```

- [ ] **Step 4: Update the badge match in `src/tui.rs`**

Find (in `draw_list`):

```rust
let (badge_text, badge_color) = match s.source {
    SessionSource::ClaudeCode => ("[CC]", theme::CC_BADGE),
    SessionSource::OpenCode   => ("[OC]", theme::OC_BADGE),
};
```

Replace with:

```rust
let (badge_text, badge_color) = match s.source {
    SessionSource::ClaudeCode => ("[CC]", theme::CC_BADGE),
    SessionSource::OpenCode   => ("[OC]", theme::OC_BADGE),
    SessionSource::Copilot    => ("[CO]", theme::CO_BADGE),
};
```

- [ ] **Step 5: Add source filter key `'3'` in `src/tui.rs`**

Find:

```rust
(AppMode::Normal, KeyModifiers::NONE, KeyCode::Char('2')) => {
    app.source_filter = Some(SessionSource::OpenCode);
    app.apply_filter();
}
(AppMode::Normal, KeyModifiers::NONE, KeyCode::Char('0')) => {
```

Replace with:

```rust
(AppMode::Normal, KeyModifiers::NONE, KeyCode::Char('2')) => {
    app.source_filter = Some(SessionSource::OpenCode);
    app.apply_filter();
}
(AppMode::Normal, KeyModifiers::NONE, KeyCode::Char('3')) => {
    app.source_filter = Some(SessionSource::Copilot);
    app.apply_filter();
}
(AppMode::Normal, KeyModifiers::NONE, KeyCode::Char('0')) => {
```

- [ ] **Step 6: Update all four key-action match arms in `src/tui.rs`**

**Enter (resume)** — find:

```rust
match s.source {
    SessionSource::ClaudeCode => {
        let name = crate::tmux::cc_session_name(&path);
        return crate::tmux::resume_in_tmux(&name, &path, &id, false, &title);
    }
    SessionSource::OpenCode => {
        let name = crate::tmux::oc_session_name(&path);
        return crate::tmux::resume_opencode_in_tmux(&name, &path, &id, &title);
    }
}
```

Replace with:

```rust
match s.source {
    SessionSource::ClaudeCode => {
        let name = crate::tmux::cc_session_name(&path);
        return crate::tmux::resume_in_tmux(&name, &path, &id, false, &title);
    }
    SessionSource::OpenCode => {
        let name = crate::tmux::oc_session_name(&path);
        return crate::tmux::resume_opencode_in_tmux(&name, &path, &id, &title);
    }
    SessionSource::Copilot => {
        let name = crate::tmux::copilot_session_name(&path);
        return crate::tmux::resume_copilot_in_tmux(&name, &path, &id, false, &title);
    }
}
```

**`n` (new session)** — find:

```rust
match s.source {
    SessionSource::ClaudeCode => {
        let name = crate::tmux::new_cc_session_name(&path);
        return crate::tmux::new_cc_in_tmux(&name, &path, false, &title);
    }
    SessionSource::OpenCode => {
        let name = crate::tmux::new_oc_session_name(&path);
        return crate::tmux::new_oc_in_tmux(&name, &path, &title);
    }
}
```

Replace with:

```rust
match s.source {
    SessionSource::ClaudeCode => {
        let name = crate::tmux::new_cc_session_name(&path);
        return crate::tmux::new_cc_in_tmux(&name, &path, false, &title);
    }
    SessionSource::OpenCode => {
        let name = crate::tmux::new_oc_session_name(&path);
        return crate::tmux::new_oc_in_tmux(&name, &path, &title);
    }
    SessionSource::Copilot => {
        let name = crate::tmux::new_copilot_session_name(&path);
        return crate::tmux::new_copilot_in_tmux(&name, &path, &title);
    }
}
```

**`Ctrl+N` (new session yolo)** — find:

```rust
match s.source {
    SessionSource::ClaudeCode => {
        let name = crate::tmux::new_cc_session_name(&path);
        return crate::tmux::new_cc_in_tmux(&name, &path, true, &title);
    }
    SessionSource::OpenCode => {
        let name = crate::tmux::new_oc_session_name(&path);
        return crate::tmux::new_oc_in_tmux(&name, &path, &title);
    }
}
```

Replace with:

```rust
match s.source {
    SessionSource::ClaudeCode => {
        let name = crate::tmux::new_cc_session_name(&path);
        return crate::tmux::new_cc_in_tmux(&name, &path, true, &title);
    }
    SessionSource::OpenCode => {
        let name = crate::tmux::new_oc_session_name(&path);
        return crate::tmux::new_oc_in_tmux(&name, &path, &title);
    }
    SessionSource::Copilot => {
        let name = crate::tmux::new_copilot_session_name(&path);
        return crate::tmux::new_copilot_in_tmux(&name, &path, &title);
    }
}
```

**`Ctrl+Y` (yolo resume)** — find:

```rust
match s.source {
    SessionSource::ClaudeCode => {
        let name = crate::tmux::cc_session_name(&path);
        return crate::tmux::resume_in_tmux(&name, &path, &id, true, &title);
    }
    SessionSource::OpenCode => {
        // OpenCode has no --dangerously-skip-permissions; fall back to normal resume
        let name = crate::tmux::oc_session_name(&path);
        return crate::tmux::resume_opencode_in_tmux(&name, &path, &id, &title);
    }
}
```

Replace with:

```rust
match s.source {
    SessionSource::ClaudeCode => {
        let name = crate::tmux::cc_session_name(&path);
        return crate::tmux::resume_in_tmux(&name, &path, &id, true, &title);
    }
    SessionSource::OpenCode => {
        // OpenCode has no --dangerously-skip-permissions; fall back to normal resume
        let name = crate::tmux::oc_session_name(&path);
        return crate::tmux::resume_opencode_in_tmux(&name, &path, &id, &title);
    }
    SessionSource::Copilot => {
        let name = crate::tmux::copilot_session_name(&path);
        return crate::tmux::resume_copilot_in_tmux(&name, &path, &id, true, &title);
    }
}
```

- [ ] **Step 7: Update `spawn_summary_generation` in `src/tui.rs`**

Find (in `spawn_summary_generation`):

```rust
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
};
```

Replace with:

```rust
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
```

Find the `src_str` match immediately after:

```rust
let src_str = match source {
    SessionSource::ClaudeCode => "cc",
    SessionSource::OpenCode   => "oc",
};
```

Replace with:

```rust
let src_str = match source {
    SessionSource::ClaudeCode => "cc",
    SessionSource::OpenCode   => "oc",
    SessionSource::Copilot    => "co",
};
```

- [ ] **Step 8: Update the status bar help text in `src/tui.rs`**

Find both occurrences of (the string appears twice — in the flash-expired branch and the default branch):

```rust
" 1:CC  2:OC  0:all  /: filter  Enter: resume  n: new  Ctrl+Y/N: yolo  Tab  j/k  r  c  x: pin  Ctrl+R  q"
```

Replace both with:

```rust
" 1:CC  2:OC  3:CO  0:all  /: filter  Enter: resume  n: new  Ctrl+Y/N: yolo  Tab  j/k  r  c  x: pin  Ctrl+R  q"
```

- [ ] **Step 9: Build to verify zero errors**

```bash
cargo build 2>&1
```

Expected: success, no errors.

- [ ] **Step 10: Run all tests**

```bash
cargo test 2>&1
```

Expected: all existing tests plus new copilot tests pass.

- [ ] **Step 11: Commit**

```bash
git add src/unified.rs src/theme.rs src/tui.rs
git commit -m "feat: integrate Copilot sessions into TUI — badge [CO], filter key 3, resume/yolo/new/summary"
```

---

## Smoke Test

After all tasks complete, do a quick manual check:

```bash
cargo run
```

- Verify `[CO]` sessions appear in the list (orange badge)
- Press `3` — list filters to Copilot sessions only
- Press `0` — all sources restored
- Navigate to a CO session, press `Ctrl+R` — summary generation starts
- Press `Enter` on a CO session — tmux opens `copilot --resume=<id>` in the project directory
