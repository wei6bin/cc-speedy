# Weekly Digest — Design Spec

**Date:** 2026-04-23
**Status:** Approved
**Part of:** [Productivity Roadmap](./2026-04-23-productivity-roadmap-design.md) — feature #6

---

## Overview

Answering "what did I do this week?" requires scrolling 20+ session rows and mentally aggregating. Add `D` to open a pre-formatted digest of activity over the last 7 days: session counts, a per-project breakdown with session titles, and the learning points captured in the window. Export to Obsidian via `e`.

MVP scope: **pure aggregation, no LLM**. Instant open; no API call; no failure surface. The LLM-narrative regenerate path sketched in the roadmap is deferred until signal that the raw aggregation isn't rich enough.

---

## 1. Mode & Keybinding

- `D` (Shift+d) at top level → `AppMode::Digest`.
- `Esc` exits to Normal.
- `e` inside Digest mode exports to Obsidian (if configured; otherwise status flash "No Obsidian path set").
- Window is fixed at 7 days back from "now". No `[` / `]` shifting in MVP.

## 2. Content Structure

```
── Weekly Digest ─────────────────────────────────────
  Window: 2026-04-17 → 2026-04-24
  Sessions: 12       Projects: 3       Learnings: 18

── By project ───────────────────────────────────────
▸ cc-speedy  (8 sessions, last 2026-04-24)
    • add cross-session grep (?)
    • git status column
    • learning library view
    • ...

▸ ec-hip-tools  (3 sessions, last 2026-04-22)
    • pipeline monitoring
    • ...

── Learnings captured ───────────────────────────────
  [DEC] pick postgres over mysql — cc-speedy · 2026-04-22
  [LSN] tmux paste-buffer -p preserves bracketed paste — cc-speedy · 2026-04-20
  [TOL] git -C <path> is cleaner than chdir — cc-speedy · 2026-04-19
  ...
```

Each row is plain text in a scrollable Paragraph (no interactive per-row selection in MVP).

## 3. Aggregation Source

A session "occurred in the window" iff its `modified` timestamp is within the last 7 days. This reflects the cc-speedy data model — we index sessions by their JSONL `modified` time, not by a session-start time we don't track.

Learnings "occurred in the window" iff their `captured_at` (unix seconds) is within the last 7 days.

## 4. Data Model

No schema change. Reuses `summaries` for session titles and `learnings` for points.

## 5. Module API (new `src/digest.rs`)

```rust
pub struct DigestData {
    pub window_start: SystemTime,
    pub window_end:   SystemTime,
    pub session_count: usize,
    pub learning_count: usize,
    pub projects: Vec<ProjectDigest>,     // sorted by last_active desc
    pub learnings: Vec<LearningLine>,     // sorted by captured_at desc
}

pub struct ProjectDigest {
    pub project_path: String,
    pub name: String,                     // last 2 segments
    pub session_count: usize,
    pub last_active: SystemTime,
    pub session_titles: Vec<String>,      // sorted by modified desc
}

pub struct LearningLine {
    pub category:     String,             // "decision_points" | ...
    pub point:        String,
    pub project_name: String,             // for the "— project · date" suffix
    pub captured_at:  i64,
}

pub fn build_digest(
    sessions: &[UnifiedSession],
    learnings: &[LearningEntryWithSession],  // joined at caller
    window_days: i64,
    now: SystemTime,
) -> DigestData;

pub fn render_digest(d: &DigestData) -> String;
```

`build_digest` is pure — no DB, no I/O. The caller resolves sessions + learnings + window. This keeps the module testable in memory.

## 6. Export to Obsidian (`e`)

Reuses `src/obsidian.rs` pattern. Writes to `<vault>/cc-speedy/digests/YYYY-Www-digest.md`. Week number derived via chrono's ISO week.

If no Obsidian path configured, status message "No Obsidian path set — see `s` for settings".

## 7. Files Changed

- `src/digest.rs` — new, ~180 LOC: data types + `build_digest` + `render_digest`.
- `src/store.rs` — small helper `load_learnings_with_captured_at_since(window_start) -> Vec<(session_id, category, point, captured_at)>` (or just reuse `load_all_learnings` and filter in-memory).
- `src/tui.rs`
  - `AppMode::Digest`, field `digest_text: String`.
  - `D` handler: gather data, call `build_digest` + `render_digest`, enter Digest mode.
  - `Esc` exit, `e` export.
  - Render full-screen Paragraph; `j/k` scroll.
- `src/obsidian.rs` — add `export_digest(vault_path, date, digest_text)`.
- `tests/digest_test.rs` — build_digest over a small fixture, assert shape + ordering.

## 8. Testing

**Unit:**
- Session within window → counted.
- Session outside window → excluded.
- Per-project grouping: 3 sessions / 2 projects → 2 ProjectDigest rows.
- Project order: most-recent last_active first.
- Session titles per project: most-recent modified first.
- Learnings filtered by captured_at.
- `render_digest` produces the expected header + sections for an empty window ("no activity") and a populated window.

**Manual:**
- `D` opens with the last 7 days.
- `e` writes a markdown file under the Obsidian vault.
- `Esc` returns.

## 9. Non-Goals (deferred)

- **`[` / `]` window shifting.** Add later if users want arbitrary weeks.
- **LLM-narrative regenerate (`r`).** Raw aggregation is the v1 value prop; LLM-paraphrase is gloss on top.
- **Per-session-source filtering in the digest.** The digest spans all three agents; filtering would be a separate feature.
- **Charts / sparklines.** Text-only.
