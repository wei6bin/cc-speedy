# cc-speedy Productivity Roadmap — Design Spec

**Date:** 2026-04-23
**Status:** Draft
**Scope:** 7 independent features that turn cc-speedy from a session list + resume tool into an active productivity surface.

---

## Overview

cc-speedy already captures three things that few other tools do: a unified view of sessions across three coding agents (CC/OC/Copilot), LLM-generated per-session summaries, and structured learning points (decisions, gotchas, tools) accumulated across re-generations. Today those artifacts are trapped inside individual session rows.

This spec describes seven features that expose and cross-cut that data so the user can find, categorize, and reflect on their work without leaving the TUI.

Each feature is independent and can ship in any order. Per the brainstorming skill, each one will get its own design doc + implementation plan before coding; this roadmap is the decomposition.

## Goals

- Make accumulated summaries + learnings **searchable and browsable** across sessions, not just per-session.
- Give users **lightweight organizational primitives** (tags, project grouping, session links) that scale better than binary pin/unpin.
- Surface **context-aware signals** (git status, weekly digest) that reduce the mental load of triaging "what should I pick up next?"
- Preserve the existing TUI ethos: keyboard-driven, fast, single-binary, SQLite-backed.

## Non-goals

- **Session notes (free-text per-session)** — explicitly deferred. Summaries + learnings already cover the "what happened" slot; free-text notes would duplicate without strong new value.
- **Web UI / mouse interaction** — cc-speedy is a TUI. Every feature must be keyboard-reachable.
- **Multi-machine sync** — Obsidian export already provides one sync path; we don't replicate that.
- **AI chat over sessions** — no conversational LLM layer. LLM use stays bounded to summary generation and (feature #6) digest generation.

---

## Features

### 1. Learning library

**Problem:** Learning points (decisions, gotchas, tools) are collected per-session but never viewed across sessions. The most reusable data the tool captures is hidden.

**Behavior:**
- New top-level key `L` opens a full-screen "Learning Library" view.
- Three tabs / filter keys: `1` decisions, `2` lessons & gotchas, `3` tools & commands. `0` = all.
- Live-filter box (`/`) searches point text.
- Each row shows: `[category] point text — session title · date`.
- Enter on a row jumps back to the source session (preview pane + list selection).
- `Esc` returns to the main list.

**Data model:** No schema change — reuses existing `learnings` table.

**Implementation notes:**
- New module `src/learning_library.rs` with its own `AppState`-like struct.
- `tui.rs` toggles between main `AppState` and library state.
- Library state loads all rows via `store::load_all_learnings()` (new helper; single `SELECT session_id, category, point FROM learnings` joined with `summaries.session_id` for titles).

**Effort:** Medium. ~250 LOC, 1 new file + small tui.rs additions.

**Dependencies:** None.

---

### 2. Cross-session grep

**Problem:** The existing `/` filter only matches session titles. You can't find "where did I solve that auth bug?" because the answer is in the summary body, not the title.

**Behavior:**
- `?` key enters grep mode (parallel to `/` filter mode).
- Bottom bar shows `grep: <query>` with live match count.
- Matches search across: session title, summary content, learning point text.
- Hits are ranked with title matches first, then summary matches, then learning matches.
- Preview pane highlights the matched substring.
- `Esc` clears the query and returns to normal filter.

**Data model:** No schema change. Optional: add `summaries_fts` virtual table (SQLite FTS5) for performance if naive LIKE over thousands of rows gets slow. Start without FTS; add if needed.

**Implementation notes:**
- Extend `apply_filter()` in `tui.rs` to accept a `FilterMode::{Title, Grep}` enum.
- Grep path runs `SELECT session_id, content FROM summaries WHERE content LIKE '%query%'` and merges with in-memory session list.
- Highlighting uses ratatui `Span` to wrap match regions with an accent style.

**Effort:** Small. ~150 LOC.

**Dependencies:** None. Most attractive to ship first — small, high daily value.

---

### 4. Tags / labels

**Problem:** Pin is binary. Users want to group sessions by arbitrary categorizations (`wip`, `blocked`, `needs-review`, `side-project`).

**Behavior:**
- `t` on a selected session opens a tag editor popup: comma-separated input, pre-filled with current tags.
- Tags render as `[tag1][tag2]` chips in the session row.
- Filter bar accepts `#tag` tokens: typing `#blocked wip` shows sessions tagged both `blocked` AND `wip`, minus the prefix characters for substring match.
- New top-level key `T` opens a "tag browser": list of all tags with counts, Enter filters the main list to that tag.

**Data model:**
```sql
CREATE TABLE tags (
    session_id TEXT NOT NULL,
    tag        TEXT NOT NULL,
    PRIMARY KEY (session_id, tag)
);
CREATE INDEX idx_tags_tag ON tags (tag);
```

**Implementation notes:**
- `store.rs` gets `load_tags(session_id)`, `set_tags(session_id, &[String])`, `load_all_tags_with_counts()`.
- Filter parser in `tui.rs` splits input on whitespace; tokens starting with `#` route to tag filter, rest routes to title substring.
- Tag chip rendering uses a dim accent style so it doesn't dominate the row.

**Effort:** Medium. ~200 LOC + migration.

**Dependencies:** None. Good to ship before #8 (project dashboard) since dashboard rows benefit from tag chips.

---

### 5. Git status column

**Problem:** When triaging sessions, you can't tell at a glance which project has uncommitted work. You have to open tmux and check.

**Behavior:**
- Each row gets a small git indicator after the source badge: `●` dirty, `○` clean, `·` no-git, `◦` stale (check failed / timed out).
- Branch name shown in preview pane header: `BRANCH: master (dirty)`.
- Status is cached in memory per `project_path` for the TUI lifetime, with a `g` key to force refresh.
- Refresh fires `git -C <path> status --porcelain` with a 500ms per-project timeout.

**Data model:** None. In-memory cache only — git state changes too fast to persist usefully.

**Implementation notes:**
- New `src/git_status.rs` with `fn check(path: &str) -> GitStatus` (enum: Clean, Dirty, NoGit, Error).
- Startup spawns a `tokio::task` that walks unique project paths and populates the cache in parallel.
- Focus-change on a row triggers a background refresh if cache is > 30s stale.

**Effort:** Small. ~120 LOC.

**Dependencies:** None.

---

### 6. Weekly digest

**Problem:** Users can't easily answer "what did I accomplish this week?" without manually reviewing each session. Standup prep and retrospectives become tedious.

**Behavior:**
- New key `D` opens a "Digest" view.
- Default range: last 7 days. `[` / `]` shift the window ±7 days.
- View shows:
  - Session count, unique project count, total messages (proxied from session size).
  - Aggregated "## What was done" bullets grouped by project.
  - Top learning points from the window (decisions + gotchas, deduplicated).
  - Top commands/tools discovered.
- `e` key exports the digest as `YYYY-Www-digest.md` to the configured Obsidian path.
- `r` key regenerates using `claude --print` with an aggregation prompt if the user wants a narrative version instead of the raw bullets.

**Data model:** No schema change.

**Implementation notes:**
- New module `src/digest.rs` with `fn build_digest(conn, window) -> Digest`.
- Bullet aggregation: parse `## What was done` section from each summary (simple regex), group by project.
- LLM path is optional (`r` key) — default is pure aggregation, no API call, instant.
- Obsidian export reuses the existing `obsidian.rs` writer, just with a different template.

**Effort:** Large. ~350 LOC + new prompt. LLM-regenerate path adds failure handling surface.

**Dependencies:** Benefits from #2 (grep) if the digest is used as a starting point for searching, but not required.

---

### 7. Session linking

**Problem:** Long-running work spans multiple sessions (e.g. "day 1: investigation, day 2: implementation, day 3: tests"). Today these appear as unrelated rows.

**Behavior:**
- `l` on a session opens a "link to parent" picker: lists recent sessions, `/` to filter, Enter to link.
- Linked sessions show a `↳` marker in the list.
- Preview pane shows a "Part of:" line with the parent session's title and a "Children:" line listing child sessions.
- `u` on a linked session unlinks it.
- Chain navigation: `]` on a linked session jumps to parent, `[` jumps to first child.

**Data model:**
```sql
ALTER TABLE summaries ADD COLUMN parent_session_id TEXT REFERENCES summaries(session_id);
CREATE INDEX idx_summaries_parent ON summaries (parent_session_id);
```
(Using `summaries` for the FK since not every `sessions`-index entry has a row. If a session has no summary row yet, linking creates an empty one.)

**Implementation notes:**
- `store.rs`: `set_parent(session_id, parent)`, `load_children(session_id)`, `load_chain(session_id)`.
- Chain traversal has a depth limit (e.g. 50) to guard against cycles even though the UI should prevent them.
- Link picker reuses the main list widget with a dedicated mode.

**Effort:** Medium. ~220 LOC + migration.

**Dependencies:** None, but the data is most useful alongside #8 (project dashboard) where chains naturally visualize.

---

### 8. Project dashboard

**Problem:** The flat session list mixes 40 projects. Users think in projects, not sessions.

**Behavior:**
- `P` opens a "Projects" view: one row per unique `project_path`, showing:
  - Project name (last 2 path segments).
  - Session count, last-active timestamp, pinned-session count, active-tag chips (from feature #4).
  - Git status indicator (from feature #5).
- Enter drills into a filtered main list showing only sessions for that project.
- `/` filters the project list by name.
- Sort key `s`: cycles between "last active" / "session count" / "alphabetical".

**Data model:** No schema change. Pure view over existing data, aggregated in memory at view-open.

**Implementation notes:**
- New module `src/project_view.rs`.
- Aggregation: `sessions.iter().group_by(|s| s.project_path)` with derived stats.
- Tag chips and git indicators are thin reads from the caches in features #4 and #5 — graceful when those features aren't built yet.

**Effort:** Medium. ~200 LOC.

**Dependencies:** Works standalone. Gains richness from #4 (tags) and #5 (git status) but does not require them.

---

## Suggested Sequencing

Ordered by value-per-effort:

| # | Feature | Effort | Rationale |
|---|---------|--------|-----------|
| 1 | **#2 Cross-session grep** | S | Smallest change, highest daily-use payoff. Ship first. |
| 2 | **#5 Git status column** | S | Small; immediately useful for triage. |
| 3 | **#1 Learning library** | M | Activates the knowledge that's already being collected but unused. |
| 4 | **#8 Project dashboard** | M | View-only change over existing data. |
| 5 | **#4 Tags** | M | Schema migration but well-bounded. |
| 6 | **#7 Session linking** | M | Nice-to-have; the pain is less acute than tags. |
| 7 | **#6 Weekly digest** | L | Largest surface; new LLM path + export template. Do last. |

Ordering is a suggestion — features are independent and can ship in any order the user prefers.

## Process

Each feature in this roadmap will go through the normal brainstorming → design → plan → implement cycle when it's picked up:

1. Dedicated design spec: `docs/superpowers/specs/YYYY-MM-DD-<feature>-design.md`.
2. Implementation plan via `writing-plans` skill.
3. Implementation + tests + release.

This roadmap doc itself is the decomposition artifact and does not gate implementation of any individual feature.

## Open Questions

- Feature #6 digest: default to pure aggregation (instant, no API) or LLM-narrative (slower, richer)? Current proposal: aggregation by default, LLM on `r`. Revisit during #6 design.
- Feature #5 git: how stale is "too stale"? 30s cache hint is a guess — may need observation once shipped.
- Feature #4 tags: shared tag vocabulary or free-form? Current proposal: free-form, with the tag browser (`T`) giving the user visibility into their own vocabulary.

## Risks

- **Scope creep.** Seven features is a lot; discipline is to ship one, learn, then pick the next — don't parallelize.
- **TUI busyness.** Row rendering is already dense. Each new column (tags, git) must be optional-looking by default (dim accent, small glyphs) or we lose the "fast scan" property of the current list.
- **Data model accretion.** Two new tables (`tags`, optional FTS) + one column (`parent_session_id`). Keep migrations additive and idempotent; never drop or rename existing tables.
