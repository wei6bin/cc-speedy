# cc-speedy Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a Rust TUI that lists Claude Code sessions, shows AI-generated summaries, and resumes them in named tmux sessions.

**Architecture:** Single binary with three subcommands (`list`/default, `summarize`, `install`). Sessions are parsed from `~/.claude/projects/**/*.jsonl`, summaries stored in `~/.claude/summaries/<session-id>.md`. TUI built with ratatui + crossterm; tmux integration via shell-out to `tmux` CLI.

**Tech Stack:** Rust 1.93, ratatui 0.29, crossterm 0.28, serde_json 1, reqwest 0.12 (async), tokio 1, dirs 6

---

### Task 1: Project Scaffold

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`

**Step 1: Init cargo project**

```bash
cd /home/weibin/repo/ai/cc-speedy
cargo init --name cc-speedy
```

**Step 2: Replace Cargo.toml**

```toml
[package]
name = "cc-speedy"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "cc-speedy"
path = "src/main.rs"

[dependencies]
ratatui = "0.29"
crossterm = "0.28"
serde_json = "1"
reqwest = { version = "0.12", features = ["json"] }
tokio = { version = "1", features = ["full"] }
dirs = "6"
serde = { version = "1", features = ["derive"] }
anyhow = "1"
chrono = { version = "0.4", features = ["serde"] }
```

**Step 3: Write stub main.rs**

```rust
mod sessions;
mod summary;
mod tmux;
mod tui;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("summarize") => summary::run_hook().await,
        Some("install")   => install::run(),
        _                 => tui::run().await,
    }
}
```

**Step 4: Verify it compiles**

```bash
cargo build 2>&1 | head -20
```
Expected: compile errors about missing modules (that's fine, we'll add them next)

**Step 5: Commit**

```bash
git init
git add Cargo.toml src/main.rs
git commit -m "feat: scaffold cc-speedy project"
```

---

### Task 2: Session Parsing (`sessions.rs`)

**Files:**
- Create: `src/sessions.rs`
- Create: `tests/sessions_test.rs`
- Create: `tests/fixtures/sample.jsonl`

**Step 1: Create fixture jsonl**

```bash
mkdir -p tests/fixtures
```

Write `tests/fixtures/sample.jsonl`:
```json
{"type":"user","message":{"content":[{"type":"text","text":"fix the bug in auth"}]}}
{"type":"assistant","message":{"content":[{"type":"text","text":"Sure, let me look..."}]}}
{"type":"user","message":{"content":[{"type":"text","text":"great thanks"}]}}
{"type":"assistant","message":{"content":[{"type":"text","text":"Done! Fixed."}]}}
```

**Step 2: Write failing test**

Create `tests/sessions_test.rs`:
```rust
use cc_speedy::sessions::parse_messages;

#[test]
fn test_parse_messages_counts_correctly() {
    let path = std::path::Path::new("tests/fixtures/sample.jsonl");
    let msgs = parse_messages(path).unwrap();
    assert_eq!(msgs.len(), 4);
}

#[test]
fn test_parse_messages_extracts_first_user_text() {
    let path = std::path::Path::new("tests/fixtures/sample.jsonl");
    let msgs = parse_messages(path).unwrap();
    assert_eq!(msgs[0].text, "fix the bug in auth");
}
```

**Step 3: Run test to verify it fails**

```bash
cargo test test_parse_messages 2>&1 | tail -10
```
Expected: compile error — `parse_messages` not found

**Step 4: Implement `src/sessions.rs`**

```rust
use anyhow::Result;
use serde_json::Value;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Message {
    pub role: String,   // "user" or "assistant"
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct Session {
    pub session_id: String,
    pub project_name: String,
    pub project_path: String,
    pub modified: std::time::SystemTime,
    pub message_count: usize,
    pub first_user_msg: String,
    pub jsonl_path: String,
}

pub fn parse_messages(path: &Path) -> Result<Vec<Message>> {
    let content = std::fs::read_to_string(path)?;
    let mut msgs = Vec::new();
    for line in content.lines() {
        let Ok(v): Result<Value, _> = serde_json::from_str(line) else { continue };
        let role = v["type"].as_str().unwrap_or("").to_string();
        if role != "user" && role != "assistant" { continue; }
        let text = extract_text(&v["message"]["content"]);
        if text.is_empty() { continue; }
        msgs.push(Message { role, text });
    }
    Ok(msgs)
}

fn extract_text(content: &Value) -> String {
    if let Some(s) = content.as_str() {
        return s.to_string();
    }
    if let Some(arr) = content.as_array() {
        for item in arr {
            if item["type"] == "text" {
                if let Some(s) = item["text"].as_str() {
                    return s.to_string();
                }
            }
        }
    }
    String::new()
}

pub fn list_sessions() -> Result<Vec<Session>> {
    let claude_dir = dirs::home_dir()
        .unwrap()
        .join(".claude")
        .join("projects");

    let mut sessions = Vec::new();

    for proj_entry in std::fs::read_dir(&claude_dir)? {
        let proj_entry = proj_entry?;
        let proj_path = proj_entry.path();
        if !proj_path.is_dir() { continue; }

        for file_entry in std::fs::read_dir(&proj_path)? {
            let file_entry = file_entry?;
            let file_path = file_entry.path();
            if file_path.extension().and_then(|e| e.to_str()) != Some("jsonl") { continue; }

            let metadata = file_path.metadata()?;
            let modified = metadata.modified()?;

            let msgs = parse_messages(&file_path).unwrap_or_default();
            // Skip command-line only / empty sessions
            if msgs.len() < 4 { continue; }

            let first_user_msg = msgs.iter()
                .find(|m| m.role == "user")
                .map(|m| m.text.chars().take(80).collect())
                .unwrap_or_default();

            // Skip local-command-caveat-only sessions
            if first_user_msg.contains("local-command-caveat") && msgs.len() < 10 { continue; }

            let session_id = file_path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();

            // Derive human-readable project name from dir name
            let dir_name = proj_path.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let project_name = dir_name_to_path(&dir_name);
            let project_path_str = dir_name_to_abs_path(&dir_name);

            sessions.push(Session {
                session_id,
                project_name,
                project_path: project_path_str,
                modified,
                message_count: msgs.len(),
                first_user_msg,
                jsonl_path: file_path.to_string_lossy().to_string(),
            });
        }
    }

    sessions.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(sessions)
}

/// Convert dir name like "-home-weibin-repo-ai-foo" to "/home/weibin/repo/ai/foo"
fn dir_name_to_abs_path(dir_name: &str) -> String {
    "/".to_string() + &dir_name.replace('-', "/").trim_start_matches('/').to_string()
}

/// Get last 2 segments as display name, e.g. "ai/foo"
fn dir_name_to_path(dir_name: &str) -> String {
    let abs = dir_name_to_abs_path(dir_name);
    let parts: Vec<&str> = abs.trim_end_matches('/').split('/').collect();
    match parts.len() {
        0 | 1 => abs.clone(),
        n => parts[n-2..].join("/"),
    }
}
```

Add to `Cargo.toml` under `[lib]`:
```toml
[lib]
name = "cc_speedy"
path = "src/lib.rs"
```

Create `src/lib.rs`:
```rust
pub mod sessions;
pub mod summary;
pub mod tmux;
```

**Step 5: Run tests**

```bash
cargo test test_parse_messages 2>&1 | tail -10
```
Expected: 2 tests PASS

**Step 6: Commit**

```bash
git add src/sessions.rs src/lib.rs tests/ Cargo.toml
git commit -m "feat: session parsing from .jsonl files"
```

---

### Task 3: Summary Read/Write (`summary.rs`)

**Files:**
- Create: `src/summary.rs`
- Create: `tests/summary_test.rs`

**Step 1: Write failing test**

Create `tests/summary_test.rs`:
```rust
use cc_speedy::summary::{read_summary, write_summary};
use tempfile::TempDir;

#[test]
fn test_write_and_read_summary() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("abc123.md");
    write_summary(&path, "## What was done\n- Fixed bug").unwrap();
    let content = read_summary(&path).unwrap();
    assert!(content.contains("Fixed bug"));
}

#[test]
fn test_read_missing_summary_returns_none() {
    let path = std::path::PathBuf::from("/tmp/nonexistent_abc999.md");
    assert!(read_summary(&path).is_none());
}
```

Add to `Cargo.toml`:
```toml
[dev-dependencies]
tempfile = "3"
```

**Step 2: Run to verify fail**

```bash
cargo test test_write_and_read 2>&1 | tail -5
```

**Step 3: Implement `src/summary.rs`**

```rust
use anyhow::Result;
use dirs::home_dir;
use std::path::{Path, PathBuf};

pub fn summaries_dir() -> PathBuf {
    home_dir().unwrap().join(".claude").join("summaries")
}

pub fn summary_path(session_id: &str) -> PathBuf {
    summaries_dir().join(format!("{}.md", session_id))
}

pub fn read_summary(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

pub fn write_summary(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)?;
    Ok(())
}
```

**Step 4: Run tests**

```bash
cargo test test_write_and_read test_read_missing 2>&1 | tail -10
```
Expected: 2 PASS

**Step 5: Commit**

```bash
git add src/summary.rs tests/summary_test.rs Cargo.toml
git commit -m "feat: summary read/write"
```

---

### Task 4: Claude API Summary Generation

**Files:**
- Modify: `src/summary.rs`

**Step 1: Add generate_summary function**

Append to `src/summary.rs`:
```rust
use crate::sessions::Message;

pub async fn generate_summary(session_id: &str, messages: &[Message]) -> Result<String> {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .map_err(|_| anyhow::anyhow!("ANTHROPIC_API_KEY not set"))?;

    // Take last 50 messages, format as conversation snippet
    let snippet: String = messages.iter().rev().take(50).rev()
        .map(|m| format!("{}: {}", m.role, &m.text.chars().take(200).collect::<String>()))
        .collect::<Vec<_>>()
        .join("\n");

    let prompt = format!(
        "Summarize this Claude Code conversation in 3-5 bullet points. \
        Focus on: what was asked, what was done, files changed, final status.\n\
        Output markdown with these sections only:\n\
        ## What was done\n- bullet\n\n## Files changed\n- file (or \"none\")\n\n## Status\nCompleted/In progress\n\n\
        Conversation:\n{}", snippet
    );

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&serde_json::json!({
            "model": "claude-haiku-4-5-20251001",
            "max_tokens": 512,
            "messages": [{"role": "user", "content": prompt}]
        }))
        .send()
        .await?;

    let body: Value = resp.json().await?;
    let text = body["content"][0]["text"]
        .as_str()
        .unwrap_or("Summary unavailable")
        .to_string();

    Ok(text)
}
```

Add missing import at top of `src/summary.rs`:
```rust
use serde_json::Value;
```

**Step 2: Add `run_hook` function (called by `cc-speedy summarize`)**

Append to `src/summary.rs`:
```rust
pub async fn run_hook() -> Result<()> {
    // Claude Code sets these env vars in hook context
    let session_id = std::env::var("CLAUDE_SESSION_ID")
        .or_else(|_| std::env::var("SESSION_ID"))
        .unwrap_or_default();
    let project_path = std::env::var("CLAUDE_PROJECT_DIR")
        .or_else(|_| std::env::var("PROJECT_DIR"))
        .unwrap_or_default();

    if session_id.is_empty() {
        eprintln!("cc-speedy summarize: no SESSION_ID, skipping");
        return Ok(());
    }

    let out_path = summary_path(&session_id);
    if out_path.exists() { return Ok(()); }  // already summarized

    // Find the jsonl for this session
    let jsonl = find_jsonl(&session_id);
    if jsonl.is_none() {
        eprintln!("cc-speedy summarize: jsonl not found for {}", session_id);
        return Ok(());
    }

    let messages = crate::sessions::parse_messages(Path::new(&jsonl.unwrap()))?;
    let summary = generate_summary(&session_id, &messages).await?;
    write_summary(&out_path, &summary)?;
    Ok(())
}

fn find_jsonl(session_id: &str) -> Option<String> {
    let base = home_dir()?.join(".claude").join("projects");
    for proj in std::fs::read_dir(&base).ok()? {
        let proj = proj.ok()?;
        let candidate = proj.path().join(format!("{}.jsonl", session_id));
        if candidate.exists() {
            return Some(candidate.to_string_lossy().to_string());
        }
    }
    None
}
```

**Step 3: Verify compile**

```bash
cargo build 2>&1 | grep "^error" | head -10
```
Expected: no errors (warnings ok)

**Step 4: Commit**

```bash
git add src/summary.rs
git commit -m "feat: Claude API summary generation and SessionEnd hook handler"
```

---

### Task 5: Tmux Integration (`tmux.rs`)

**Files:**
- Create: `src/tmux.rs`
- Create: `tests/tmux_test.rs`

**Step 1: Write failing tests**

Create `tests/tmux_test.rs`:
```rust
use cc_speedy::tmux::session_name_from_path;

#[test]
fn test_session_name_from_path_two_segments() {
    let name = session_name_from_path("/home/weibin/repo/ai/zero-drift-chat");
    assert_eq!(name, "ai-zero-drift-chat");
}

#[test]
fn test_session_name_truncated_to_50_chars() {
    let long = "/a/b/c/d/e/f/g/h/i/j/k/l/m/n/this-is-a-very-long-project-name-that-exceeds-limits";
    let name = session_name_from_path(long);
    assert!(name.len() <= 50);
}
```

**Step 2: Run to verify fail**

```bash
cargo test test_session_name 2>&1 | tail -5
```

**Step 3: Implement `src/tmux.rs`**

```rust
use anyhow::Result;

/// Derive tmux session name: last 2 path segments joined with "-"
pub fn session_name_from_path(path: &str) -> String {
    let parts: Vec<&str> = path.trim_end_matches('/').split('/').collect();
    let name = match parts.len() {
        0 => "cc-speedy".to_string(),
        1 => parts[0].to_string(),
        n => parts[n-2..].join("-"),
    };
    // tmux session names max 50 chars, sanitize
    name.chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .take(50)
        .collect()
}

pub fn is_inside_tmux() -> bool {
    std::env::var("TMUX").is_ok()
}

pub fn session_exists(name: &str) -> bool {
    std::process::Command::new("tmux")
        .args(["has-session", "-t", name])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn resume_in_tmux(session_name: &str, project_path: &str, session_id: &str) -> Result<()> {
    let claude_cmd = format!("claude --resume {}", session_id);

    if is_inside_tmux() {
        if session_exists(session_name) {
            // Just switch to existing session
            std::process::Command::new("tmux")
                .args(["switch-client", "-t", session_name])
                .status()?;
        } else {
            // Create detached, then switch
            std::process::Command::new("tmux")
                .args(["new-session", "-d", "-s", session_name, "-c", project_path, &claude_cmd])
                .status()?;
            std::process::Command::new("tmux")
                .args(["switch-client", "-t", session_name])
                .status()?;
        }
    } else {
        // Not in tmux: create and attach
        std::process::Command::new("tmux")
            .args(["new-session", "-s", session_name, "-c", project_path, &claude_cmd])
            .status()?;
    }
    Ok(())
}
```

**Step 4: Run tests**

```bash
cargo test test_session_name 2>&1 | tail -10
```
Expected: 2 PASS

**Step 5: Commit**

```bash
git add src/tmux.rs tests/tmux_test.rs
git commit -m "feat: tmux session management"
```

---

### Task 6: TUI — List + Preview Pane (`tui.rs`)

**Files:**
- Create: `src/tui.rs`

**Step 1: Write `src/tui.rs`**

```rust
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Terminal,
};
use std::io::stdout;
use std::sync::{Arc, Mutex};
use crate::sessions::{list_sessions, Session};
use crate::summary::{read_summary, summary_path};

struct AppState {
    sessions: Vec<Session>,
    filtered: Vec<usize>,   // indices into sessions
    list_state: ListState,
    filter: String,
    filter_mode: bool,
    summaries: Arc<Mutex<std::collections::HashMap<String, String>>>,
}

impl AppState {
    fn new(sessions: Vec<Session>) -> Self {
        let n = sessions.len();
        let mut list_state = ListState::default();
        if n > 0 { list_state.select(Some(0)); }
        Self {
            filtered: (0..n).collect(),
            sessions,
            list_state,
            filter: String::new(),
            filter_mode: false,
            summaries: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }

    fn apply_filter(&mut self) {
        let q = self.filter.to_lowercase();
        self.filtered = self.sessions.iter().enumerate()
            .filter(|(_, s)| q.is_empty() || s.project_name.to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect();
        if !self.filtered.is_empty() {
            self.list_state.select(Some(0));
        }
    }

    fn selected_session(&self) -> Option<&Session> {
        let idx = self.list_state.selected()?;
        let raw = *self.filtered.get(idx)?;
        self.sessions.get(raw)
    }
}

pub async fn run() -> Result<()> {
    let sessions = list_sessions()?;

    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = AppState::new(sessions);

    // Pre-load existing summaries
    for session in &app.sessions {
        let path = summary_path(&session.session_id);
        if let Some(content) = read_summary(&path) {
            app.summaries.lock().unwrap().insert(session.session_id.clone(), content);
        }
    }

    loop {
        terminal.draw(|f| draw(f, &mut app))?;

        if event::poll(std::time::Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                match (key.modifiers, key.code) {
                    (_, KeyCode::Char('q')) if !app.filter_mode => break,
                    (KeyModifiers::CONTROL, KeyCode::Char('c')) => break,
                    (_, KeyCode::Char('/')) if !app.filter_mode => {
                        app.filter_mode = true;
                    }
                    (_, KeyCode::Esc) if app.filter_mode => {
                        app.filter_mode = false;
                        app.filter.clear();
                        app.apply_filter();
                    }
                    (_, KeyCode::Backspace) if app.filter_mode => {
                        app.filter.pop();
                        app.apply_filter();
                    }
                    (_, KeyCode::Char(c)) if app.filter_mode => {
                        app.filter.push(c);
                        app.apply_filter();
                    }
                    (_, KeyCode::Down) | (_, KeyCode::Char('j')) => {
                        let n = app.filtered.len();
                        if n > 0 {
                            let i = app.list_state.selected().unwrap_or(0);
                            app.list_state.select(Some((i + 1).min(n - 1)));
                        }
                    }
                    (_, KeyCode::Up) | (_, KeyCode::Char('k')) => {
                        let i = app.list_state.selected().unwrap_or(0);
                        app.list_state.select(Some(i.saturating_sub(1)));
                    }
                    (_, KeyCode::Char('r')) if !app.filter_mode => {
                        // Trigger re-generation for selected session
                        if let Some(s) = app.selected_session() {
                            let id = s.session_id.clone();
                            let jsonl = s.jsonl_path.clone();
                            let summaries = app.summaries.clone();
                            tokio::spawn(async move {
                                let path = std::path::Path::new(&jsonl);
                                if let Ok(msgs) = crate::sessions::parse_messages(path) {
                                    if let Ok(text) = crate::summary::generate_summary(&id, &msgs).await {
                                        let out = crate::summary::summary_path(&id);
                                        let _ = crate::summary::write_summary(&out, &text);
                                        summaries.lock().unwrap().insert(id, text);
                                    }
                                }
                            });
                        }
                    }
                    (_, KeyCode::Enter) if !app.filter_mode => {
                        if let Some(s) = app.selected_session() {
                            let name = crate::tmux::session_name_from_path(&s.project_path);
                            let path = s.project_path.clone();
                            let id = s.session_id.clone();
                            // Restore terminal before handing off to tmux
                            disable_raw_mode()?;
                            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                            crate::tmux::resume_in_tmux(&name, &path, &id)?;
                            return Ok(());
                        }
                    }
                    _ => {}
                }
            }
        }

        // Check for async-loaded summaries
        if let Some(s) = app.selected_session() {
            let id = s.session_id.clone();
            let has_summary = app.summaries.lock().unwrap().contains_key(&id);
            if !has_summary {
                let jsonl = s.jsonl_path.clone();
                let summaries = app.summaries.clone();
                tokio::spawn(async move {
                    let path = std::path::Path::new(&jsonl);
                    if let Ok(msgs) = crate::sessions::parse_messages(path) {
                        // Check again to avoid double-gen
                        let already = summaries.lock().unwrap().contains_key(&id);
                        if !already {
                            summaries.lock().unwrap().insert(id.clone(), "Generating...".to_string());
                            if let Ok(text) = crate::summary::generate_summary(&id, &msgs).await {
                                let out = crate::summary::summary_path(&id);
                                let _ = crate::summary::write_summary(&out, &text);
                                summaries.lock().unwrap().insert(id, text);
                            }
                        }
                    }
                });
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}

fn draw(f: &mut ratatui::Frame, app: &mut AppState) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    // Filter bar
    let filter_text = if app.filter_mode {
        format!("> {}|", app.filter)
    } else {
        format!("  {}  (press / to filter)", app.filter)
    };
    let filter_block = Paragraph::new(filter_text)
        .block(Block::default().borders(Borders::ALL).title(" cc-speedy "));
    f.render_widget(filter_block, chunks[0]);

    // Main panes
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(chunks[1]);

    draw_list(f, app, panes[0]);
    draw_preview(f, app, panes[1]);

    // Status bar
    let status = Paragraph::new(" Enter: resume tmux session  j/k: navigate  /: filter  r: regenerate  q: quit")
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(status, chunks[2]);
}

fn draw_list(f: &mut ratatui::Frame, app: &mut AppState, area: Rect) {
    let items: Vec<ListItem> = app.filtered.iter().map(|&i| {
        let s = &app.sessions[i];
        let dt = format_time(s.modified);
        let line = Line::from(vec![
            Span::styled(format!("{} ", dt), Style::default().fg(Color::DarkGray)),
            Span::raw(s.project_name.clone()),
        ]);
        ListItem::new(line)
    }).collect();

    let count = items.len();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL)
            .title(format!(" Sessions ({}) ", count)))
        .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
        .highlight_symbol("► ");

    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn draw_preview(f: &mut ratatui::Frame, app: &mut AppState, area: Rect) {
    let content = match app.selected_session() {
        None => "No session selected".to_string(),
        Some(s) => {
            let summary = app.summaries.lock().unwrap()
                .get(&s.session_id)
                .cloned()
                .unwrap_or_else(|| "[no summary yet — hover to generate]".to_string());
            format!(
                "PROJECT:  {}\nMSGS:     {}  |  {}\nSESSION:  {}...\n\n{}",
                s.project_path,
                s.message_count,
                format_time(s.modified),
                &s.session_id[..8.min(s.session_id.len())],
                summary
            )
        }
    };

    let preview = Paragraph::new(content)
        .block(Block::default().borders(Borders::ALL).title(" Summary "))
        .wrap(Wrap { trim: false });
    f.render_widget(preview, area);
}

fn format_time(t: std::time::SystemTime) -> String {
    let secs = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let dt = chrono::DateTime::from_timestamp(secs as i64, 0)
        .unwrap_or_default()
        .format("%Y-%m-%d %H:%M")
        .to_string();
    dt
}
```

**Step 2: Update `src/main.rs`**

```rust
mod sessions;
mod summary;
mod tmux;
mod tui;
mod install;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("summarize") => summary::run_hook().await,
        Some("install")   => install::run(),
        _                 => tui::run().await,
    }
}
```

**Step 3: Verify compile**

```bash
cargo build 2>&1 | grep "^error" | head -20
```
Expected: no errors

**Step 4: Commit**

```bash
git add src/tui.rs src/main.rs
git commit -m "feat: ratatui TUI with list, preview pane, filter, and async summary"
```

---

### Task 7: Install Subcommand (`install.rs`)

**Files:**
- Create: `src/install.rs`
- Create: `tests/install_test.rs`

**Step 1: Write failing test**

Create `tests/install_test.rs`:
```rust
use cc_speedy::install::build_hook_entry;

#[test]
fn test_hook_entry_contains_summarize() {
    let entry = build_hook_entry("/usr/local/bin/cc-speedy");
    let s = serde_json::to_string(&entry).unwrap();
    assert!(s.contains("summarize"));
}
```

Add to `src/lib.rs`:
```rust
pub mod install;
```

**Step 2: Run to verify fail**

```bash
cargo test test_hook_entry 2>&1 | tail -5
```

**Step 3: Implement `src/install.rs`**

```rust
use anyhow::Result;
use dirs::home_dir;
use serde_json::{json, Value};

pub fn build_hook_entry(binary_path: &str) -> Value {
    json!({
        "hooks": [{
            "type": "command",
            "command": format!("{} summarize", binary_path)
        }]
    })
}

pub fn run() -> Result<()> {
    let settings_path = home_dir()
        .unwrap()
        .join(".claude")
        .join("settings.json");

    let content = std::fs::read_to_string(&settings_path)
        .unwrap_or_else(|_| "{}".to_string());
    let mut settings: Value = serde_json::from_str(&content)?;

    // Find binary path
    let binary = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "cc-speedy".to_string());

    let entry = build_hook_entry(&binary);
    let hook_cmd = entry["hooks"][0]["command"].as_str().unwrap().to_string();

    // Check if already installed
    if let Some(existing) = settings["hooks"]["SessionEnd"].as_array() {
        for e in existing {
            if let Some(hooks) = e["hooks"].as_array() {
                for h in hooks {
                    if h["command"].as_str() == Some(&hook_cmd) {
                        println!("cc-speedy: SessionEnd hook already installed.");
                        return Ok(());
                    }
                }
            }
        }
    }

    // Append hook
    let hooks = settings["hooks"]["SessionEnd"]
        .as_array_mut()
        .map(|a| { a.push(entry.clone()); })
        .unwrap_or_else(|| {
            settings["hooks"]["SessionEnd"] = json!([entry]);
        });

    let _ = hooks; // satisfy unused warning

    let pretty = serde_json::to_string_pretty(&settings)?;
    std::fs::write(&settings_path, pretty)?;
    println!("cc-speedy: SessionEnd hook installed in {:?}", settings_path);
    Ok(())
}
```

**Step 4: Run tests**

```bash
cargo test test_hook_entry 2>&1 | tail -10
```
Expected: PASS

**Step 5: Verify full build**

```bash
cargo build --release 2>&1 | grep "^error"
```
Expected: no output (no errors)

**Step 6: Commit**

```bash
git add src/install.rs src/lib.rs tests/install_test.rs
git commit -m "feat: install subcommand patches ~/.claude/settings.json"
```

---

### Task 8: End-to-End Smoke Test

**Step 1: Run binary against real data**

```bash
cargo run -- 2>&1 | head -5
```
Expected: TUI launches, shows session list

**Step 2: Test summarize subcommand manually**

```bash
# Find a real session ID
ls ~/.claude/projects/-home-weibin-repo/ | head -3

# Set env vars and run
SESSION_ID=<id-from-above> CLAUDE_SESSION_ID=<id> cargo run -- summarize
```
Expected: writes `~/.claude/summaries/<id>.md`

**Step 3: Verify summary file**

```bash
cat ~/.claude/summaries/<id>.md
```
Expected: markdown with ## What was done, ## Files changed, ## Status

**Step 4: Test install (dry run - inspect diff first)**

```bash
# Backup settings first
cp ~/.claude/settings.json ~/.claude/settings.json.bak
cargo run -- install
diff ~/.claude/settings.json.bak ~/.claude/settings.json
```
Expected: diff shows new SessionEnd entry added

**Step 5: Final commit**

```bash
git add -A
git commit -m "feat: cc-speedy v0.1.0 complete"
```

---

### Task 9: README

**Files:**
- Create: `README.md`

**Step 1: Write README.md**

```markdown
# cc-speedy

Terminal TUI to browse and resume Claude Code sessions with tmux.

## Install

```bash
cargo install --path .
cc-speedy install   # registers SessionEnd hook in ~/.claude/settings.json
```

## Usage

```bash
cc-speedy           # open TUI
```

### Key bindings

| Key | Action |
|-----|--------|
| j/k or arrows | Navigate |
| / | Filter by project name |
| Enter | Resume session in tmux |
| r | Regenerate summary |
| q | Quit |

## How it works

- Sessions stored in `~/.claude/projects/**/*.jsonl`
- Summaries stored in `~/.claude/summaries/<session-id>.md`
- On Enter: opens/switches to a named tmux session, runs `claude --resume <id>`
- Summaries auto-generated via `SessionEnd` hook using `claude-haiku-4-5`

## Environment

Requires `ANTHROPIC_API_KEY` for summary generation.
```

**Step 2: Commit**

```bash
git add README.md
git commit -m "docs: README for cc-speedy"
```
