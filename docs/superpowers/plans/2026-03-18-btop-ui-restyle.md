# btop-Style UI Restyle Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restyle cc-speedy's TUI to match btop's aesthetic — dark canvas, rounded panels, muted per-panel borders, bright focus accent, embedded titles, 24-bit RGB color throughout.

**Architecture:** Create `src/theme.rs` as the single source of all colors/styles; register it in `src/lib.rs`; update `src/tui.rs` to use theme constants, add rounded `BorderType`, a full-frame background fill, and replace all inline `Color::*` / `Modifier::*` with theme references.

**Tech Stack:** Rust, ratatui v0.29, crossterm v0.28. No new dependencies needed — `Color::Rgb` and `BorderType::Rounded` are already available in ratatui 0.29.

**Spec:** `docs/superpowers/specs/2026-03-18-btop-ui-restyle-design.md`

**Important:** Every commit in this plan leaves the project in a **compiling state**. `Color` and `Modifier` are NOT removed from `tui.rs` imports until Task 8, after all inline usages have been replaced.

---

## Chunk 1: Theme Module & Foundation

### Task 1: Create `src/theme.rs`

**Files:**
- Create: `src/theme.rs`

- [ ] **Step 1: Write `src/theme.rs` with all constants and helpers**

```rust
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::BorderType;

// ── Canvas & base text ───────────────────────────────────────────────
pub const BG:      Color = Color::Rgb(30, 33, 36);    // #1e2124  btop main_bg
pub const FG:      Color = Color::Rgb(216, 216, 216); // #d8d8d8  btop main_fg
pub const FG_DIM:  Color = Color::Rgb(89, 89, 89);    // #595959  btop inactive_fg

// ── Accent ───────────────────────────────────────────────────────────
pub const TITLE:   Color = Color::Rgb(0, 178, 255);   // #00b2ff  btop title blue

// ── Panel border colors (unfocused) ──────────────────────────────────
pub const BORDER_LIST:    Color = Color::Rgb(42, 97, 128);   // #2a6180
pub const BORDER_PREVIEW: Color = Color::Rgb(30, 102, 128);  // #1e6680
pub const BORDER_JOBS:    Color = Color::Rgb(107, 79, 0);    // #6b4f00
pub const BORDER_TOP:     Color = Color::Rgb(74, 74, 106);   // #4a4a6a

// ── Focused panel border (same as TITLE) ─────────────────────────────
pub const BORDER_FOCUSED: Color = TITLE;

// ── Selection ────────────────────────────────────────────────────────
pub const SEL_BG: Color = Color::Rgb(11, 51, 99);    // #0b3363  btop hi_bg
pub const SEL_FG: Color = Color::Rgb(255, 255, 255); // #ffffff

// ── Badges ───────────────────────────────────────────────────────────
pub const CC_BADGE: Color = Color::Rgb(13, 131, 0);   // #0d8300  muted btop green
pub const OC_BADGE: Color = Color::Rgb(30, 144, 255); // #1e90ff  btop blue

// ── Jobs panel content text ───────────────────────────────────────────
pub const JOBS_FG: Color = Color::Rgb(212, 160, 23);  // #d4a017  warm amber

// ── Status bar ───────────────────────────────────────────────────────
pub const STATUS_OK:   Color = TITLE;    // flash message
pub const STATUS_HELP: Color = FG_DIM;  // help text

// ── Border type for all panels ───────────────────────────────────────
pub const BORDER_TYPE: BorderType = BorderType::Rounded; // ╭╮╰╯

// ── Style helpers ────────────────────────────────────────────────────

/// Border color only — pass to .border_style()
pub fn panel_block_style(border_color: Color) -> Style {
    Style::default().fg(border_color)
}

/// #00b2ff + BOLD — for panel titles embedded in borders
pub fn title_style() -> Style {
    Style::default().fg(TITLE).add_modifier(Modifier::BOLD)
}

/// Selection highlight: #0b3363 bg + white + BOLD
pub fn sel_style() -> Style {
    Style::default().bg(SEL_BG).fg(SEL_FG).add_modifier(Modifier::BOLD)
}

/// Dim metadata text: #595959
pub fn dim_style() -> Style {
    Style::default().fg(FG_DIM)
}
```

- [ ] **Step 2: Register `theme` in `src/lib.rs`**

Add `pub mod theme;` to `src/lib.rs`:

```rust
pub mod sessions;
pub mod summary;
pub mod tmux;
pub mod install;
pub mod tui;
pub mod unified;
pub mod opencode_sessions;
pub mod util;
pub mod theme;
```

- [ ] **Step 3: Verify it compiles**

```bash
cargo check 2>&1
```

Expected: no errors (only possible unused-constant warnings, which are fine).

- [ ] **Step 4: Commit**

```bash
git add src/theme.rs src/lib.rs
git commit -m "feat: add theme module with btop-style 24-bit RGB palette"
```

---

### Task 2: Add `use crate::theme` import and background fill to `draw()`

**Files:**
- Modify: `src/tui.rs`

Note: `Color` and `Modifier` stay in `tui.rs` imports for now. They are removed in Task 8 once all inline usages are gone.

- [ ] **Step 1: Add `use crate::theme;` import**

In `src/tui.rs`, find these two lines (around lines 18–19):
```rust
use crate::unified::{list_all_sessions, UnifiedSession, SessionSource};
use crate::summary::{read_summary, summary_path, opencode_summary_path};
```

Add `use crate::theme;` immediately after:
```rust
use crate::unified::{list_all_sessions, UnifiedSession, SessionSource};
use crate::summary::{read_summary, summary_path, opencode_summary_path};
use crate::theme;
```

- [ ] **Step 2: Add `BorderType` to the ratatui widgets import**

Find (line 12):
```rust
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
```

Replace with:
```rust
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap},
```

- [ ] **Step 3: Add background fill at the top of `draw()`**

Find the `draw` function at line 369:
```rust
fn draw(f: &mut ratatui::Frame, app: &mut AppState) {
    let area = f.area();
```

After `let area = f.area();`, add:
```rust
    // Paint dark canvas before any panels (btop #1e2124 background)
    f.render_widget(
        Block::default().style(Style::default().bg(theme::BG).fg(theme::FG)),
        area,
    );
```

- [ ] **Step 4: Verify it compiles**

```bash
cargo check 2>&1
```

Expected: **no errors**. `Color`, `Modifier`, `BorderType` are all still imported. The background fill compiles cleanly.

- [ ] **Step 5: Commit**

```bash
git add src/tui.rs
git commit -m "feat: add btop background fill and theme import to tui"
```

---

## Chunk 2: Panel Styling

### Task 3: Restyle the top bar (filter / rename)

**Files:**
- Modify: `src/tui.rs` lines 416–418

- [ ] **Step 1: Replace the filter block construction**

Find (lines 416–418):
```rust
    let filter_block = Paragraph::new(bar_text)
        .block(Block::default().borders(Borders::ALL).title(bar_title));
    f.render_widget(filter_block, chunks[0]);
```

Replace with:
```rust
    let filter_block = Paragraph::new(bar_text)
        .block(
            Block::default()
                .border_type(theme::BORDER_TYPE)
                .borders(Borders::ALL)
                .border_style(theme::panel_block_style(theme::BORDER_TOP))
                .title(Span::styled(bar_title, theme::title_style())),
        );
    f.render_widget(filter_block, chunks[0]);
```

This applies the same `BORDER_TOP` color for all three modes (`Normal`, `Filter`, `Rename`) since the same widget handles all three.

- [ ] **Step 2: Verify it compiles**

```bash
cargo check 2>&1
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/tui.rs
git commit -m "feat: apply btop styling to top bar panel"
```

---

### Task 4: Restyle the jobs panel

**Files:**
- Modify: `src/tui.rs` lines 430–438

- [ ] **Step 1: Replace jobs panel block**

Find (lines 430–438):
```rust
    if jobs_height > 0 {
        let text = jobs.join("\n");
        let jobs_panel = Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL)
                .title(" Background ")
                .border_style(Style::default().fg(Color::Yellow)))
            .style(Style::default().fg(Color::Yellow));
        f.render_widget(jobs_panel, chunks[2]);
    }
```

Replace with:
```rust
    if jobs_height > 0 {
        let text = jobs.join("\n");
        let jobs_panel = Paragraph::new(text)
            .block(
                Block::default()
                    .border_type(theme::BORDER_TYPE)
                    .borders(Borders::ALL)
                    .border_style(theme::panel_block_style(theme::BORDER_JOBS))
                    .title(Span::styled(" Background ", theme::title_style())),
            )
            .style(Style::default().fg(theme::JOBS_FG));
        f.render_widget(jobs_panel, chunks[2]);
    }
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo check 2>&1
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/tui.rs
git commit -m "feat: apply btop styling to background jobs panel"
```

---

### Task 5: Restyle the status bar

**Files:**
- Modify: `src/tui.rs` lines 441–452

- [ ] **Step 1: Replace status bar color references**

Find (lines 441–452):
```rust
    let (status_text, status_style) = if let Some((msg, at)) = &app.status_msg {
        if at.elapsed().as_secs() < 2 {
            (msg.as_str(), Style::default().fg(Color::Green))
        } else {
            (" 1:CC  2:OC  0:all  /: filter  Enter: resume  Ctrl+Y: yolo  Tab  j/k  r  c  Ctrl+R  q",
             Style::default().fg(Color::DarkGray))
        }
    } else {
        (" 1:CC  2:OC  0:all  /: filter  Enter: resume  Ctrl+Y: yolo  Tab  j/k  r  c  Ctrl+R  q",
         Style::default().fg(Color::DarkGray))
    };
    f.render_widget(Paragraph::new(status_text).style(status_style), chunks[3]);
```

Replace with:
```rust
    let (status_text, status_style) = if let Some((msg, at)) = &app.status_msg {
        if at.elapsed().as_secs() < 2 {
            (msg.as_str(), Style::default().fg(theme::STATUS_OK))
        } else {
            (" 1:CC  2:OC  0:all  /: filter  Enter: resume  Ctrl+Y: yolo  Tab  j/k  r  c  Ctrl+R  q",
             Style::default().fg(theme::STATUS_HELP))
        }
    } else {
        (" 1:CC  2:OC  0:all  /: filter  Enter: resume  Ctrl+Y: yolo  Tab  j/k  r  c  Ctrl+R  q",
         Style::default().fg(theme::STATUS_HELP))
    };
    f.render_widget(Paragraph::new(status_text).style(status_style), chunks[3]);
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo check 2>&1
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/tui.rs
git commit -m "feat: apply btop styling to status bar"
```

---

### Task 6: Restyle the session list rows and block

**Files:**
- Modify: `src/tui.rs` lines 455–499 (`draw_list` function)

- [ ] **Step 1: Replace `draw_list` function entirely**

Find the entire `draw_list` function (lines 455–499) and replace with:

```rust
fn draw_list(f: &mut ratatui::Frame, app: &mut AppState, area: Rect) {
    let items: Vec<ListItem> = app
        .filtered
        .iter()
        .map(|&i| {
            let s = &app.sessions[i];
            let dt = format_time(s.modified);
            let folder = crate::util::path_last_n(&s.project_path, 3);
            let label = if s.summary.is_empty() {
                truncate(&format!("[{}]", s.project_name), 21)
            } else {
                truncate(&s.summary, 21)
            };
            let (badge_text, badge_color) = match s.source {
                SessionSource::ClaudeCode => ("[CC]", theme::CC_BADGE),
                SessionSource::OpenCode   => ("[OC]", theme::OC_BADGE),
            };
            let line = Line::from(vec![
                Span::styled(format!("{} ", dt), theme::dim_style()),
                Span::styled(format!("{} ", badge_text), Style::default().fg(badge_color)),
                Span::styled(format!("{:<22}", label), Style::default().fg(theme::FG)),
                Span::styled(format!("{:>4} ", s.message_count), theme::dim_style()),
                Span::styled(folder, theme::dim_style()),
            ]);
            ListItem::new(line)
        })
        .collect();

    let count = items.len();
    let focused = app.focus == Focus::List;
    let border_color = if focused { theme::BORDER_FOCUSED } else { theme::BORDER_LIST };
    let list = List::new(items)
        .block(
            Block::default()
                .border_type(theme::BORDER_TYPE)
                .borders(Borders::ALL)
                .border_style(theme::panel_block_style(border_color))
                .title(Span::styled(
                    format!(" Sessions ({}) ", count),
                    theme::title_style(),
                )),
        )
        .highlight_style(theme::sel_style())
        .highlight_symbol("► ");

    f.render_stateful_widget(list, area, &mut app.list_state);
}
```

Note: The label span uses `Span::styled(..., Style::default().fg(theme::FG))` (not `Span::raw`) to match the spec's explicit `#d8d8d8` requirement for title text.

- [ ] **Step 2: Verify it compiles**

```bash
cargo check 2>&1
```

Expected: no errors. The `Modifier::BOLD` at the old line 494 is now gone (replaced by `theme::sel_style()`).

- [ ] **Step 3: Commit**

```bash
git add src/tui.rs
git commit -m "feat: apply btop styling to session list rows and panel"
```

---

### Task 7: Restyle the preview panel

**Files:**
- Modify: `src/tui.rs` lines 555–568 (`draw_preview` function)

- [ ] **Step 1: Replace `draw_preview` function**

Find the entire `draw_preview` function (lines 555–568) and replace with:

```rust
fn draw_preview(f: &mut ratatui::Frame, app: &mut AppState, area: Rect, scroll: u16) {
    let content = build_preview_content(app);

    let focused = app.focus == Focus::Preview;
    let border_color = if focused { theme::BORDER_FOCUSED } else { theme::BORDER_PREVIEW };
    let block = Block::default()
        .border_type(theme::BORDER_TYPE)
        .borders(Borders::ALL)
        .border_style(theme::panel_block_style(border_color))
        .title(Span::styled(
            if focused { " Summary  [Tab: back to list] " } else { " Summary  [Tab: scroll] " },
            theme::title_style(),
        ));
    let preview = Paragraph::new(content)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    f.render_widget(preview, area);
}
```

- [ ] **Step 2: Run `cargo check` — expect clean**

```bash
cargo check 2>&1
```

Expected: **no errors**. All `Color::*` and `Modifier::*` usages in `draw_list` and `draw_preview` are now gone. The only remaining usages are in `tui.rs`'s import lines (which haven't been cleaned up yet — that's Task 8).

- [ ] **Step 3: Run tests**

```bash
cargo test 2>&1
```

Expected: all existing tests pass (`truncate` and `format_time` tests are unchanged).

- [ ] **Step 4: Commit**

```bash
git add src/tui.rs
git commit -m "feat: apply btop styling to preview panel — restyle complete"
```

---

## Chunk 3: Final Cleanup

### Task 8: Remove unused imports and verify

**Files:**
- Modify: `src/tui.rs` lines 7–14 (import block)

- [ ] **Step 1: Verify no inline Color/Modifier usages remain**

```bash
grep -n "Color::\|Modifier::" src/tui.rs
```

Expected: **no output** — all have been replaced by theme constants.

- [ ] **Step 2: Remove `Color` and `Modifier` from the ratatui style import**

Find (lines 7–14):
```rust
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Terminal,
};
```

Replace with:
```rust
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Terminal,
};
```

- [ ] **Step 3: Run `cargo build` — expect clean with no warnings**

```bash
cargo build 2>&1
```

Expected: clean build, no `unused import` warnings for `Color` or `Modifier`.

- [ ] **Step 4: Run full test suite**

```bash
cargo test 2>&1
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/tui.rs
git commit -m "chore: remove unused Color/Modifier imports from tui.rs"
```

---

## Visual Verification

After building, run cc-speedy in a truecolor terminal (Alacritty, WezTerm, kitty, iTerm2, Windows Terminal):

```bash
cargo run 2>&1
```

Checklist:
- [ ] Dark `#1e2124` canvas fills the terminal background
- [ ] All panel borders are rounded (`╭╮╰╯`)
- [ ] Panel titles are bright blue (`#00b2ff`) and bold
- [ ] Sessions panel border: muted steel blue (`#2a6180`) when unfocused
- [ ] Sessions panel border: bright blue (`#00b2ff`) when `Focus::List`
- [ ] Preview panel border: muted teal (`#1e6680`) when unfocused
- [ ] Preview panel border: bright blue (`#00b2ff`) when `Focus::Preview`
- [ ] Tab toggles focus correctly — active panel glows blue
- [ ] `[CC]` badges are muted green (`#0d8300`)
- [ ] `[OC]` badges are blue (`#1e90ff`)
- [ ] Selected row: dark navy background (`#0b3363`), white text, bold
- [ ] Metadata (timestamps, counts, paths) is dim gray (`#595959`)
- [ ] Jobs panel (trigger with Ctrl+R): dark amber border + warm amber text
- [ ] Status bar flash (copy with `c`): bright blue message (`#00b2ff`)
- [ ] Status bar help text: dim gray (`#595959`)
- [ ] Filter bar border: muted purple-gray (`#4a4a6a`) in all modes
