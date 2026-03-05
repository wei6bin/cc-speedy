# Features

- **Session browser** — lists all Claude Code sessions across projects, sorted by most recent
- **Rename titles** — shows `/rename` titles from `~/.claude/history.jsonl` as session labels
- **3-column list** — timestamp (SGT) / rename title or `[folder]` fallback / project path (3 levels)
- **Session index** — reads `sessions-index.json` for accurate project paths (fixes hyphen-ambiguity bug)
- **Resume in tmux** — `Enter` opens a named tmux session running `claude --resume <id>` in the project directory
- **Yolo mode** — `Ctrl+Y` resumes with `--dangerously-skip-permissions`
- **Filter** — `/` to fuzzy-filter sessions by rename title or project name
- **Auto-summary** — sessions with ≤20 messages auto-generate an AI summary on hover (max 5 concurrent)
- **Manual summary** — `r` to force-regenerate summary for any session
- **Tab scroll** — `Tab` focuses the summary panel for scrolling long summaries with `j`/`k`
- **Background panel** — yellow panel shows active summary generation jobs
- **Summary preview** — right panel shows project path, message count, first prompt, summary, and generated timestamp
- **Git branch** — shows `[branch]` badge when available from session index
- **Secure** — no shell injection (tmux args passed directly), session ID path sanitization, binary path quoting
- **Zero dependencies** — Linux binary statically linked via musl, no runtime deps required
