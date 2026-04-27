# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Build
cargo build
cargo build --release

# Run
cargo run                  # open TUI
cargo run -- install       # install SessionEnd hook to ~/.claude/settings.json
cargo run -- summarize     # run hook manually (reads CLAUDE_SESSION_ID from env)
cargo run -- update        # download latest release from GitHub and replace the binary in place
cargo run -- --version

# Test
cargo test                                # all suites (~25 integration suites under tests/)
cargo test --test sessions_test           # one suite (file under tests/)
cargo test insights_test::tests::name     # single test by path

# Lint / check
cargo clippy
cargo fmt
```

## Architecture

**Entry point:** `src/main.rs` strips `ANTHROPIC_*` env vars at startup (so wrapper-shell proxies like openclaw don't leak into spawned `claude --print` / `tmux` / agent children) and dispatches to four modes: `tui::run()` (default), `summary::run_hook()`, `install::run()`, `update::run()`.

**Library surface:** all logic lives in `src/lib.rs` re-exports — `main.rs` is a thin shim. Each module is independently testable via the `tests/` integration suites.

### Session ingestion

- `sessions.rs` — Claude Code sessions from `~/.claude/projects/**/*.jsonl` and `sessions-index.json` (the index resolves project paths accurately, fixing hyphen-in-path ambiguity).
- `opencode_sessions.rs` — OpenCode sessions from `~/.local/share/opencode/`.
- `copilot_sessions.rs` — Copilot sessions from `~/.copilot/session-state/` (each session is a directory with `workspace.yaml` + `events.jsonl`; sessions with fewer than 4 messages are skipped).
- `unified.rs` — merges CC, OC, and Copilot into `Vec<UnifiedSession>`, sorted by recency.

### Persistence (`store.rs`)

SQLite at `~/.local/share/cc-speedy/data.db` (WAL mode). Tables, all created idempotently on launch:

| Table | Purpose |
|-------|---------|
| `summaries` | `(session_id, source, content, generated_at)` — factual session summaries |
| `pinned` | Sessions floated to the top of the list |
| `learnings` | Per-session decision points / lessons & gotchas / tools & commands extracted by `summary.rs` |
| `archived` | Sessions hidden from the main list (visible in the Archived tab) |
| `tags` | Free-form `(session_id, tag)` rows; tags are normalized to `[a-z0-9_-]` |
| `links` | `child_id → parent_id` chains for multi-period work |
| `obsidian_synced` | Tracks which sessions have been pushed to the Obsidian vault |
| `insights` | Cached `SessionInsights` blob keyed by `(session_id, source_mtime)` |
| `settings` | App key-value settings (Obsidian path, vault name, daily-push toggle) |

On first launch `migrate_from_files()` imports legacy `.md` summary files and `pinned.json`.

### Insights pipeline (`insights.rs` + `copilot_insights.rs` + `turn_detail.rs` + `copilot_turn_detail.rs`)

Two parallel implementations behind one shared data shape:
- `parse_insights(path) -> SessionInsights` — token totals, tool/skill histogram, sub-agent dispatches, error counts, plus a per-turn `TurnGlyph` timeline. CC parses JSONL `type:"assistant"` lines; Copilot parses `events.jsonl` (`assistant.message`, `tool.execution_complete`, …).
- `extract_turn(path, turn_idx) -> TurnDetail` — full content of one assistant turn (paired with its triggering user message and any `tool_result` bodies, capped at `RESULT_BYTE_CAP = 8KB` per result).

Insights are cached by source-file `mtime`; the TUI reuses the SQLite blob until the underlying log advances. The renderer is **source-agnostic** — both CC and Copilot produce the same `SessionInsights` / `TurnDetail`, so the panel and modal don't branch on source.

### Summary generation (`summary.rs`)

- Calls `claude --print <prompt>` (no separate API key — uses your Claude Code subscription).
- 180-second timeout (`tokio::time::timeout`) — manual Ctrl+R on large sessions can run close to this.
- `run_hook()` mode reads `CLAUDE_SESSION_ID` env var and skips if a summary already exists.
- Two-pass: factual summary + learning-point extraction. Learnings persist into the `learnings` table and are merged back into the displayed summary on render.
- `build_combined_display(factual, learnings)` is the single source of truth for how stored data becomes preview text.

### TUI (`tui.rs`)

Built with `ratatui` + `crossterm`. The `AppState` struct holds *everything* — sessions, filtered indices, all caches (`Arc<Mutex<…>>` for the async-shared ones), DB connection, settings.

**Modes** (`enum AppMode`):
`Normal`, `Filter`, `Grep`, `Rename`, `ActionMenu`, `Settings`, `Library`, `LibraryFilter`, `Projects`, `ProjectsFilter`, `TagEdit`, `LinkPicker`, `LinkPickerFilter`, `Digest`, `Help`, `TurnDetail`.

**Focus** (`enum Focus`): `ActiveList`, `ArchivedList`, `Preview`. The active vs archived split lets the user `a`-archive without losing access; the archived tab is a separate list.

**Source filter** cycles via `1` / `2` / `3` / `0` (CC / OC / Copilot / all); badges `[CC]`, `[OC]`, `[CO]` render in the list.

**Live git column** (`git_status.rs`): a startup batch populates every unique project path in parallel with a 500ms timeout each; selection-change refresh on a 30s stale window; `g` force-refreshes everything. Glyphs `●` (dirty), `○` (clean), `·` (no git), `◦` (timeout/error).

**Insights panel** is toggled with `i` and renders above the summary. The glyph timeline is navigable with `[` / `]` (prev/next turn) and `{` / `}` (first/last); pressing `Enter` while the cursor is active opens the `TurnDetail` modal for CC and Copilot sessions.

**Async invariants:** the TUI never blocks. Summary generation, insights parsing, git checks, and Obsidian writes all run in `tokio::spawn` / `spawn_blocking`. Concurrency is bounded via `Arc<Mutex<HashSet>>` "in-flight" guards (`generating`, `insights_loading`); excess requests are silently skipped, not queued.

### Tmux integration (`tmux.rs`)

- Session name conventions: `cc-<path>` (Claude Code), `oc-<path>` (OpenCode), `co-<path>` (Copilot); new sessions append a timestamp suffix to avoid collisions.
- `resume_in_tmux_with_cmd()` is the core helper — creates or attaches to a named tmux session, switches client if already inside tmux, otherwise attaches.
- Yolo mode: CC uses `--dangerously-skip-permissions`, Copilot uses `--allow-all`.
- Action menu's "new session with prior summary" path uses tmux **bracketed-paste** to inject the summary as a first-message context block.
- `pin_window_title()` locks the tmux window name and forwards it via OSC to the terminal (needed for WSL / Windows Terminal).

### Obsidian export (`obsidian.rs` + `obsidian_cli.rs`)

- `obsidian.rs` builds the per-session note: YAML frontmatter (tags include `source/cc|oc|co`, `status/<parsed>`, learning-count tags), a body that mirrors the in-app summary, plus `PARENT:` / `CHILDREN:` blocks for linked chains.
- `obsidian_cli.rs` shells out to the official `obsidian` CLI with three discrete error variants (`CliMissing`, `NotRunning`, `CommandFailed`).
- `settings.rs` owns the Obsidian config (`obsidian_kb_path`, `obsidian_vault_name`, `obsidian_daily_push`); the Settings panel (`s`) edits it.

### Theming (`theme.rs`)

btop-inspired palette, all colors as `Color::Rgb` constants. Style helpers (`sel_style()`, `dim_style()`, …) keep rendering code clean.

### Install (`install.rs`)

Idempotently appends a `SessionEnd` hook to `~/.claude/settings.json`. Binary path is quoted to handle spaces; `summarize` is the literal hook command.

### Self-update (`update.rs`)

`cc-speedy update` hits the GitHub Releases API, downloads the platform tarball into a tempdir, and atomically replaces the running binary. Compares `tag_name` against `CARGO_PKG_VERSION`; tags carry a `.runN` suffix so any tag mismatch counts as an update.

## Key Design Decisions

- **Single SQLite source of truth** (since v0.2.1). Summaries, learnings, pins, archives, tags, links, insights — all in one `data.db`. Migrations are additive and idempotent.
- **Insights renderer is source-agnostic.** CC and Copilot have separate parsers but write identical `SessionInsights` / `TurnDetail` shapes, so adding a third source means writing a parser, not touching UI.
- **The TUI never blocks.** Anything I/O-bound goes through `tokio::spawn`; in-flight sets prevent dupes; UI reads cached state every frame.
- **Pinned sessions float to the top** regardless of recency sort.
- **Session IDs come from CC's own JSONL filenames**; `sessions-index.json` resolves project paths.
- **`ANTHROPIC_*` env vars are stripped at startup** so wrapper proxies don't leak into the children we spawn.
