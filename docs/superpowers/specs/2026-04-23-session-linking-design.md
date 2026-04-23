# Session Linking — Design Spec

**Date:** 2026-04-23
**Status:** Approved
**Part of:** [Productivity Roadmap](./2026-04-23-productivity-roadmap-design.md) — feature #7

---

## Overview

Long-running work often spans multiple sessions: investigation → implementation → tests. Today these appear as three unrelated rows. Add an explicit parent-of relation between sessions so chains of work are navigable.

MVP scope: store the relation, edit from a picker, show it in the preview pane, support unlink. Chain-navigation keys (`[` / `]` between parent and children) and a row-level `↳` marker are deferred pending signal they're wanted.

---

## 1. Data Model

New table — standalone, no foreign keys (sessions may not have a summary row yet):

```sql
CREATE TABLE IF NOT EXISTS links (
    session_id        TEXT PRIMARY KEY,
    parent_session_id TEXT NOT NULL,
    linked_at         INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);
CREATE INDEX IF NOT EXISTS idx_links_parent ON links (parent_session_id);
```

Why a dedicated table, not a column on `summaries`:
- Sessions without summaries can still participate.
- Additive migration matches the project's convention.
- Lookups by parent (to list children) are indexed.

A session has at most one parent (PRIMARY KEY). A parent can have many children.

## 2. Store API (src/store.rs)

```rust
pub fn set_link(conn: &Connection, child_id: &str, parent_id: &str) -> Result<()>;
pub fn unset_link(conn: &Connection, child_id: &str) -> Result<()>;
pub fn load_all_links(conn: &Connection) -> Result<HashMap<String, String>>;  // child → parent
```

The caller is responsible for not creating cycles. Sanity: `set_link` refuses if `child_id == parent_id`.

## 3. Edit Flow (`l` key)

- Normal + `l` on a selected session → `AppMode::LinkPicker`.
- Picker renders a full-width list of every session *except* the current one, sorted by recency.
- Keys inside picker:
  - `j` / `k` / arrows — navigate.
  - `/` — enter `AppMode::LinkPickerFilter` sub-mode; substring match on title.
  - `Enter` — set `links[current] = selected`; exit to Normal.
  - `Esc` — cancel.

## 4. Unlink (`u` key)

Normal + `u` on a selected session removes its `links` row (if any). No-op if not linked.

## 5. Preview Pane Display

When in Normal mode, the preview for a selected session adds:

```
PARENT:   2026-04-18  [CC]  investigation session title  (Enter to jump)
CHILDREN: 2026-04-20  [CC]  implementation follow-up
          2026-04-22  [OC]  test coverage pass
```

- `PARENT:` line omitted when no parent.
- `CHILDREN:` list omitted when the session has no children.
- Formatting mirrors the main list row — date, source badge, title.
- No interactive `Enter to jump` from the preview pane in MVP (documented in the line, but hotkey navigation is deferred).

## 6. Files Changed

- `src/store.rs` — migration + three new fns.
- `src/tui.rs`
  - `AppMode::LinkPicker`, `AppMode::LinkPickerFilter`.
  - `AppState`:
    - `parent_of: HashMap<String, String>` — child → parent.
    - `link_picker_filter: String`.
    - `link_picker_filtered: Vec<usize>` — indices into `app.sessions` (candidate parents).
    - `link_picker_list_state: ListState`.
  - `l` / `u` handlers in Normal.
  - LinkPicker navigation + Enter + Esc handlers.
  - LinkPicker filter sub-mode (char/backspace/enter/esc).
  - Preview pane adds PARENT / CHILDREN blocks.
  - New `draw_link_picker()` full-width renderer.
- `tests/session_linking_test.rs` — store round-trip; children lookup via reverse scan.

## 7. Testing

**Unit:**
- `set_link` then `load_all_links` returns the mapping.
- `set_link` rejects self-link (child == parent).
- `set_link` twice for same child replaces (PK conflict → ON CONFLICT DO UPDATE pattern).
- `unset_link` removes the row; subsequent load lacks it.
- Children are derivable by reverse-scanning `parent_of.values()`.

**Manual TUI:**
- `l` opens picker, excludes the current session.
- `/` filters picker; Enter saves; Esc cancels.
- Preview shows PARENT and CHILDREN lines once linked.
- `u` removes the link; preview loses the PARENT line.

## 8. Non-Goals (deferred)

- **Chain navigation keys `[` / `]`** — jump parent / first child. Deferred until the PARENT/CHILDREN preview display surfaces signal that users want faster hops.
- **Row `↳` marker** — visual noise without clear benefit at list scan time.
- **Cycle prevention beyond self-link** — the picker is recency-sorted; accidentally creating a ring is unlikely. If it becomes a real problem, add a DFS check in `set_link`.
- **Bulk un-link / re-parent** — edit per session.
- **Export to Obsidian includes chain** — considered but would conflate with the existing per-session export format.
