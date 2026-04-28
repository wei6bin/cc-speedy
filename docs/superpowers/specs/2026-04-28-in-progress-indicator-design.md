# In-Progress Indicator — Design

**Date:** 2026-04-28
**Status:** Draft, awaiting user review

## Problem

When a user has multiple coding agent sessions running in tmux (Claude Code, OpenCode, Copilot), there is no way from inside cc-speedy to see which sessions are currently active. The user can't tell at a glance whether a session is being worked on right now, was recently active, or is long-finished.

## Goal

Add a per-row liveness indicator that distinguishes three states across all three session sources:

- **`live`** — agent is actively producing output right now. The session JSONL was written to in the last few seconds **and** the trailing assistant turn has an unclosed tool call (i.e. agent is mid-thought).
- **`recent`** — session was written to in the last N minutes but the trailing turn is closed. Agent is between prompts or paused waiting for the user.
- **`idle`** — older than the recent threshold. Done, or long-paused.

The state is cached and exposed via a public `liveness` module so that future features (notably the streaming web view) can subscribe to the same primitive.

## Non-goals

- Detecting the agent **process** itself. We rely entirely on the on-disk session log; if the user kills the agent process but the JSONL is fresh, we'll briefly call it `recent`. That's acceptable.
- Identifying error / stuck states (out of scope for v1 — too source-specific to detect reliably).
- Pushing notifications or sounds when a session goes `live` or `idle`.
- Filesystem watch notifications (deferred to a future perf pass; we poll on a short tick and bound the cost by only checking visible rows).

## User-facing behavior

### Glyphs

A new column renders between the source badge (`[CC]` / `[OC]` / `[CO]`) and the timestamp:

| State | Glyph | Color | Meaning |
|-------|-------|-------|---------|
| `live` | `▶` | vivid green (`Color::Rgb(0xa6, 0xe3, 0xa1)` from theme palette) | agent is currently producing output |
| `recent` | `◦` | dim cyan (`Color::Rgb(0x89, 0xdc, 0xeb)`) | active in the last N min, idle right now |
| `idle` | (space) | — | older than threshold |

Width is one cell, always rendered (a space when idle) so that columns stay aligned. Glyph distinct from the git column (`●` / `○` / `·` / `◦`) — the cyan `◦` of `recent` shares a symbol with git's "timeout/error" but is in a different column with a different color, and they will not be confused.

### Thresholds

- `live` requires both: file `mtime` within last 5 seconds **and** trailing turn unclosed.
- `recent` requires: file `mtime` within last 5 minutes **and** trailing turn is closed (or `live` checks failed).
- `idle` is the catch-all default.

The 5-second `live` window is forgiving enough that brief tool-call latency doesn't bounce a session out of `live`. The 5-minute `recent` window matches typical "between prompts" pauses without keeping long-finished sessions glowing indefinitely.

### Refresh cadence

- A background task ticks every 5 seconds.
- Each tick computes liveness only for sessions currently visible in the active list (selected row ± viewport height, plus a small slack of 5 rows).
- Off-screen sessions are not polled; their liveness defaults to `idle` until they enter the viewport (at which point they get an immediate one-shot check).
- Pressing `R` / `F5` does **not** force a liveness re-poll — refresh only re-scans the session list. Liveness updates on its own tick. (Rationale: if the user wants instant liveness, they can scroll the row into view, which triggers a one-shot check.)

### Visibility

The indicator renders in:
- `Normal` mode (main session list, both Active and Archived tabs).
- `Library` mode and `Projects` mode — same logic, since they list the same `UnifiedSession` rows under different filters.

It does **not** render in `Digest`, `Help`, modal pickers, or settings.

## Architecture

### New module: `src/liveness.rs`

```rust
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Liveness {
    Idle,
    Recent,
    Live,
}

pub fn detect(session: &UnifiedSession) -> Liveness {
    match session.source {
        SessionSource::ClaudeCode => detect_cc(&session.path),
        SessionSource::OpenCode   => detect_oc(&session.path),
        SessionSource::Copilot    => detect_copilot(&session.path),
    }
}

const LIVE_WINDOW_SECS: u64 = 5;
const RECENT_WINDOW_SECS: u64 = 300;
```

### Per-source detection

Each source has its own log shape, so each gets its own helper:

#### `detect_cc(path: &Path) -> Liveness`

1. Stat the JSONL file. If mtime is older than `RECENT_WINDOW_SECS`, return `Idle`.
2. If mtime is older than `LIVE_WINDOW_SECS`, return `Recent` (no need to tail-parse).
3. Otherwise read the last 8 KB of the file. Parse JSONL backwards from the end:
   - Track the most recent `assistant` message's `tool_use` blocks.
   - Track which `tool_use_id`s have a corresponding `tool_result` further down (closer to EOF).
   - If any `tool_use_id` is missing its matching `tool_result`, return `Live`. Otherwise `Recent`.

#### `detect_copilot(path: &Path) -> Liveness`

The Copilot session directory contains `events.jsonl`. We stat that.

1. Same mtime gate as CC.
2. Tail parse `events.jsonl`. Look for the most recent `assistant.message` event without a closing `assistant.complete` (or whatever Copilot's terminator is — see `src/copilot_insights.rs` for the actual schema). If unclosed, `Live`; else `Recent`.

#### `detect_oc(path: &Path) -> Liveness`

1. Same mtime gate.
2. Tail parse the OpenCode log. Apply the same "unclosed turn" heuristic. The exact event names will mirror what `src/copilot_insights.rs` and `src/insights.rs` already extract — reuse those parsers when possible.

If a source's tail parser can't determine "open turn" cleanly, the function falls back to returning `Recent` (never `Live`) to avoid false-positive "live" indicators.

### AppState

```rust
// Cached liveness keyed by session_id. Updated by the polling task.
liveness_cache: Arc<Mutex<HashMap<String, Liveness>>>,

// Channel for liveness updates from the background task.
liveness_tx: tokio::sync::mpsc::UnboundedSender<HashMap<String, Liveness>>,
liveness_rx: tokio::sync::mpsc::UnboundedReceiver<HashMap<String, Liveness>>,
```

The list renderer reads `liveness_cache` each frame (cheap mutex lock) and renders the corresponding glyph.

### Polling task

Spawned once at startup:

```rust
let cache = state.liveness_cache.clone();
let tx = state.liveness_tx.clone();
// snapshot of which sessions are visible — pushed by the UI thread on scroll/selection change
let visible_ids = state.visible_session_ids.clone();   // Arc<Mutex<HashSet<String>>>
let sessions = state.sessions_handle.clone();           // Arc<Mutex<Vec<UnifiedSession>>>

tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        interval.tick().await;
        let visible = visible_ids.lock().clone();
        let snapshot = sessions.lock().clone();
        let updates = tokio::task::spawn_blocking(move || {
            let mut out = HashMap::new();
            for s in snapshot.iter().filter(|s| visible.contains(&s.id)) {
                out.insert(s.id.clone(), liveness::detect(s));
            }
            out
        }).await.unwrap_or_default();
        let _ = tx.send(updates);
    }
});
```

The event loop drains `liveness_rx` each frame and merges into `liveness_cache` (overwriting only the keys present — off-screen entries decay to `Idle` lazily when they re-enter the viewport, see "scroll-into-view check" below).

### Scroll-into-view one-shot check

When the visible-rows window changes (selection moved, scrolled), the UI thread updates `visible_session_ids` and additionally triggers a one-shot `tokio::spawn_blocking` to compute liveness for any newly-visible session_ids that aren't already in `liveness_cache`. This avoids the up-to-5s lag for rows that just scrolled into view.

### Idle decay

A session that was once `Live` but then drops off-screen will sit in `liveness_cache` as `Live` until its `session_id` next gets polled. To prevent stale `Live` glyphs from "reappearing" if the user scrolls past, we add an absolute timestamp to the cache:

```rust
struct CachedLiveness {
    state: Liveness,
    observed_at: Instant,
}
```

If the renderer reads a `Live` cached more than `LIVE_WINDOW_SECS` ago without an update, it downgrades to `Recent` for display purposes (does not mutate the cache — the next poll will overwrite).

## Reuse for #3

The streaming web view (#3) needs:
- Per-session `Liveness` for the dashboard list. → reads `liveness_cache`.
- Push events when liveness changes. → subscribes to a broadcast on top of `liveness_tx` (we'll add a `tokio::sync::broadcast::Sender<LivenessChange>` next to the existing mpsc, broadcasting the diff).

Both are additive. The polling task and detection functions are reused as-is.

## Testing

New suite at `tests/liveness_test.rs`:

| Test | Setup | Assertion |
|------|-------|-----------|
| `cc_idle_when_old_mtime` | Write CC JSONL, set mtime to 1 hour ago | `Idle` |
| `cc_recent_when_recent_mtime_closed_turn` | Write JSONL with closed assistant turn, mtime 30s ago | `Recent` |
| `cc_live_when_unclosed_tool_use` | Write JSONL ending mid-tool-use, mtime 1s ago | `Live` |
| `cc_recent_when_unclosed_but_old` | Write JSONL ending mid-tool-use, mtime 1 min ago | `Recent` (mtime gate fails before tail parse) |
| `cc_handles_truncated_tail` | Write JSONL where last 8KB starts mid-line | `Recent` (no panic) |
| `copilot_live_when_unclosed_assistant_event` | events.jsonl with `assistant.message` but no terminator, mtime 1s ago | `Live` |
| `copilot_recent_when_terminated` | events.jsonl with full assistant.message → tool.execution_complete sequence, mtime 30s ago | `Recent` |
| `oc_*` | Same shape as Copilot tests | (mirror) |
| `dispatch_routes_by_source` | Build a `UnifiedSession` for each source, ensure `detect` calls the right helper | (compile-time / behavioral) |
| `polling_only_visits_visible` | Instrumented detector counter; AppState with 100 sessions, 10 visible | Counter == 10, not 100 |

## Risks

- **Tail parse fragility:** if a source changes its log shape, our "unclosed turn" heuristic silently degrades to "always `Recent`" rather than panicking. Tests pin the current schema.
- **Mtime resolution on WSL2:** WSL2 mtime can have second-level precision rather than ms; the 5-second `live` window absorbs this.
- **8 KB tail isn't always enough:** a single huge assistant message could span 100KB+. If the `tool_use` opener is older than the last 8 KB, we'd miss it and return `Recent` (safe failure mode). Mitigation: bump tail to 32 KB if this turns out to matter; cost is still ~10ms.
- **Cache stale on backgrounded tab:** if the user has cc-speedy open but unfocused, the polling task still runs every 5s. Idle CPU cost is negligible since we only check visible rows. If the user scrolls a static viewport with a single live session for hours, that's 12 stat+tail-parse calls per minute — fine.

## Out-of-scope follow-ups

- FS watch (`notify` crate) to make detection push-based.
- Errored / stuck-state detection.
- Status-line summary like `live: 3 | recent: 5`.
- Per-session "duration in current turn" timer.
