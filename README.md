# cc-speedy

Terminal TUI to browse and resume **Claude Code**, **OpenCode**, and **Copilot CLI** sessions in named tmux windows — with AI-generated summaries, an insights timeline, a per-turn detail modal, a learning library, project dashboard, weekly digest, tags, session linking, live git status, and Obsidian export.

## Install

**One-liner (Linux / macOS — auto-detects platform):**
```bash
curl -fsSL https://raw.githubusercontent.com/wei6bin/cc-speedy/master/install.sh | bash
```

This downloads the right binary, puts it in `/usr/local/bin`, and registers the SessionEnd hook — all in one step.

To install to a custom directory:
```bash
BIN_DIR=~/.local/bin curl -fsSL https://raw.githubusercontent.com/wei6bin/cc-speedy/master/install.sh | bash
```

## Install (from source)

```bash
cargo install --path .
cc-speedy install
```

## Usage

```bash
cc-speedy           # open the session browser TUI
cc-speedy install   # register the SessionEnd hook in ~/.claude/settings.json
cc-speedy update    # download & replace the binary with the latest GitHub release
cc-speedy --version
```

## Key Bindings

### Navigation & resume

| Key | Action |
|-----|--------|
| `j` / `k` or arrows | Move selection (or scroll preview when focused) |
| `Tab` | Cycle focus: list ↔ archived ↔ preview |
| `Enter` | Resume session in tmux (or open turn detail when the insights cursor is active) |
| `Ctrl+Y` | Resume in **yolo** mode (`--dangerously-skip-permissions` for CC, `--allow-all` for Copilot) |
| `q` / `Ctrl+C` | Quit |
| `F1` | Help popup |

### Search & filter

| Key | Action |
|-----|--------|
| `/` | Filter (supports `#tag` tokens — e.g. `/#blocked auth`) |
| `?` | Cross-session **grep** (titles, paths, branches, summary, learnings) |
| `1` / `2` / `3` / `0` | Source filter: CC / OC / Copilot / all |
| `Esc` | Clear filter / project filter / mode |

### Productivity surfaces

| Key | Action |
|-----|--------|
| `L` | **Learning library** — every captured decision / lesson / tool, filterable by category (`0`-`3`) |
| `P` | **Project dashboard** — per-project stats; `s` cycles sort, `Enter` drills the main list |
| `D` | **Weekly digest** — 7-day project + learning summary; `e` exports to the Obsidian vault |
| `i` | Toggle the **Insights panel** (token totals, tool histogram, glyph timeline) |
| `[` / `]` | Move the timeline cursor; `Enter` opens the **per-turn detail modal** |
| `{` / `}` | Jump to first / last turn |
| `t` | Edit tags on the selected session |
| `l` / `u` | Link to a parent session / unlink |
| `g` | Refresh live git status |
| `x` | Actions menu (pin, new, new-with-prior-summary, ±yolo) |
| `a` | Archive / unarchive |
| `r` | Rename |
| `c` | Copy summary |
| `o` | Push session to Obsidian vault |
| `Ctrl+R` | Regenerate summary (and re-extract learnings) |
| `s` | Settings (Obsidian path / vault / daily-push) |

### Live git column

Each row carries a glyph showing the project's current git state:

| Glyph | Meaning |
|-------|---------|
| `●` red | Dirty (uncommitted changes) |
| `○` green | Clean |
| `·` dim | Not a git repo |
| `◦` yellow | Check timed out / errored |

`g` force-refreshes all entries; selection-change auto-refreshes any entry older than 30s.

## How It Works

**Session browser** — reads CC sessions from `~/.claude/projects/**/*.jsonl` (+ `sessions-index.json` for accurate project paths), OpenCode from `~/.local/share/opencode/`, and Copilot from `~/.copilot/session-state/`. Sorted by recency; pinned sessions float to the top.

**Auto-summary** — sessions with ≤20 messages auto-generate an AI summary on hover (max 5 concurrent). `Ctrl+R` force-regenerates any session and re-extracts learning points. Uses `claude --print` (your existing Claude Code subscription, no separate API key).

**Insights panel** — toggled with `i`, shows the model, token totals (input / output / cache hit %), tool & skill histogram, sub-agent dispatches, and a per-turn glyph timeline colored by category (Task / Skill / Tool / Thinking / Text). Errors are flagged red. Cached in SQLite keyed by source-file mtime.

**Per-turn detail modal** — press `Enter` while the timeline cursor is active to open a full-screen view of one assistant turn: the triggering user prompt, every content block (thinking / tool_use / text), and tool_results paired by `tool_use_id`. Works for both CC and Copilot.

**Learning library / Project dashboard / Weekly digest** — three full-screen surfaces over your entire history. The library indexes decisions, lessons, and tool/command discoveries cc-speedy captures during summarization. The dashboard rolls sessions up by project. The digest is a pure-aggregation 7-day snapshot (no LLM, instant open) you can export as a Markdown note.

**Tags & linking** — `t` adds free-form tags (normalized to `[a-z0-9_-]`); the filter accepts `#tag` tokens. `l` links sessions into parent/child chains so multi-period work is one click away.

**Obsidian export** — `o` builds a YAML-frontmatter note (status, tags, links) and pushes it to your vault via the official `obsidian` CLI. Configure the vault path under `s` (Settings).

**Tmux integration** — `Enter` opens a named tmux session running `claude --resume <id>` in the project directory. If you're already inside tmux it switches the client; otherwise it attaches. Session names use the `cc-` / `oc-` / `co-` prefix per source. The "new session with prior summary" action menu item pastes the summary into the new session via tmux bracketed-paste.

## Files

| Path | Description |
|------|-------------|
| `~/.claude/projects/` | Claude Code session data (JSONL) |
| `~/.claude/history.jsonl` | Command history (used for `/rename` titles) |
| `~/.claude/settings.json` | Modified by `cc-speedy install` (SessionEnd hook) |
| `~/.local/share/opencode/` | OpenCode sessions |
| `~/.copilot/session-state/` | Copilot CLI sessions |
| `~/.local/share/cc-speedy/data.db` | SQLite store: summaries, pins, learnings, tags, links, insights, settings |
