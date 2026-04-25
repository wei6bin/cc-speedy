# Obsidian CLI integration — feature roadmap

**Source:** brainstorm session 2026-04-25 (Saturday). Captures what was discussed
in the conversation that produced sub-project A's spec/plan/implementation, so
future-you can pick up B–E without re-deriving the decomposition.

**Status of each sub-project is tracked at the bottom.**

## CLI being integrated

The official `obsidian` command-line interface bundled with Obsidian.app
(<https://obsidian.md/cli>). Toggled on inside the desktop app at
**Settings → General → Command line interface**. The CLI requires the desktop
app to be running.

WSL note: a wrapper at `~/.local/bin/obsidian` proxies to the Windows-side
`Obsidian.com` redirector. Setup recipe lives in `docs/obsidian-setup.md`.

## Direction (from the brainstorm)

User picked **A + D + E** from the four direction options offered:

- **A. Push more out to Obsidian** — make cc-speedy a richer source of vault content
- **D. Power user / automation** — drive Obsidian programmatically
- **E. Workflow surface** — make daily/weekly review easier

Not picked: B (pull Obsidian context into the TUI) and C (launcher into
Obsidian). These can be revisited later but are out of scope for now.

## Feature list

11 features were agreed (all `yes` from the user; notes inline).

### Push features (per-session)

1. **Daily-note append.** When a session ends or `Ctrl+R` runs, append a
   one-liner to today's daily note via `obsidian daily:append`.
2. **Project-note backlink.** Auto-maintain a per-project markdown file at
   **`<vault>/docs/<project-slug>.md`** (user-confirmed path) and append a
   backlink to each new session note.
3. **Frontmatter enrichment.** Add `status`, `learnings_count`, `git_branch`,
   `last_resumed`, `source`, etc. to the session note's frontmatter so
   Dataview queries are trivial.
4. **Tag from learnings.** Auto-tag session notes with `cc-decisions/N`,
   `cc-lessons/N`, `cc-tools/N` based on extracted learning counts, plus
   bare facets (`cc-has-decisions` etc.) for filtering.

### Workflow surface features

5. **Weekly digest into the vault, not a flat file.** Replace the current
   `D` digest's flat-file write with `obsidian periodic:weekly:append`.
   **Requires the Periodic Notes plugin** to be installed in Obsidian —
   reminder to install before sub-project D starts.
6. **"Send today's work" key.** Single TUI key (e.g. `Ctrl+D`) bundling every
   session you touched today into today's daily note as a section. Idempotent
   — re-running replaces the section.
7. **Incomplete session callout.** For any session whose summary has
   `## Status: In progress`, add a `> [!todo]` callout to today's daily note
   with `[[link]]` so the work doesn't get lost.

### Power / automation features

8. **`SessionEnd` hook upgrade.** The current hook only persists summary to
   SQLite; extend it so when a vault is configured, it also pushes the
   session note + appends to daily, with no TUI involvement. Makes the
   integration "always on".
9. **Eval scratchpad.** TUI key `:` opens a one-line input that sends
   `obsidian eval code=…` and shows the result in preview. For poking at
   vault state without leaving cc-speedy.
10. **Cron-driven weekly digest.** `cc-speedy summarize-week` subcommand
    suitable for cron, that builds the weekly digest entirely via CLI, no
    TUI. Pairs with #5.
11. **Vault-side cleanup commands.** `cc-speedy obsidian-prune` to delete
    session notes for archived sessions; `cc-speedy obsidian-relink` to fix
    broken links if you reorganise.

## Sub-project decomposition

5 sub-projects in dependency order. Each gets its own brainstorm → spec →
plan → implementation cycle.

| Sub-project | Features | Depends on | Notes |
|---|---|---|---|
| **A** Per-session push enrichment | #1, #3, #4 + the foundation `obsidian_cli` Rust wrapper | nothing | First. Forces the CLI-foundation decisions everything else inherits. |
| **B** Project notes | #2 | A (uses CLI wrapper) | New artifact at `<vault>/docs/<project-slug>.md`; project-scoped lifecycle is meaningfully different from session-scoped, so worth its own design pass. |
| **C** SessionEnd hook upgrade | #8 | A and B | Trivial once A+B exist — runs the same push pipeline non-interactively when the hook fires. |
| **D** Daily / weekly workflow surface | #5, #6, #7, #10 | A | **Periodic Notes plugin must be installed in Obsidian first.** |
| **E** Power tools | #9, #11 | A | Eval scratchpad + vault hygiene subcommands. Pure niceties, completely independent of B/C/D. |

## Author's-decisions (locked-in answers from the brainstorm)

These are decisions the user made during brainstorming that should not need
to be re-litigated when each sub-project is designed:

- **Project-note location** (#2): `<vault>/docs/<project-slug>.md`. Slug is
  the existing `path_last_n(project_path, 2)` form (last two segments,
  slashes → dashes).
- **Periodic Notes plugin** (#5, #10): user is OK installing it; remind at
  start of sub-project D.
- **Weekly digest replacement** (#5): the existing `D` digest's flat-file
  output is to be replaced (not augmented) with the periodic-note append
  flow.
- **Authorship contract** (sub-project A): cc-speedy owns each session note
  end-to-end. Body edits made by the user inside Obsidian do not survive
  regeneration. Same trade-off as today.

## Status

| Sub-project | Status | Spec | Plan | Notes |
|---|---|---|---|---|
| **A** | ✅ Done | `2026-04-25-obsidian-cli-per-session-push-design.md` | `2026-04-25-obsidian-cli-per-session-push.md` | Shipped 2026-04-25 across commits `31ca64f`..`a955847`. 17 commits including review fixes (HIGH `bb33a47`, IMPORTANT `7c713ad` `6818d38` `096ca60`) and post-merge polish (`a955847` for in-memory glyph refresh after Ctrl+R). 185 tests passing. |
| **B** | 📋 Planned | — | — | Brainstorm next when picking up project notes. |
| **C** | 📋 Planned | — | — | Brainstorm after B. |
| **D** | 📋 Planned (blocked on plugin install) | — | — | Reminder: install the Periodic Notes Obsidian community plugin before brainstorming. |
| **E** | 📋 Planned | — | — | Independent of B/C/D — can be brainstormed any time after A. |

When picking up B/C/D/E, invoke `superpowers:brainstorming` and reference
this roadmap so the agent doesn't re-derive the decomposition.
