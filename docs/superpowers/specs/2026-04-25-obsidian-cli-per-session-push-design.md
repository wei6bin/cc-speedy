# Obsidian CLI — Per-session push enrichment

**Status:** Design approved 2026-04-25, ready for implementation plan.
**Sub-project:** A of five (foundation + features #1, #3, #4 from the integration brainstorm).
**Depends on:** the official Obsidian CLI bundled with Obsidian.app, reachable as `obsidian` on PATH (already wrapped at `~/.local/bin/obsidian` for this WSL host).

## Why

cc-speedy already exports session notes to a vault via direct file writes. With the official `obsidian` CLI we can teach the same export pipeline to:

1. Append a one-liner to today's daily note for every successful export, so the day's CC work surfaces in the user's existing daily-note workflow without manual copy-paste.
2. Enrich frontmatter on the session note with status, counts, and source metadata, so Dataview queries / dashboards become trivial.
3. Auto-tag session notes by learning category and count, so the same dashboards can facet by "what kind of session this was".

This sub-project also lays the CLI-wrapper foundation (`src/obsidian_cli.rs`) that all later sub-projects (B–E) reuse.

## Non-goals (handled in later sub-projects)

- Project notes (`docs/<project>.md` with backlinks) → **Sub-project B**.
- SessionEnd hook upgrade (auto-push without TUI) → **Sub-project C**. After A lands, the hook calls the same enriched `export_to_obsidian` path.
- Weekly periodic-note integration, "send today's work" key, in-progress callouts, `summarize-week` subcommand → **Sub-project D** (gated on user installing the Periodic Notes plugin).
- Eval scratchpad TUI mode, vault prune/relink subcommands → **Sub-project E**.

## Design

### A.1 Foundation — `src/obsidian_cli.rs`

A thin Rust wrapper around the `obsidian` binary. Three typed entry points for sub-project A; `frontmatter_set` / `frontmatter_set_list` and other helpers are deferred to sub-projects B–E that actually need them.

```rust
pub fn is_available() -> bool;
pub fn vault_is_running(vault: &str) -> bool;
pub fn daily_append(vault: &str, content: &str) -> Result<()>;
```

Implementation rules:

- **Concurrency.** All calls run inside `tokio::task::spawn_blocking`. The CLI proxies through a Windows redirector with ~150–400 ms typical latency — acceptable off the main thread, never on it.
- **Path arguments.** Use vault-relative `file=<basename>` form everywhere possible (Obsidian resolves wikilink-style). Avoid passing host paths so we never need `/mnt/c/…` ↔ `C:\…` translation. Vault-name argument also stays as a plain string passed via `vault=<name>`.
- **Argument shell-safety.** `Command::new("obsidian").args([…])` only — never go through a shell, never string-format values into a single command line. Values containing `"` are escaped per the CLI's own escape rules (`\"` for quote, `\\n` for newline).
- **Error model.** Three discrete error categories surfaced as typed `Err` variants:
  - `Error::CliMissing` — `obsidian --help` fails (binary absent).
  - `Error::NotRunning` — `obsidian eval code="app.vault.getName()"` returns non-zero or empty stdout.
  - `Error::CommandFailed { stderr_first_line }` — anything else.
- **Status flashes.**
  - `o`-key (interactive): map errors to user-visible strings. `CliMissing` → "Obsidian CLI not installed (see docs/obsidian-setup.md)". `NotRunning` → "Obsidian not running — open the vault first". `CommandFailed` → "Obsidian: <first line>".
  - Auto-export (post-`Ctrl+R`): swallow errors, log to stderr only. Never blocks the user.

### A.2 Authorship contract

cc-speedy continues to own the session note file end-to-end. On every export:

- `obsidian.rs` writes the **whole file** — body + frontmatter — overwriting any prior contents. Same as today.
- After the file write succeeds, the daily-note append (A.3) and any future CLI-side mutations run.
- Body edits made by the user inside Obsidian are not preserved across regeneration. This is the same trade-off as today; users who want to keep their edits should copy them to a separate note.

Rejected alternatives, recorded for context:

- *CLI-mediated frontmatter patching:* lower payoff (frontmatter is rebuilt on every regen anyway) and higher risk (partial-write inconsistencies if the CLI fails mid-call).
- *Append-only history:* unbounded note growth, messy diffs.

### A.3 Daily-note append (feature #1)

Triggered after every successful `export_to_obsidian` call (both `o`-key and auto-export from `spawn_summary_generation`).

**Line format**, one bullet per session per export:

```
- [[<note-stem>]] **<project_name>** · <message_count> msgs · <status_emoji> <factual_title> #cc-session
```

- `<note-stem>` = the filename produced by `obsidian.rs` minus the `.md` extension, i.e. `{YYYY-MM-DD}-{project_slug}-{id_prefix}` (e.g. `2026-04-25-cc-speedy-abcdef`). Definition unchanged from current code.
- `<status_emoji>` from the `## Status` heading in the factual summary: `completed` → `✅`, `in progress` → `🔧`, otherwise `🚧`.
- `<factual_title>` = first non-empty bullet of `## What was done`, truncated to 80 chars (Unicode-safe via `chars().take(80)`).
- Trailing `#cc-session` tag for daily-note Dataview filtering.

**Placement.** Bottom of the daily note via `obsidian daily:append content="…"`. The CLI auto-creates today's daily note if missing. We do **not** maintain a `## CC sessions` heading section — Obsidian's outline view groups bullets visually, and `daily:append` does not support insertion at a specific heading without `path=`, which would defeat A.1's "no host paths" rule.

**Idempotency.** Before appending, query today's daily note for the wikilink:

```javascript
// passed to `obsidian eval code="..."`
const today = window.moment().format('YYYY-MM-DD');
const f = app.vault.getMarkdownFiles().find(x => x.basename === today);
f && (await app.vault.read(f)).includes('[[<note-stem>]]')
```

If `true` → skip the append. If `false` → append. We do not try to "update" an existing line on regeneration; the session note itself carries the latest content, the daily-note line is a pointer.

### A.4 Frontmatter enrichment (feature #3)

`obsidian.rs` writes the same single file as today, but with richer frontmatter:

```yaml
---
date: 2026-04-25
project: "/home/weibin/repo/ai/cc-speedy"
project_name: "cc-speedy"
session_id: "abcdef-…"
source: "cc"
status: "completed"
message_count: 47
learnings_count: 8
git_branch: "master"
last_exported: 2026-04-25T07:38:00+08:00
tags: [agent-session, cc-source/cc, cc-status/completed, cc-decisions/3, cc-lessons/4, cc-tools/1, cc-has-decisions, cc-has-lessons, cc-has-tools]
---
```

Field derivations:

| Field | Source |
|---|---|
| `project_name` | `path_last_n(project_path, 1)` |
| `source` | `match session.source { ClaudeCode => "cc", OpenCode => "oc", Copilot => "co" }` |
| `status` | parse the `## Status` line — first word lowercased, normalised to `completed` / `in_progress` / `unknown` |
| `message_count` | `session.message_count` |
| `learnings_count` | `learnings.len()` |
| `git_branch` | `session.git_branch` (empty string omitted) |
| `last_exported` | `chrono::Local::now()` ISO-8601 |

Edge cases:

- YAML escaping: project paths and titles route through a single `yaml_escape_str` helper that produces double-quoted strings with `\\` and `\"` escapes. Existing code already does this for `project` and `session_id`; we extend it.
- `git_branch` empty → omit field, do not emit `git_branch: ""`.
- Timezone: ISO-8601 with offset (`%Y-%m-%dT%H:%M:%S%z`) so the value sorts correctly in Dataview.

No additional CLI calls — this is a YAML format change inside `obsidian.rs`.

### A.5 Learning-count tags (feature #4)

Tags are emitted into the same `tags:` list as A.4, in three families:

- **Faceted counts:** `cc-decisions/N`, `cc-lessons/N`, `cc-tools/N` where N is the count of learning points in each category. Skipped when N=0.
- **Bare facets:** `cc-has-decisions`, `cc-has-lessons`, `cc-has-tools`. Emitted iff the corresponding count > 0. Lets `tag:#cc-has-lessons` filter in Dataview without parsing N out of the slash-tag.
- **Status / source facets:** `cc-status/<status>` and `cc-source/<source>` from A.4.

Always-present: `agent-session` (existing).

The order of tags in the YAML list is deterministic so re-exports produce stable diffs:

```
[agent-session, cc-source/<src>, cc-status/<st>,
 cc-decisions/N, cc-lessons/N, cc-tools/N,        # only nonzero
 cc-has-decisions, cc-has-lessons, cc-has-tools]  # only nonzero
```

### A.6 Settings additions

Two new fields in the existing settings panel (key `s`):

| Setting | DB key | Default | Purpose |
|---|---|---|---|
| Obsidian vault name | `obsidian_vault_name` | basename of `obsidian_kb_path` | Argument for `vault=<name>`. Inferred if unset; user can override. |
| Push to daily note | `obsidian_daily_push` | `true` | Disables A.3 without disabling export. Stored as `"1"`/`"0"` per existing settings convention. |

Settings panel UI (`draw_settings_popup`) gains two rows below the existing path row. Edit semantics identical to the path row.

### A.7 Failure / fallback matrix

| Condition | `o`-key behaviour | auto-export behaviour |
|---|---|---|
| `obsidian` not on PATH | flash "Obsidian CLI not installed", file write still succeeds | log stderr, file write still succeeds |
| Obsidian app not running | flash "Obsidian not running — open the vault first", file write still succeeds | log stderr, file write still succeeds |
| `daily:append` fails | flash "Obsidian daily push failed: \<reason\>" but file write + sync mark succeed | log stderr, file write + sync mark succeed |
| `obsidian_daily_push` setting off | skip A.3 silently | skip A.3 silently |
| `obsidian_kb_path` unset | "Obsidian path not set" (existing) | export skipped (existing) |

The file-write export path remains the source of truth: A.3 is layered on top and never blocks the file write. The `obsidian_synced` indicator (`◆` glyph) is set on file-write success regardless of A.3 outcome.

## Data model changes

- **Settings table:** two new keys (`obsidian_vault_name`, `obsidian_daily_push`). No schema change — `settings` is already a free-form key/value table.
- **No other DB changes.** No migration.

## File-touch summary

| File | Change |
|---|---|
| `src/obsidian_cli.rs` | **new** — CLI wrapper module |
| `src/obsidian.rs` | enriched frontmatter writer; calls `obsidian_cli::daily_append` after successful file write |
| `src/settings.rs` | two new fields on `AppSettings`, save/load helpers |
| `src/tui.rs` | settings panel UI gains two rows; `save_selected_to_obsidian` and `spawn_summary_generation` already call `export_to_obsidian` so no further wiring needed |
| `src/lib.rs` | `pub mod obsidian_cli;` |
| `Cargo.toml` | no changes (uses `tokio::process::Command` already in tree) |
| `docs/obsidian-setup.md` | **new** — quick install/registration guide for users without the CLI |

## Testing

- **Unit tests in `obsidian.rs`** for the YAML-frontmatter writer: status parsing, learning-count tag generation, escaping of titles with quotes/newlines, deterministic tag ordering.
- **Unit tests in `obsidian_cli.rs`** for argument construction (no shell metacharacter leakage, escape of `"` and `\n` in `content=` values).
- **Integration test** behind `#[ignore]`, runnable with `cargo test -- --ignored`, that requires a real running Obsidian + a throwaway test vault. Verifies `daily_append` produces the expected line and `frontmatter_set` round-trips. Skipped in CI.
- **No mocking of `obsidian`** — the CLI surface is small enough that we either talk to a real one in the integration test or test pure Rust logic in unit tests.

## Rollout

Single PR. No feature flag — the CLI calls are no-ops if `is_available()` is false, so users without the CLI experience no behavioural change. The new frontmatter fields are additive; existing notes get them on next regeneration, but nothing breaks for notes never re-exported.
