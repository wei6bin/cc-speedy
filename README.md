# cc-speedy

Terminal TUI to browse and resume Claude Code sessions in named tmux sessions, with AI-generated conversation summaries.

## Install

```bash
cargo install --path .
cc-speedy install   # registers SessionEnd hook in ~/.claude/settings.json
```

Re-run `cc-speedy install` after reinstalling to update the hook path.

## Usage

```bash
cc-speedy           # open session browser TUI
```

## Key Bindings

| Key | Action |
|-----|--------|
| `j` / `k` or arrows | Navigate sessions |
| `/` | Enter filter mode |
| `Esc` | Clear filter / exit filter mode |
| `Enter` | Resume session in tmux |
| `r` | Regenerate summary for selected session |
| `q` / `Ctrl-C` | Quit |

## How It Works

**Session browser:**
- Reads all sessions from `~/.claude/projects/**/*.jsonl`
- Filters out sessions with fewer than 4 messages (command-line only)
- Sorted by most recent first
- 40/60 split: session list on left, summary preview on right

**Tmux integration:**
- On `Enter`, opens a named tmux session: `<parent-dir>-<project-dir>`
- If already inside tmux: switches to existing session or creates one
- If outside tmux: creates and attaches to the session
- Session runs `claude --resume <session-id>` in the project directory

**Summaries:**
- Auto-generated at session end via `SessionEnd` hook (requires `ANTHROPIC_API_KEY`)
- On-demand generation when hovering a session without a summary
- Press `r` to force regenerate
- Stored in `~/.claude/summaries/<session-id>.md`
- Model: `claude-haiku-4-5` (fast, cheap)

## Environment

| Variable | Required | Description |
|----------|----------|-------------|
| `ANTHROPIC_API_KEY` | Yes | For summary generation |

## Files

| Path | Description |
|------|-------------|
| `~/.claude/projects/` | Claude Code session data |
| `~/.claude/summaries/` | Generated session summaries |
| `~/.claude/settings.json` | Modified by `cc-speedy install` |
