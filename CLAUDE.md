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
cargo run -- --version

# Test
cargo test                             # all tests
cargo test sessions                    # single test file (e.g. tests/sessions_test.rs)
cargo test install::tests::test_name   # single test by path

# Lint / check
cargo clippy
cargo fmt
```

## Architecture

**Entry point:** `src/main.rs` dispatches to three modes: `tui::run()` (default), `summary::run_hook()`, and `install::run()`.

**Session data flow:**
- `sessions.rs` — parses Claude Code sessions from `~/.claude/projects/**/*.jsonl` and `sessions-index.json`
- `opencode_sessions.rs` — parses OpenCode sessions from `~/.local/share/opencode/`
- `copilot_sessions.rs` — parses Copilot sessions from `~/.copilot/session-state/` (each session is a directory with `workspace.yaml` + `events.jsonl`; sessions with fewer than 4 messages are skipped)
- `unified.rs` — merges CC, OC, and Copilot sessions into `Vec<UnifiedSession>`, sorted by recency

**Persistence (`store.rs`):**
- SQLite database at `~/.local/share/cc-speedy/data.db`
- Two tables: `summaries` (session_id, source, content, generated_at) and `pinned` (session_id)
- On first run, `migrate_from_files()` imports legacy `.md` summary files and `pinned.json`

**TUI (`tui.rs`):**
- Built with `ratatui` + `crossterm`
- `AppState` holds all runtime state: sessions, filtered indices, pinned set, summary cache, DB connection
- Modes: `Normal`, `Filter`, `Rename`, `PinMenu`
- Focus toggles between `List` (left panel) and `Preview` (right panel)
- Source filter cycles: All → CC only → OC only → Copilot only (keys `1`/`2`/`3`); badges `[CC]`, `[OC]`, `[CO]` in the list
- Summary generation is async (up to 5 concurrent), tracked via `Arc<Mutex<HashSet>>` in `generating`

**Summary generation (`summary.rs`):**
- Calls `claude --print <prompt>` (uses existing Claude Code subscription, no separate API key)
- 60-second timeout enforced via `tokio::time::timeout`
- Hook mode (`run_hook`) reads `CLAUDE_SESSION_ID` env var, skips if already summarised
- Session IDs are sanitized before use as filenames (alphanumeric + `-_` only)

**Tmux integration (`tmux.rs`):**
- Session name conventions: `cc-<path>` (Claude Code), `oc-<path>` (OpenCode), `co-<path>` (Copilot); new sessions append a timestamp suffix to avoid collisions
- `resume_in_tmux_with_cmd()` is the core helper — creates or attaches to a named tmux session, switches client if already inside tmux, otherwise attaches
- Yolo mode: CC uses `--dangerously-skip-permissions`, Copilot uses `--allow-all`
- `pin_window_title()` locks the tmux window name and forwards it via OSC to the terminal (needed for WSL / Windows Terminal)

**Theming (`theme.rs`):**
- btop-inspired color palette, all colors defined as `Color::Rgb` constants
- Style helpers (`sel_style()`, `dim_style()`, etc.) keep rendering code clean

**Install (`install.rs`):**
- Idempotently appends a `SessionEnd` hook to `~/.claude/settings.json`
- Binary path is quoted to handle spaces; `summarize` subcommand is the literal hook command

## Key Design Decisions

- Summaries are stored in SQLite (not files) since v0.2.1 — the `store.rs` module is the single source of truth
- Pinned sessions float to the top of the list regardless of recency sort
- The TUI never blocks on summary generation — all async work runs in `tokio::spawn` tasks
- Session IDs come from Claude Code's own JSONL filenames; the `sessions-index.json` resolves project paths accurately (avoids hyphen-in-path ambiguity)
