# cc-speedy

Terminal TUI to browse and resume Claude Code sessions in named tmux sessions, with AI-generated conversation summaries.

## Install (prebuilt binary)

Pick the binary for your platform from the [latest release](https://github.com/wei6bin/cc-speedy/releases/latest):

**Linux (x86_64):**
```bash
curl -sL https://github.com/wei6bin/cc-speedy/releases/latest/download/cc-speedy-x86_64-unknown-linux-musl.tar.gz \
  | tar xz && sudo mv cc-speedy-x86_64-unknown-linux-musl /usr/local/bin/cc-speedy
```

**macOS Apple Silicon (M1/M2/M3):**
```bash
curl -sL https://github.com/wei6bin/cc-speedy/releases/latest/download/cc-speedy-aarch64-apple-darwin.tar.gz \
  | tar xz && sudo mv cc-speedy-aarch64-apple-darwin /usr/local/bin/cc-speedy
```

**macOS Intel:**
```bash
curl -sL https://github.com/wei6bin/cc-speedy/releases/latest/download/cc-speedy-x86_64-apple-darwin.tar.gz \
  | tar xz && sudo mv cc-speedy-x86_64-apple-darwin /usr/local/bin/cc-speedy
```

Then register the SessionEnd hook:
```bash
cc-speedy install
```

## Install (from source)

```bash
cargo install --path .
cc-speedy install
```

## Usage

```bash
cc-speedy           # open session browser TUI
```

## Key Bindings

| Key | Action |
|-----|--------|
| `j` / `k` or arrows | Navigate sessions (or scroll summary when focused) |
| `Tab` | Toggle focus between session list and summary panel |
| `/` | Enter filter mode |
| `Esc` | Clear filter / exit filter mode |
| `Enter` | Resume session in tmux |
| `Ctrl+Y` | Resume session in yolo mode (`--dangerously-skip-permissions`) |
| `r` | Regenerate summary for selected session |
| `q` / `Ctrl-C` | Quit |

## How It Works

**Session browser:**
- Reads sessions from `~/.claude/projects/**/*.jsonl` and `sessions-index.json`
- Reads `/rename` titles from `~/.claude/history.jsonl`
- Filters out sessions with fewer than 4 messages
- 3-column list: timestamp (SGT) / rename title / folder path
- Sorted by most recent first

**Tmux integration:**
- `Enter` resumes the session in a named tmux window
- `Ctrl+Enter` resumes with `--dangerously-skip-permissions` (yolo mode)
- Session name derived from last 2 path segments of project path
- If already inside tmux: switches to existing session or creates one

**Summaries:**
- Auto-generated for sessions ≤ 20 messages when selected
- Max 5 concurrent background summary processes (excess silently skipped, not queued)
- Press `r` to force regenerate any session
- Stored in `~/.claude/summaries/<session-id>.md`
- Uses `claude --print` (no separate API key — uses your Claude Code subscription)

## Files

| Path | Description |
|------|-------------|
| `~/.claude/projects/` | Claude Code session data |
| `~/.claude/history.jsonl` | Command history (used for `/rename` titles) |
| `~/.claude/summaries/` | Generated session summaries |
| `~/.claude/settings.json` | Modified by `cc-speedy install` |
