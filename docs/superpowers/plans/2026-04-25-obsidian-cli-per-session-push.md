# Obsidian CLI — Per-session push enrichment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Layer Obsidian-CLI-driven enrichments onto cc-speedy's existing `export_to_obsidian` pipeline — append today's daily note, write richer frontmatter, emit faceted tags — without changing the file-write authorship contract or breaking users without the CLI.

**Architecture:** New `src/obsidian_cli.rs` provides a small typed wrapper around the `obsidian` binary (3 fns: `is_available`, `vault_is_running`, `daily_append`). `src/obsidian.rs` keeps writing the whole session-note file but with enriched frontmatter and learning-count tags; after a successful write, it calls `daily_append` (gated by a new `obsidian_daily_push` setting). All CLI failures degrade to a stderr log + status flash and never block the file write or the `obsidian_synced` mark.

**Tech Stack:** Rust 2021, rusqlite (settings persistence), tokio (`spawn_blocking` for CLI calls — non-blocking from the auto-export path; sync façade from the `o`-key path), chrono (existing).

**Spec:** `docs/superpowers/specs/2026-04-25-obsidian-cli-per-session-push-design.md`

**Precondition:** The prior in-flight TUI work (F1 help popup, `o`-key Obsidian save, generation spinner, archived-list fix, footer version, `obsidian_synced` indicator) is expected to be committed before starting Task 1. Those changes already live in the working tree as uncommitted edits to `src/store.rs`, `src/tui.rs`, `src/theme.rs`. The plan below assumes they have landed; Task 1 picks up from a clean tree on top of master + that batch.

---

## File Map

| File | Change |
|------|--------|
| `src/settings.rs` | Two new fields on `AppSettings`; `save_obsidian_vault_name`, `save_obsidian_daily_push` helpers |
| `src/store.rs` | `get_setting_bool`, `set_setting_bool` thin wrappers (string-encoded `"1"`/`"0"`) |
| `src/obsidian_cli.rs` | **New.** `Error` enum, `is_available`, `vault_is_running`, `daily_append`, internal `escape_arg_value` helper |
| `src/obsidian.rs` | Refactor frontmatter writer to use a `build_frontmatter()` helper; add `parse_status_from_factual`; emit new fields and tag families; call `daily_append` after successful write |
| `src/tui.rs` | Settings panel: add field-cycle state, render 3 rows (path / vault name / push toggle), Tab cycles, Enter edits text or toggles bool |
| `src/lib.rs` | `pub mod obsidian_cli;` |
| `tests/obsidian_test.rs` | Extend with frontmatter + tag assertions; status-parse tests |
| `tests/obsidian_cli_test.rs` | **New.** Argument-escape unit tests; daily-line format tests |
| `tests/settings_test.rs` | **New.** Settings round-trip tests for the two new fields |
| `docs/obsidian-setup.md` | **New.** Install + registration guide for users without the CLI |

---

## Task 1: Boolean settings helpers in `store.rs`

**Files:**
- Modify: `src/store.rs` (append)
- Test: `tests/settings_test.rs` (new)

**Goal:** Provide ergonomic `bool` accessors over the existing string-keyed `settings` table so later tasks can read/write `obsidian_daily_push` without ad-hoc parsing.

- [ ] **Step 1: Write the failing test**

Create `tests/settings_test.rs`:

```rust
use cc_speedy::store::{get_setting_bool, set_setting_bool, open_db};
use tempfile::TempDir;

fn open_temp_db() -> (TempDir, rusqlite::Connection) {
    let tmp = TempDir::new().unwrap();
    std::env::set_var("XDG_DATA_HOME", tmp.path());
    let conn = open_db().unwrap();
    (tmp, conn)
}

#[test]
fn test_bool_setting_default_when_missing() {
    let (_tmp, conn) = open_temp_db();
    assert_eq!(get_setting_bool(&conn, "missing_key", true), true);
    assert_eq!(get_setting_bool(&conn, "missing_key", false), false);
}

#[test]
fn test_bool_setting_round_trip_true() {
    let (_tmp, conn) = open_temp_db();
    set_setting_bool(&conn, "x", true).unwrap();
    assert_eq!(get_setting_bool(&conn, "x", false), true);
}

#[test]
fn test_bool_setting_round_trip_false() {
    let (_tmp, conn) = open_temp_db();
    set_setting_bool(&conn, "x", false).unwrap();
    assert_eq!(get_setting_bool(&conn, "x", true), false);
}

#[test]
fn test_bool_setting_overwrites_prior() {
    let (_tmp, conn) = open_temp_db();
    set_setting_bool(&conn, "x", true).unwrap();
    set_setting_bool(&conn, "x", false).unwrap();
    assert_eq!(get_setting_bool(&conn, "x", true), false);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test settings_test`
Expected: compile error — `get_setting_bool` and `set_setting_bool` not found.

- [ ] **Step 3: Implement the helpers**

Append to `src/store.rs` (after `set_setting`):

```rust
/// Read a setting as bool. Encoded as "1" / "0". Anything else → `default`.
pub fn get_setting_bool(conn: &Connection, key: &str, default: bool) -> bool {
    match get_setting(conn, key).as_deref() {
        Some("1") => true,
        Some("0") => false,
        _ => default,
    }
}

/// Persist a bool setting.
pub fn set_setting_bool(conn: &Connection, key: &str, value: bool) -> Result<()> {
    set_setting(conn, key, if value { "1" } else { "0" })
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test settings_test`
Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
git add src/store.rs tests/settings_test.rs
git commit -m "feat(store): add bool setting helpers"
```

---

## Task 2: Extend `AppSettings` with vault-name and daily-push fields

**Files:**
- Modify: `src/settings.rs`
- Modify (test): `tests/settings_test.rs`

**Goal:** Surface the two new settings on the existing `AppSettings` struct with sane defaults so later UI tasks have a single load/save surface.

- [ ] **Step 1: Write the failing test**

Append to `tests/settings_test.rs`:

```rust
use cc_speedy::settings::{load, save_obsidian_daily_push, save_obsidian_vault_name};

#[test]
fn test_load_defaults_when_unset() {
    let (_tmp, conn) = open_temp_db();
    let s = load(&conn);
    assert_eq!(s.obsidian_kb_path, None);
    assert_eq!(s.obsidian_vault_name, None);
    assert_eq!(s.obsidian_daily_push, true, "daily push default = true");
}

#[test]
fn test_save_and_load_vault_name() {
    let (_tmp, conn) = open_temp_db();
    save_obsidian_vault_name(&conn, "my-vault").unwrap();
    let s = load(&conn);
    assert_eq!(s.obsidian_vault_name.as_deref(), Some("my-vault"));
}

#[test]
fn test_save_and_load_daily_push_off() {
    let (_tmp, conn) = open_temp_db();
    save_obsidian_daily_push(&conn, false).unwrap();
    let s = load(&conn);
    assert_eq!(s.obsidian_daily_push, false);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test settings_test`
Expected: compile error — fields and helpers not found.

- [ ] **Step 3: Update `AppSettings` and add helpers**

Replace contents of `src/settings.rs`:

```rust
use anyhow::Result;
use rusqlite::Connection;

#[derive(Debug, Clone)]
pub struct AppSettings {
    pub obsidian_kb_path:    Option<String>,
    pub obsidian_vault_name: Option<String>,
    pub obsidian_daily_push: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            obsidian_kb_path:    None,
            obsidian_vault_name: None,
            obsidian_daily_push: true,
        }
    }
}

/// Load all settings from DB into AppSettings.
pub fn load(conn: &Connection) -> AppSettings {
    AppSettings {
        obsidian_kb_path:    crate::store::get_setting(conn, "obsidian_kb_path"),
        obsidian_vault_name: crate::store::get_setting(conn, "obsidian_vault_name"),
        obsidian_daily_push: crate::store::get_setting_bool(conn, "obsidian_daily_push", true),
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

/// Persist the Obsidian vault name (used as the `vault=<name>` argument to the CLI).
/// Trims whitespace; empty string clears it.
pub fn save_obsidian_vault_name(conn: &Connection, name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        crate::store::set_setting(conn, "obsidian_vault_name", "")?;
    } else {
        crate::store::set_setting(conn, "obsidian_vault_name", name)?;
    }
    Ok(())
}

/// Persist the "push to today's daily note" toggle.
pub fn save_obsidian_daily_push(conn: &Connection, value: bool) -> Result<()> {
    crate::store::set_setting_bool(conn, "obsidian_daily_push", value)
}

/// Vault name to use for CLI calls. Returns the configured value if non-empty,
/// otherwise the basename of `obsidian_kb_path`, otherwise `None`.
pub fn effective_vault_name(s: &AppSettings) -> Option<String> {
    if let Some(n) = s.obsidian_vault_name.as_deref() {
        if !n.is_empty() { return Some(n.to_string()); }
    }
    s.obsidian_kb_path.as_deref().and_then(|p| {
        std::path::Path::new(p)
            .file_name()
            .and_then(|x| x.to_str())
            .map(|x| x.to_string())
    })
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test settings_test`
Expected: all settings tests pass.

- [ ] **Step 5: Verify the rest of the workspace still compiles**

Run: `cargo build`
Expected: clean build (`tui.rs` already constructs `AppSettings` via `settings::load(...)` so the extra fields propagate without changes).

- [ ] **Step 6: Commit**

```bash
git add src/settings.rs tests/settings_test.rs
git commit -m "feat(settings): add obsidian_vault_name and obsidian_daily_push"
```

---

## Task 3: `obsidian_cli` skeleton — Error enum, escape helper, `is_available`

**Files:**
- Create: `src/obsidian_cli.rs`
- Modify: `src/lib.rs` (add `pub mod obsidian_cli;`)
- Test: `tests/obsidian_cli_test.rs` (new)

**Goal:** Land the wrapper module's surface and pure helpers first; later tasks add the I/O-touching functions.

- [ ] **Step 1: Write the failing test**

Create `tests/obsidian_cli_test.rs`:

```rust
use cc_speedy::obsidian_cli::escape_arg_value;

#[test]
fn test_escape_plain_text_unchanged() {
    assert_eq!(escape_arg_value("hello world"), "hello world");
}

#[test]
fn test_escape_double_quote() {
    assert_eq!(escape_arg_value(r#"say "hi""#), r#"say \"hi\""#);
}

#[test]
fn test_escape_newline() {
    assert_eq!(escape_arg_value("line1\nline2"), r"line1\nline2");
}

#[test]
fn test_escape_tab() {
    assert_eq!(escape_arg_value("a\tb"), r"a\tb");
}

#[test]
fn test_escape_backslash_first() {
    // backslash itself is doubled before the quote/newline rules apply
    assert_eq!(escape_arg_value(r"a\b"), r"a\\b");
}

#[test]
fn test_escape_combined() {
    assert_eq!(
        escape_arg_value("she said \"hi\"\nbye"),
        r#"she said \"hi\"\nbye"#,
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test obsidian_cli_test`
Expected: compile error — module doesn't exist.

- [ ] **Step 3: Create the module**

Create `src/obsidian_cli.rs`:

```rust
//! Thin wrapper around the official `obsidian` CLI bundled with Obsidian.app.
//!
//! All public functions either succeed or return a typed `Error` describing
//! one of three discrete failure modes. Callers map these to user-facing
//! strings or stderr logs as appropriate.

use std::process::Command;

#[derive(Debug)]
pub enum Error {
    /// The `obsidian` binary could not be invoked (not on PATH or not executable).
    CliMissing,
    /// The CLI is reachable but no Obsidian instance is running or the named vault
    /// is not open.
    NotRunning,
    /// The command itself returned a non-zero exit. The first line of stderr is
    /// captured for surfacing to the user.
    CommandFailed { stderr_first_line: String },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::CliMissing => write!(f, "Obsidian CLI not installed"),
            Error::NotRunning => write!(f, "Obsidian not running — open the vault first"),
            Error::CommandFailed { stderr_first_line } => {
                write!(f, "Obsidian: {}", stderr_first_line)
            }
        }
    }
}

impl std::error::Error for Error {}

/// Escape a value for use as the right-hand side of a `key=value` argument
/// passed to `obsidian`. Per the CLI's own escape rules: `\\` for backslash,
/// `\"` for double-quote, `\n` for newline, `\t` for tab. Backslash is escaped
/// first so the other replacements don't double-escape its expansions.
pub fn escape_arg_value(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str(r"\\"),
            '"'  => out.push_str(r#"\""#),
            '\n' => out.push_str(r"\n"),
            '\t' => out.push_str(r"\t"),
            _    => out.push(ch),
        }
    }
    out
}

/// Returns true iff the `obsidian` binary is on PATH and responds to `--help`.
pub fn is_available() -> bool {
    Command::new("obsidian")
        .arg("--help")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
```

Modify `src/lib.rs` — add the module declaration. (Existing file already has `pub mod obsidian; pub mod settings;` etc.; insert the new line alphabetically.)

```rust
pub mod obsidian_cli;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test obsidian_cli_test`
Expected: 6 passed.

- [ ] **Step 5: Verify whole workspace builds**

Run: `cargo build`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add src/obsidian_cli.rs src/lib.rs tests/obsidian_cli_test.rs
git commit -m "feat(obsidian-cli): add wrapper skeleton + escape_arg_value"
```

---

## Task 4: `vault_is_running` probe

**Files:**
- Modify: `src/obsidian_cli.rs`

**Goal:** Add a cheap probe that the auto-export and `o`-key paths use to short-circuit when Obsidian isn't running. No new unit test — `is_available` and `vault_is_running` both depend on the live CLI; we cover them in the integration test (Task 12).

- [ ] **Step 1: Add the function**

Append to `src/obsidian_cli.rs`:

```rust
/// Probe whether the named vault is currently open in a running Obsidian
/// instance. Returns false if the CLI is missing, the app isn't running,
/// the vault isn't open, or the eval otherwise fails.
pub fn vault_is_running(vault: &str) -> bool {
    let output = Command::new("obsidian")
        .arg(format!("vault={}", vault))
        .arg("eval")
        .arg(r#"code=app.vault.getName()"#)
        .output();
    match output {
        Ok(o) if o.status.success() => {
            // CLI prints "=> <value>" — accept anything non-empty.
            !String::from_utf8_lossy(&o.stdout).trim().is_empty()
        }
        _ => false,
    }
}
```

- [ ] **Step 2: Verify compiles**

Run: `cargo build`
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add src/obsidian_cli.rs
git commit -m "feat(obsidian-cli): add vault_is_running probe"
```

---

## Task 5: `daily_append` with idempotency

**Files:**
- Modify: `src/obsidian_cli.rs`
- Modify: `tests/obsidian_cli_test.rs`

**Goal:** Implement the function the rest of the system calls. Includes the eval-based dedupe check.

- [ ] **Step 1: Write the failing test for the eval-probe JS fragment**

Append to `tests/obsidian_cli_test.rs`:

```rust
use cc_speedy::obsidian_cli::build_dedupe_eval_code;

#[test]
fn test_dedupe_eval_contains_marker() {
    let js = build_dedupe_eval_code("[[2026-04-25-foo]]");
    assert!(js.contains("[[2026-04-25-foo]]"), "marker must appear: {}", js);
}

#[test]
fn test_dedupe_eval_escapes_quote_in_marker() {
    // markers shouldn't normally contain quotes, but if they do they must
    // not break out of the JS string literal
    let js = build_dedupe_eval_code(r#"contains "a quote""#);
    assert!(!js.contains(r#""contains "a quote""#),
        "raw quote must not appear unescaped: {}", js);
    assert!(js.contains(r#"contains \"a quote\""#),
        "expected escaped quotes: {}", js);
}

#[test]
fn test_dedupe_eval_uses_today_moment() {
    let js = build_dedupe_eval_code("anything");
    assert!(js.contains("moment()"), "should ask for today's daily note: {}", js);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test obsidian_cli_test`
Expected: compile error — `build_dedupe_eval_code` not defined.

- [ ] **Step 3: Implement `build_dedupe_eval_code` and `daily_append`**

Append to `src/obsidian_cli.rs`:

```rust
/// Build a JS expression suitable for `obsidian eval code=...` that returns
/// `true` if today's daily note exists AND already contains `marker`, else
/// `false`. Marker is escaped to remain inside the JS string literal.
pub fn build_dedupe_eval_code(marker: &str) -> String {
    // Escape backslash first, then double-quote, for embedding in JS string literal.
    let escaped: String = marker.chars().flat_map(|c| {
        match c {
            '\\' => vec!['\\', '\\'],
            '"'  => vec!['\\', '"'],
            '\n' => vec!['\\', 'n'],
            other => vec![other],
        }
    }).collect();
    format!(
        r#"(()=>{{const t=window.moment().format('YYYY-MM-DD');const f=app.vault.getMarkdownFiles().find(x=>x.basename===t);return !!(f && (await app.vault.read(f)).includes("{}"))}})()"#,
        escaped,
    )
}

/// Append a single line of content to today's daily note in `vault`. If
/// `dedupe_marker` is `Some(s)` and today's daily note already contains `s`,
/// the call is a no-op (idempotent). The CLI auto-creates today's daily note
/// if it doesn't yet exist.
pub fn daily_append(vault: &str, content: &str, dedupe_marker: Option<&str>) -> Result<(), Error> {
    if !is_available() {
        return Err(Error::CliMissing);
    }

    if let Some(marker) = dedupe_marker {
        let code = build_dedupe_eval_code(marker);
        let probe = Command::new("obsidian")
            .arg(format!("vault={}", vault))
            .arg("eval")
            .arg(format!("code={}", escape_arg_value(&code)))
            .output()
            .map_err(|_| Error::NotRunning)?;
        if !probe.status.success() {
            // Vault not open or eval failed — surface as NotRunning.
            return Err(Error::NotRunning);
        }
        let stdout = String::from_utf8_lossy(&probe.stdout);
        if stdout.contains("=> true") {
            return Ok(()); // already there; nothing to do
        }
    }

    let out = Command::new("obsidian")
        .arg(format!("vault={}", vault))
        .arg("daily:append")
        .arg(format!("content={}", escape_arg_value(content)))
        .output()
        .map_err(|_| Error::CliMissing)?;

    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let line = stderr.lines().next().unwrap_or("(no stderr)").to_string();
        Err(Error::CommandFailed { stderr_first_line: line })
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test obsidian_cli_test`
Expected: 9 passed total (6 from Task 3 + 3 new).

- [ ] **Step 5: Build everything**

Run: `cargo build`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add src/obsidian_cli.rs tests/obsidian_cli_test.rs
git commit -m "feat(obsidian-cli): add daily_append with eval-based idempotency"
```

---

## Task 6: Status parser for the factual summary

**Files:**
- Modify: `src/obsidian.rs`
- Modify: `tests/obsidian_test.rs`

**Goal:** Parse the `## Status` heading out of the factual summary so frontmatter can advertise it. Pure function — testable without I/O.

- [ ] **Step 1: Write the failing test**

Append to `tests/obsidian_test.rs`:

```rust
use cc_speedy::obsidian::parse_status_from_factual;

#[test]
fn test_parse_status_completed() {
    let body = "## What was done\n- x\n\n## Status\nCompleted\n\n## Approach\n";
    assert_eq!(parse_status_from_factual(body), "completed");
}

#[test]
fn test_parse_status_in_progress_two_words() {
    let body = "## Status\nIn progress\n";
    assert_eq!(parse_status_from_factual(body), "in_progress");
}

#[test]
fn test_parse_status_missing_returns_unknown() {
    let body = "## What was done\n- only this\n";
    assert_eq!(parse_status_from_factual(body), "unknown");
}

#[test]
fn test_parse_status_extra_whitespace() {
    let body = "## Status\n  Completed   \n";
    assert_eq!(parse_status_from_factual(body), "completed");
}

#[test]
fn test_parse_status_unrecognised_value() {
    let body = "## Status\nBlocked on infra\n";
    assert_eq!(parse_status_from_factual(body), "unknown");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test obsidian_test parse_status`
Expected: compile error — function not defined.

- [ ] **Step 3: Implement `parse_status_from_factual`**

Add to `src/obsidian.rs` (top of the file, after the `use` lines):

```rust
/// Parse the `## Status` line out of a factual summary body and normalise it.
/// Returns one of `"completed"`, `"in_progress"`, or `"unknown"`.
pub fn parse_status_from_factual(body: &str) -> &'static str {
    let mut lines = body.lines();
    while let Some(l) = lines.next() {
        if l.trim().eq_ignore_ascii_case("## Status") {
            // Read forward until the first non-empty line.
            for next in lines.by_ref() {
                let t = next.trim();
                if t.is_empty() { continue; }
                let lc = t.to_ascii_lowercase();
                return match lc.as_str() {
                    "completed"   => "completed",
                    "in progress" => "in_progress",
                    _             => "unknown",
                };
            }
        }
    }
    "unknown"
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test obsidian_test parse_status`
Expected: 5 passed.

- [ ] **Step 5: Commit**

```bash
git add src/obsidian.rs tests/obsidian_test.rs
git commit -m "feat(obsidian): add parse_status_from_factual"
```

---

## Task 7: Tag-list builder for frontmatter

**Files:**
- Modify: `src/obsidian.rs`
- Modify: `tests/obsidian_test.rs`

**Goal:** Build the deterministic ordered tag list from session metadata + learning counts. Pure function.

- [ ] **Step 1: Write the failing test**

Append to `tests/obsidian_test.rs`:

```rust
use cc_speedy::obsidian::build_frontmatter_tags;
use cc_speedy::store::LearningPoint;

fn lp(cat: &str) -> LearningPoint {
    LearningPoint { category: cat.to_string(), point: "x".to_string() }
}

#[test]
fn test_tags_baseline_no_learnings() {
    let tags = build_frontmatter_tags("cc", "completed", &[]);
    assert_eq!(tags, vec![
        "agent-session".to_string(),
        "cc-source/cc".to_string(),
        "cc-status/completed".to_string(),
    ]);
}

#[test]
fn test_tags_with_learning_counts_and_facets() {
    let learnings = vec![
        lp("decision_points"),
        lp("decision_points"),
        lp("lessons_gotchas"),
        lp("tools_commands"),
    ];
    let tags = build_frontmatter_tags("oc", "in_progress", &learnings);
    assert_eq!(tags, vec![
        "agent-session".to_string(),
        "cc-source/oc".to_string(),
        "cc-status/in_progress".to_string(),
        "cc-decisions/2".to_string(),
        "cc-lessons/1".to_string(),
        "cc-tools/1".to_string(),
        "cc-has-decisions".to_string(),
        "cc-has-lessons".to_string(),
        "cc-has-tools".to_string(),
    ]);
}

#[test]
fn test_tags_skip_zero_count_categories() {
    let learnings = vec![lp("lessons_gotchas")];
    let tags = build_frontmatter_tags("co", "unknown", &learnings);
    // Only the "lessons" family should appear.
    assert!(tags.contains(&"cc-lessons/1".to_string()));
    assert!(tags.contains(&"cc-has-lessons".to_string()));
    assert!(!tags.iter().any(|t| t.starts_with("cc-decisions/")));
    assert!(!tags.iter().any(|t| t.starts_with("cc-tools/")));
    assert!(!tags.contains(&"cc-has-decisions".to_string()));
    assert!(!tags.contains(&"cc-has-tools".to_string()));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test obsidian_test build_frontmatter_tags`
Expected: compile error.

- [ ] **Step 3: Implement `build_frontmatter_tags`**

Add to `src/obsidian.rs` (after `parse_status_from_factual`):

```rust
/// Build the ordered list of tags that go into the session note's frontmatter.
/// Order is deterministic so re-exports produce stable diffs.
///
/// `source` should be `"cc"`, `"oc"`, or `"co"`. `status` is the lower-snake form
/// from `parse_status_from_factual`.
pub fn build_frontmatter_tags(
    source: &str,
    status: &str,
    learnings: &[crate::store::LearningPoint],
) -> Vec<String> {
    let mut count_decisions = 0usize;
    let mut count_lessons   = 0usize;
    let mut count_tools     = 0usize;
    for l in learnings {
        match l.category.as_str() {
            "decision_points" => count_decisions += 1,
            "lessons_gotchas" => count_lessons   += 1,
            "tools_commands"  => count_tools     += 1,
            _ => {}
        }
    }

    let mut tags: Vec<String> = Vec::with_capacity(16);
    tags.push("agent-session".to_string());
    tags.push(format!("cc-source/{}", source));
    tags.push(format!("cc-status/{}", status));

    // Counted slash-tags first.
    if count_decisions > 0 { tags.push(format!("cc-decisions/{}", count_decisions)); }
    if count_lessons   > 0 { tags.push(format!("cc-lessons/{}",   count_lessons)); }
    if count_tools     > 0 { tags.push(format!("cc-tools/{}",     count_tools)); }

    // Bare facets second.
    if count_decisions > 0 { tags.push("cc-has-decisions".to_string()); }
    if count_lessons   > 0 { tags.push("cc-has-lessons".to_string()); }
    if count_tools     > 0 { tags.push("cc-has-tools".to_string()); }

    tags
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test obsidian_test build_frontmatter_tags`
Expected: 3 passed.

- [ ] **Step 5: Commit**

```bash
git add src/obsidian.rs tests/obsidian_test.rs
git commit -m "feat(obsidian): add build_frontmatter_tags"
```

---

## Task 8: Switch `export_to_obsidian` to enriched frontmatter

**Files:**
- Modify: `src/obsidian.rs`
- Modify: `tests/obsidian_test.rs`

**Goal:** Replace the inline frontmatter-string assembly with the enriched version using the new helpers. The existing tests in `tests/obsidian_test.rs` will need their assertions widened.

- [ ] **Step 1: Update existing tests for the new shape**

Edit `tests/obsidian_test.rs`. Replace `test_export_writes_markdown_file`:

```rust
#[test]
fn test_export_writes_markdown_file() {
    let tmp = TempDir::new().unwrap();
    let session = make_session(10);
    let learnings = vec![
        LearningPoint {
            category: "decision_points".to_string(),
            point: "used tokio::spawn".to_string(),
        },
        LearningPoint {
            category: "lessons_gotchas".to_string(),
            point: "watch lock order".to_string(),
        },
    ];
    export_to_obsidian(
        &session,
        "## What was done\n- fixed bug\n\n## Status\nCompleted\n",
        &learnings,
        tmp.path().to_str().unwrap(),
    )
    .unwrap();

    let files: Vec<_> = std::fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(files.len(), 1);
    let content = std::fs::read_to_string(files[0].path()).unwrap();
    // Original frontmatter fields still present.
    assert!(content.contains("session_id: \"abc12345-test\""));
    assert!(content.contains("project: \"/home/user/ai/cc-speedy\""));
    // New frontmatter fields.
    assert!(content.contains("project_name: \"cc-speedy\""), "missing project_name: {}", content);
    assert!(content.contains("source: \"cc\""));
    assert!(content.contains("status: \"completed\""));
    assert!(content.contains("message_count: 10"));
    assert!(content.contains("learnings_count: 2"));
    assert!(content.contains("git_branch: \"main\""));
    assert!(content.contains("last_exported:"));
    // Tags include new families.
    assert!(content.contains("cc-source/cc"));
    assert!(content.contains("cc-status/completed"));
    assert!(content.contains("cc-decisions/1"));
    assert!(content.contains("cc-lessons/1"));
    assert!(content.contains("cc-has-decisions"));
    // Body intact.
    assert!(content.contains("## What was done"));
    assert!(content.contains("## Decision points"));
    assert!(content.contains("used tokio::spawn"));
    assert!(content.contains("## Lessons & gotchas"));
    assert!(content.contains("watch lock order"));
}

#[test]
fn test_export_omits_empty_git_branch() {
    let tmp = TempDir::new().unwrap();
    let mut session = make_session(10);
    session.git_branch = String::new();
    export_to_obsidian(&session, "x", &[], tmp.path().to_str().unwrap()).unwrap();
    let files: Vec<_> = std::fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    let content = std::fs::read_to_string(files[0].path()).unwrap();
    assert!(!content.contains("git_branch:"), "should omit empty branch: {}", content);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test obsidian_test`
Expected: failures on new field assertions.

- [ ] **Step 3: Refactor `export_to_obsidian`**

Replace the body of `export_to_obsidian` in `src/obsidian.rs` (keep the function signature the same):

```rust
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
    let last_exported = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%z").to_string();

    let project_slug: String = crate::util::path_last_n(&session.project_path, 2)
        .replace('/', "-")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect();

    let id_prefix: String = session.session_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(8)
        .collect();

    let filename = format!("{}-{}-{}.md", date_str, project_slug, id_prefix);
    let file_path = std::path::Path::new(vault_path).join(&filename);

    let source_str = match session.source {
        crate::unified::SessionSource::ClaudeCode => "cc",
        crate::unified::SessionSource::OpenCode   => "oc",
        crate::unified::SessionSource::Copilot    => "co",
    };
    let status = parse_status_from_factual(factual);
    let project_name = crate::util::path_last_n(&session.project_path, 1);
    let tags = build_frontmatter_tags(source_str, status, learnings);

    let mut front = String::new();
    front.push_str("---\n");
    front.push_str(&format!("date: {}\n", date_str));
    front.push_str(&format!("project: \"{}\"\n", session.project_path.replace('"', "\\\"")));
    front.push_str(&format!("project_name: \"{}\"\n", project_name.replace('"', "\\\"")));
    front.push_str(&format!("session_id: \"{}\"\n", session.session_id.replace('"', "\\\"")));
    front.push_str(&format!("source: \"{}\"\n", source_str));
    front.push_str(&format!("status: \"{}\"\n", status));
    front.push_str(&format!("message_count: {}\n", session.message_count));
    front.push_str(&format!("learnings_count: {}\n", learnings.len()));
    if !session.git_branch.is_empty() {
        front.push_str(&format!("git_branch: \"{}\"\n", session.git_branch.replace('"', "\\\"")));
    }
    front.push_str(&format!("last_exported: {}\n", last_exported));
    front.push_str("tags: [");
    for (i, t) in tags.iter().enumerate() {
        if i > 0 { front.push_str(", "); }
        front.push_str(t);
    }
    front.push_str("]\n");
    front.push_str("---\n\n");

    let mut content = format!("{}{}", front, factual);

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

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test obsidian_test`
Expected: all obsidian tests pass (the existing `test_export_skips_sessions_with_few_messages`, `test_export_filename_format`, `test_export_overwrites_existing_file` remain green; the rewritten test passes; new `test_export_omits_empty_git_branch` passes).

- [ ] **Step 5: Commit**

```bash
git add src/obsidian.rs tests/obsidian_test.rs
git commit -m "feat(obsidian): enriched frontmatter (status, counts, source, branch, faceted tags)"
```

---

## Task 9: Daily-line builder + helper to derive note stem

**Files:**
- Modify: `src/obsidian.rs`
- Modify: `tests/obsidian_test.rs`

**Goal:** Pull the filename construction out into a reusable helper, plus a pure builder for the daily-note bullet line. Both testable.

- [ ] **Step 1: Write the failing test**

Append to `tests/obsidian_test.rs`:

```rust
use cc_speedy::obsidian::{build_daily_line, note_stem_for_session};

#[test]
fn test_note_stem_includes_date_slug_id() {
    let session = make_session(10);
    let stem = note_stem_for_session(&session, "2026-04-25");
    assert!(stem.starts_with("2026-04-25-"), "stem: {}", stem);
    assert!(stem.contains("ai-cc-speedy"), "stem: {}", stem);
    assert!(stem.ends_with("-abc12345"), "stem: {}", stem);
}

#[test]
fn test_daily_line_completed_status() {
    let session = make_session(10);
    let line = build_daily_line(
        &session,
        "2026-04-25-ai-cc-speedy-abc12345",
        "completed",
        "Fix the F1 popup clipping",
    );
    assert!(line.starts_with("- [[2026-04-25-ai-cc-speedy-abc12345]]"));
    assert!(line.contains("**cc-speedy**"));
    assert!(line.contains("10 msgs"));
    assert!(line.contains("✅"));
    assert!(line.contains("Fix the F1 popup clipping"));
    assert!(line.ends_with("#cc-session"));
}

#[test]
fn test_daily_line_in_progress_emoji() {
    let session = make_session(10);
    let line = build_daily_line(&session, "stem", "in_progress", "wip");
    assert!(line.contains("🔧"));
}

#[test]
fn test_daily_line_unknown_emoji() {
    let session = make_session(10);
    let line = build_daily_line(&session, "stem", "unknown", "x");
    assert!(line.contains("🚧"));
}

#[test]
fn test_daily_line_truncates_title_to_80_chars_unicode_safe() {
    let session = make_session(10);
    let long: String = "あ".repeat(120); // 120 multi-byte chars
    let line = build_daily_line(&session, "stem", "completed", &long);
    // The title chunk inside the line should be at most 80 chars from the long string.
    let count = line.matches("あ").count();
    assert!(count <= 80, "expected ≤80 occurrences, got {}", count);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test obsidian_test note_stem_for_session`
Expected: compile error — helpers not defined.

- [ ] **Step 3: Add the helpers**

Add to `src/obsidian.rs`:

```rust
/// Compute the filename stem (filename minus `.md`) for a session note. Uses
/// the same project-slug + id-prefix scheme as `export_to_obsidian` so the
/// daily-note wikilink resolves to the right file.
pub fn note_stem_for_session(session: &UnifiedSession, date_str: &str) -> String {
    let project_slug: String = crate::util::path_last_n(&session.project_path, 2)
        .replace('/', "-")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    let id_prefix: String = session.session_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(8)
        .collect();
    format!("{}-{}-{}", date_str, project_slug, id_prefix)
}

/// Build the bullet line that gets appended to today's daily note.
pub fn build_daily_line(
    session: &UnifiedSession,
    note_stem: &str,
    status: &str,
    factual_title: &str,
) -> String {
    let emoji = match status {
        "completed"   => "✅",
        "in_progress" => "🔧",
        _             => "🚧",
    };
    let title_truncated: String = factual_title.chars().take(80).collect();
    format!(
        "- [[{}]] **{}** · {} msgs · {} {} #cc-session",
        note_stem,
        session.project_name,
        session.message_count,
        emoji,
        title_truncated,
    )
}
```

Also refactor `export_to_obsidian` (Task 8) to use `note_stem_for_session` instead of duplicating the filename logic. Replace:

```rust
    let project_slug: String = ...
    let id_prefix: String = ...
    let filename = format!("{}-{}-{}.md", date_str, project_slug, id_prefix);
```

with:

```rust
    let stem = note_stem_for_session(session, &date_str);
    let filename = format!("{}.md", stem);
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test obsidian_test`
Expected: all obsidian tests pass (5 from earlier tasks + 5 new = 10 total in this file, modulo whichever already existed).

- [ ] **Step 5: Commit**

```bash
git add src/obsidian.rs tests/obsidian_test.rs
git commit -m "feat(obsidian): add note_stem_for_session + build_daily_line helpers"
```

---

## Task 10: Wire daily push into the call sites (not into `export_to_obsidian`)

**Files:**
- Modify: `src/obsidian.rs` (add `extract_factual_title` helper only — keep `export_to_obsidian` signature unchanged)
- Modify: `src/tui.rs` (call `daily_append` directly from each call site after a successful export)
- Modify: `tests/obsidian_test.rs`

**Goal:** After a successful file write, push the daily-note line. Each call site owns its error-handling flavour:

- `save_selected_to_obsidian` (`o`-key): file write OK + daily push OK → "Saved to Obsidian"; file write OK + daily push fail → "Saved (daily push: <reason>)"; file write fail → "Obsidian save failed: …" (existing). Matches spec A.7.
- `spawn_summary_generation` (auto-export from Ctrl+R): file write OK → mark synced; daily push runs but errors only `eprintln!`. Matches spec A.7.

This keeps `export_to_obsidian`'s signature unchanged — no parameter pollution.

- [ ] **Step 1: Add `extract_factual_title` helper to `src/obsidian.rs`**

Insert above `export_to_obsidian`:

```rust
/// Pull the first non-empty bullet under `## What was done` for use as the
/// daily-note line title. Empty string if not found.
pub fn extract_factual_title(factual: &str) -> String {
    let mut lines = factual.lines();
    while let Some(l) = lines.next() {
        if l.trim().eq_ignore_ascii_case("## What was done") {
            for next in lines.by_ref() {
                let t = next.trim();
                if t.is_empty() { continue; }
                if let Some(rest) = t.strip_prefix("- ") {
                    return rest.to_string();
                }
                // First non-empty non-bullet line — use it as-is.
                return t.to_string();
            }
        }
    }
    String::new()
}
```

- [ ] **Step 2: Add a unit test for it**

Append to `tests/obsidian_test.rs`:

```rust
use cc_speedy::obsidian::extract_factual_title;

#[test]
fn test_extract_title_returns_first_bullet() {
    let body = "## What was done\n- fixed bug\n- did other thing\n";
    assert_eq!(extract_factual_title(body), "fixed bug");
}

#[test]
fn test_extract_title_skips_blank_lines() {
    let body = "## What was done\n\n\n- delayed bullet\n";
    assert_eq!(extract_factual_title(body), "delayed bullet");
}

#[test]
fn test_extract_title_missing_section() {
    let body = "## Status\nCompleted\n";
    assert_eq!(extract_factual_title(body), "");
}
```

- [ ] **Step 3: Run the helper tests**

Run: `cargo test --test obsidian_test extract_title`
Expected: 3 passed.

- [ ] **Step 4: Add a private daily-push helper in `src/tui.rs`**

Insert near `save_selected_to_obsidian`. Returns `Ok(())` on push success, `Err(String)` carrying a user-facing reason on push failure. Pure orchestration — no UI side-effects.

```rust
/// Compute the daily-note line for a session and push it to today's daily
/// note. Idempotent via the wikilink marker. Caller decides whether to
/// surface the result. Returns `Ok(())` if the push succeeded OR was skipped
/// (push disabled / no vault name); returns `Err(reason)` only on actual CLI
/// failure when push was attempted.
fn push_session_to_daily(
    session: &crate::unified::UnifiedSession,
    factual: &str,
    settings: &crate::settings::AppSettings,
) -> Result<(), String> {
    if !settings.obsidian_daily_push {
        return Ok(());
    }
    let Some(vault) = crate::settings::effective_vault_name(settings) else {
        return Ok(()); // no vault configured — nothing to push to
    };

    let date_str = chrono::Local::now().format("%Y-%m-%d").to_string();
    let stem = crate::obsidian::note_stem_for_session(session, &date_str);
    let line = crate::obsidian::build_daily_line(
        session,
        &stem,
        crate::obsidian::parse_status_from_factual(factual),
        &crate::obsidian::extract_factual_title(factual),
    );
    let marker = format!("[[{}]]", stem);

    crate::obsidian_cli::daily_append(&vault, &line, Some(&marker))
        .map_err(|e| e.to_string())
}
```

- [ ] **Step 5: Wire the helper into `save_selected_to_obsidian`**

Locate the existing `match crate::obsidian::export_to_obsidian(&session, &factual, &learnings, &vault_path)` block (we already changed the function to take a mutable AppState earlier). Replace its `Ok(()) => { ... "Saved to Obsidian".to_string() }` arm with:

```rust
        Ok(()) => {
            let conn = app.db.lock().unwrap_or_else(|e| e.into_inner());
            let _ = crate::store::mark_obsidian_synced(&conn, &session.session_id);
            drop(conn);
            app.obsidian_synced.insert(session.session_id.clone());

            match push_session_to_daily(&session, &factual, &app.settings) {
                Ok(())   => "Saved to Obsidian".to_string(),
                Err(why) => format!("Saved (daily push: {})", why),
            }
        }
```

(The `Err` branch of the export match is unchanged.)

- [ ] **Step 6: Wire the helper into `spawn_summary_generation`**

Inside the spawned tokio task, find the block starting with `if let Some(ref vault_path) = obsidian_path {` and replace it with:

```rust
                    if let Some(ref vault_path) = obsidian_path {
                        let exported = crate::obsidian::export_to_obsidian(
                            &session, &factual, &all_learnings, vault_path,
                        );
                        if exported.is_ok() {
                            let _ = crate::store::mark_obsidian_synced(
                                &db.lock().unwrap_or_else(|e| e.into_inner()),
                                &id,
                            );
                            // Snapshot settings now (we're outside the TUI, so we can't
                            // borrow AppState). Read fresh from the DB.
                            let settings_snapshot = {
                                let conn = db.lock().unwrap_or_else(|e| e.into_inner());
                                crate::settings::load(&conn)
                            };
                            if let Err(e) = push_session_to_daily(&session, &factual, &settings_snapshot) {
                                eprintln!("cc-speedy: daily push failed: {}", e);
                            }
                        }
                    }
```

This requires `push_session_to_daily` to be reachable from `spawn_summary_generation`. Since both live in `src/tui.rs`, no `pub` change is needed — just keep the helper private to the module.

- [ ] **Step 7: Run the full test suite**

Run: `cargo test`
Expected: all tests pass. No changes to existing `tests/obsidian_test.rs` calls of `export_to_obsidian` — its signature is unchanged.

- [ ] **Step 8: Build**

Run: `cargo build`
Expected: clean.

- [ ] **Step 9: Commit**

```bash
git add src/obsidian.rs src/tui.rs tests/obsidian_test.rs
git commit -m "feat(obsidian): push session to today's daily note after each export"
```

---

## Task 11: Settings panel UI — vault name + push toggle rows

**Files:**
- Modify: `src/tui.rs`

**Goal:** Let users edit `obsidian_vault_name` and toggle `obsidian_daily_push` from the existing settings popup. Three-field cycle with Tab.

- [ ] **Step 1: Add a focused-field enum**

Just above `AppMode`:

```rust
#[derive(PartialEq, Copy, Clone)]
enum SettingsField { Path, VaultName, DailyPush }
```

- [ ] **Step 2: Extend `AppState`**

In the `AppState` struct, alongside `settings_editing`, `settings_input`, `settings_error`, add:

```rust
    settings_field: SettingsField,
```

Initialise in `new()`:

```rust
    settings_field: SettingsField::Path,
```

- [ ] **Step 3: Update the `s` keypress to seed input from the focused field**

Replace the current `(AppMode::Normal, KeyModifiers::NONE, KeyCode::Char('s')) => { ... }` arm body with:

```rust
    app.settings_field = SettingsField::Path;
    app.settings_input = app.settings.obsidian_kb_path.clone().unwrap_or_default();
    app.settings_error = None;
    app.settings_editing = false;
    app.mode = AppMode::Settings;
```

(Same as today, just keyed off the new `settings_field`.)

- [ ] **Step 4: Add Tab cycle in Settings mode**

Add to the Settings-mode handlers:

```rust
    (AppMode::Settings, _, KeyCode::Tab) if !app.settings_editing => {
        app.settings_field = match app.settings_field {
            SettingsField::Path      => SettingsField::VaultName,
            SettingsField::VaultName => SettingsField::DailyPush,
            SettingsField::DailyPush => SettingsField::Path,
        };
        // Reseed input from the new focused field's stored value.
        app.settings_input = match app.settings_field {
            SettingsField::Path      => app.settings.obsidian_kb_path.clone().unwrap_or_default(),
            SettingsField::VaultName => app.settings.obsidian_vault_name.clone().unwrap_or_default(),
            SettingsField::DailyPush => String::new(), // boolean — no text input
        };
        app.settings_error = None;
    }
```

- [ ] **Step 5: Update the Enter handler to dispatch by focused field**

Replace the existing `(AppMode::Settings, _, KeyCode::Enter) => { ... }` arm:

```rust
    (AppMode::Settings, _, KeyCode::Enter) => {
        if !app.settings_editing && app.settings_field == SettingsField::DailyPush {
            // Boolean — Enter toggles directly, no edit mode.
            let new_val = !app.settings.obsidian_daily_push;
            let result = crate::settings::save_obsidian_daily_push(
                &app.db.lock().unwrap_or_else(|e| e.into_inner()),
                new_val,
            );
            match result {
                Ok(()) => {
                    app.settings.obsidian_daily_push = new_val;
                    app.status_msg = Some((
                        format!("Daily push: {}", if new_val { "on" } else { "off" }),
                        Instant::now(),
                    ));
                }
                Err(e) => app.settings_error = Some(e.to_string()),
            }
        } else if !app.settings_editing {
            app.settings_editing = true;
            app.settings_error = None;
        } else {
            // Save the focused text field.
            let value = app.settings_input.trim().to_string();
            let conn = app.db.lock().unwrap_or_else(|e| e.into_inner());
            let result = match app.settings_field {
                SettingsField::Path      => crate::settings::save_obsidian_path(&conn, &value),
                SettingsField::VaultName => crate::settings::save_obsidian_vault_name(&conn, &value),
                SettingsField::DailyPush => unreachable!(),
            };
            drop(conn);
            match result {
                Ok(()) => {
                    match app.settings_field {
                        SettingsField::Path      => app.settings.obsidian_kb_path    = if value.is_empty() { None } else { Some(value) },
                        SettingsField::VaultName => app.settings.obsidian_vault_name = if value.is_empty() { None } else { Some(value) },
                        SettingsField::DailyPush => unreachable!(),
                    }
                    app.settings_editing = false;
                    app.settings_error = None;
                    app.status_msg = Some(("Saved".to_string(), Instant::now()));
                }
                Err(e) => app.settings_error = Some(e.to_string()),
            }
        }
    }
```

- [ ] **Step 6: Update `draw_settings_popup`**

Replace the body of `draw_settings_popup`:

```rust
fn draw_settings_popup(f: &mut ratatui::Frame, app: &AppState, area: Rect) {
    let popup_area = centered_rect(70, 16, area);
    f.render_widget(Clear, popup_area);

    let path_val   = app.settings.obsidian_kb_path.as_deref().unwrap_or("(not set)");
    let vault_val  = app.settings.obsidian_vault_name.as_deref().unwrap_or("(auto-derived from path)");
    let push_val   = if app.settings.obsidian_daily_push { "on" } else { "off" };

    let row = |focused: bool, label: &str, value: &str| -> Line<'_> {
        let marker = if focused { "▶ " } else { "  " };
        Line::from(vec![
            Span::raw(format!("{}{:<22}", marker, label)),
            Span::styled(value.to_string(), Style::default().fg(theme::FG)),
        ])
    };

    let path_line = if app.settings_editing && app.settings_field == SettingsField::Path {
        row(true, "Vault path:", &format!("{}|", app.settings_input))
    } else {
        row(app.settings_field == SettingsField::Path, "Vault path:", path_val)
    };
    let name_line = if app.settings_editing && app.settings_field == SettingsField::VaultName {
        row(true, "Vault name:", &format!("{}|", app.settings_input))
    } else {
        row(app.settings_field == SettingsField::VaultName, "Vault name:", vault_val)
    };
    let push_line = row(
        app.settings_field == SettingsField::DailyPush,
        "Push to daily note:",
        push_val,
    );

    let mut lines = vec![
        Line::from(""),
        path_line,
        name_line,
        push_line,
        Line::from(""),
    ];
    if let Some(ref err) = app.settings_error {
        lines.push(Line::from(Span::styled(
            format!("  ✗ {}", err),
            Style::default().fg(ratatui::style::Color::Red),
        )));
        lines.push(Line::from(""));
    }
    let hint = if app.settings_editing {
        "  [Enter] Save   [Esc] Cancel"
    } else {
        "  [Tab] Next field   [Enter] Edit / Toggle   [Esc] Close"
    };
    lines.push(Line::from(hint));

    let popup = Paragraph::new(lines)
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

- [ ] **Step 7: Build and smoke-test**

Run: `cargo build`
Expected: clean.

Run: `cargo run` (interactively):
- Press `s`. Confirm 3 rows visible, focus on Vault path.
- Press `Tab` twice. Focus moves to "Push to daily note: on".
- Press `Enter`. Status flashes "Daily push: off" and the row updates.
- Press `Tab` until back at Vault path, press `Enter`, type something, press `Enter`. Either accepted or shown as error.

- [ ] **Step 8: Commit**

```bash
git add src/tui.rs
git commit -m "feat(tui): settings panel with vault name + daily push fields"
```

---

## Task 12: Setup docs

**Files:**
- Create: `docs/obsidian-setup.md`

- [ ] **Step 1: Write the docs file**

Create `docs/obsidian-setup.md`:

```markdown
# Obsidian CLI setup

cc-speedy uses the official `obsidian` command-line tool that ships with the
Obsidian desktop app (Obsidian ≥ 1.10). This is **not** a separate download —
it's built into the installer and you toggle it on inside Obsidian.

## Enable the CLI

1. Open Obsidian on your platform.
2. Settings → General → **Command line interface** → toggle on.
3. Follow the on-screen registration prompt.

After registration:

| OS | Where the CLI lands |
|----|-----|
| macOS   | `/usr/local/bin/obsidian` (symlink) |
| Linux   | `~/.local/bin/obsidian` |
| Windows | `Obsidian.com` next to `Obsidian.exe`; the installer adds it to PATH |

Verify:

```sh
obsidian --help
obsidian eval code="app.vault.getName()"
```

The second command prints the active vault's name. If both work, cc-speedy is
ready to use the CLI features.

## WSL users

The Linux side of WSL doesn't see Windows-side PATH automatically. Drop a
small wrapper at `~/.local/bin/obsidian`:

```bash
#!/usr/bin/env bash
set -euo pipefail
CANDIDATES=(
  "/mnt/c/Users/$(whoami)/AppData/Local/Programs/Obsidian/Obsidian.com"
  "/mnt/c/Program Files/Obsidian/Obsidian.com"
)
for c in "${CANDIDATES[@]}"; do
  [[ -x "$c" ]] && exec "$c" "$@"
done
echo "obsidian: redirector not found — adjust CANDIDATES" >&2
exit 1
```

`chmod +x ~/.local/bin/obsidian` and you're done.

## Required: vault must be open

The CLI talks to a *running* Obsidian instance. Make sure the vault you've
configured in cc-speedy (`s` settings panel) is currently open in Obsidian
when cc-speedy tries to push.

## Required for daily-note features

cc-speedy uses `obsidian daily:append` to push session lines into today's
daily note. Out of the box this works against Obsidian's built-in Daily Notes
core plugin (enabled by default). If you've disabled Daily Notes, re-enable
it under Settings → Core plugins.

## Configuring cc-speedy

Press `s` in cc-speedy to open the Settings panel:

- **Vault path** — absolute path to the vault directory (existing setting).
- **Vault name** — the name Obsidian uses internally. If left blank, cc-speedy
  defaults to the basename of the vault path. If your vault directory and
  Obsidian-side vault name differ, set this explicitly.
- **Push to daily note** — toggle on/off. Off disables the daily-note push
  without affecting the per-session note export.

## Troubleshooting

| Status flash | Meaning |
|----|----|
| `Obsidian CLI not installed` | `obsidian --help` failed. Re-run the registration step in Settings. |
| `Obsidian not running — open the vault first` | The CLI is reachable but no instance is running, or the configured vault isn't open. |
| `Obsidian: <message>` | The CLI returned non-zero. The message is the first line of stderr. |

The per-session Markdown file is always written regardless of CLI status — the
CLI integrations are purely additive.
```

- [ ] **Step 2: Commit**

```bash
git add docs/obsidian-setup.md
git commit -m "docs: add Obsidian CLI setup guide"
```

---

## Task 13: Integration test (ignored by default)

**Files:**
- Create: `tests/obsidian_cli_integration_test.rs`

**Goal:** A live, opt-in test that talks to a real running Obsidian. Requires manual setup and is gated behind `#[ignore]` so CI never runs it.

- [ ] **Step 1: Write the integration test**

Create `tests/obsidian_cli_integration_test.rs`:

```rust
//! Live integration tests against a running Obsidian instance.
//!
//! Run manually with:
//!   OBSIDIAN_TEST_VAULT=my-vault cargo test --test obsidian_cli_integration_test -- --ignored
//!
//! Requirements:
//!  - Obsidian.app running with the named vault open.
//!  - Daily Notes core plugin enabled.
//!  - The `obsidian` CLI on PATH.

use cc_speedy::obsidian_cli::{is_available, vault_is_running, daily_append};

fn vault_name() -> Option<String> {
    std::env::var("OBSIDIAN_TEST_VAULT").ok()
}

#[test]
#[ignore]
fn live_is_available() {
    assert!(is_available(), "obsidian binary not on PATH");
}

#[test]
#[ignore]
fn live_vault_is_running() {
    let v = vault_name().expect("set OBSIDIAN_TEST_VAULT");
    assert!(vault_is_running(&v), "vault not open: {}", v);
}

#[test]
#[ignore]
fn live_daily_append_idempotent() {
    let v = vault_name().expect("set OBSIDIAN_TEST_VAULT");
    let marker = format!("[[cc-speedy-integration-test-{}]]", chrono::Local::now().timestamp());
    let line = format!("- {} test marker", marker);
    daily_append(&v, &line, Some(&marker)).unwrap();
    // Second call should be a no-op (marker now present).
    daily_append(&v, &line, Some(&marker)).unwrap();
}
```

- [ ] **Step 2: Verify the test compiles**

Run: `cargo test --test obsidian_cli_integration_test --no-run`
Expected: compiles. (Tests don't run because they're `#[ignore]`d; that's fine.)

- [ ] **Step 3: Optional manual run**

If Obsidian is currently running and the vault is open:

```sh
OBSIDIAN_TEST_VAULT=obsidian-vault cargo test --test obsidian_cli_integration_test -- --ignored
```

Expected: 3 passed (or skipped with a clear error if the env var is missing or Obsidian isn't running).

- [ ] **Step 4: Commit**

```bash
git add tests/obsidian_cli_integration_test.rs
git commit -m "test(obsidian-cli): add ignored live integration test"
```

---

## Final verification

- [ ] **Run the full suite:**

```sh
cargo build
cargo test
cargo clippy
```

Expected: clean build, all tests pass (no new warnings introduced beyond the pre-existing two `unused variable: e` in `opencode_sessions.rs`).

- [ ] **Reinstall and smoke-test:**

```sh
cargo install --path . --force
cc-speedy
```

In the TUI: open a session that already has a summary, press `o`. Confirm:
- The session note in the vault has the new frontmatter fields.
- Today's daily note (if Obsidian is running with the configured vault) has a new bullet.
- Pressing `o` again does not add a duplicate bullet.
- The `◆` glyph still appears in the row.
