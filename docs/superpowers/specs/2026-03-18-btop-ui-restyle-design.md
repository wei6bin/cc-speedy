# btop-Style UI Restyle — Design Spec

**Date:** 2026-03-18
**Scope:** Full visual restyle of cc-speedy TUI to match btop's aesthetic
**Approach:** Approach B — btop structure + preserved CC/OC semantic color meaning

---

## Goal

Adopt btop's visual language (dark canvas, rounded panels, muted panel borders, bright focus accent, embedded titles) while preserving the meaningful CC=green / OC=blue badge semantics already understood by users.

---

## 1. Theme Module (`src/theme.rs`)

New file centralizing all color and style constants. No other file defines colors inline.

### Color Palette (24-bit RGB)

| Constant | Hex | Purpose |
|---|---|---|
| `BG` | `#1e2124` | Dark canvas background (btop main_bg) |
| `FG` | `#d8d8d8` | Main text (btop main_fg) |
| `FG_DIM` | `#595959` | Metadata, inactive text (btop inactive_fg) |
| `TITLE` | `#00b2ff` | Panel title accent, flash message (btop title) |
| `BORDER_LIST` | `#2a6180` | Sessions panel border (unfocused) |
| `BORDER_PREVIEW` | `#1e6680` | Preview panel border (unfocused) |
| `BORDER_JOBS` | `#6b4f00` | Background jobs border (warning tone) |
| `JOBS_FG` | `#d4a017` | Background jobs content text — warm amber (replaces `Color::Yellow`) |
| `BORDER_TOP` | `#4a4a6a` | Filter/rename bar border (always unfocused — no Focus variant exists for top bar) |
| `BORDER_FOCUSED` | `#00b2ff` | Any focused panel border (= TITLE) |
| `SEL_BG` | `#0b3363` | List selection background (btop hi_bg) |
| `SEL_FG` | `#ffffff` | List selection text |
| `CC_BADGE` | `#0d8300` | [CC] badge — muted btop green (replaces `Color::Green`) |
| `OC_BADGE` | `#1e90ff` | [OC] badge — btop blue (intentional change from `Color::Cyan` for truecolor consistency) |
| `STATUS_OK` | `#00b2ff` | Flash status message (= TITLE) |
| `STATUS_HELP` | `#595959` | Help text in status bar (= FG_DIM) |

### Border Type Constant

```rust
use ratatui::widgets::BorderType;
pub const BORDER_TYPE: BorderType = BorderType::Rounded;  // ╭╮╰╯ corners
```

### Style Helpers

Each helper returns a `Style`. `panel_block_style` sets **border color only** (fg); it does not set bg or fg for content — those come from the background fill block (Section 5).

```rust
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

---

## 2. Panel Borders & Focus States

Every `Block` gets:
- `border_type(theme::BORDER_TYPE)` — rounded corners
- `border_style(theme::panel_block_style(...))` — muted color when unfocused, `BORDER_FOCUSED` when focused
- `title(Span::styled(..., theme::title_style()))` — `#00b2ff` bold title embedded in border

**Focus rule:** The `Focus` enum has two variants — `Focus::List` and `Focus::Preview`. Only these two panels switch to `BORDER_FOCUSED`. The filter/rename bar and jobs panel always use their fixed unfocused colors.

| Panel | `app.focus` variant | Unfocused Border | Focused Border |
|---|---|---|---|
| Filter / rename bar | *(no variant — always unfocused)* | `#4a4a6a` | N/A |
| Session list | `Focus::List` | `#2a6180` | `#00b2ff` |
| Preview | `Focus::Preview` | `#1e6680` | `#00b2ff` |
| Background jobs | *(no variant — always unfocused)* | `#6b4f00` | N/A |

The jobs panel `Paragraph` content uses `.style(Style::default().fg(theme::JOBS_FG))` (`#d4a017`) to preserve the warning-amber tone. Replace the existing `Color::Yellow` with `JOBS_FG`.

**Rename bar:** `AppMode::Rename` shares the same border styling as `AppMode::Filter` — both use `BORDER_TOP` (`#4a4a6a`). No styling difference between the two modes; only the title text changes (already handled by existing code).

---

## 3. List Rows, Badges & Metadata

### Row Structure (5 spans)

1. **Timestamp** — `dim_style()` (`#595959`)
2. **Badge** — `CC_BADGE` (`#0d8300`) or `OC_BADGE` (`#1e90ff`)
3. **Title** — `Style::default().fg(theme::FG)` (`#d8d8d8`)
4. **Message count** — `dim_style()`
5. **Folder breadcrumb** — `dim_style()`

### Selection

```rust
.highlight_style(theme::sel_style())  // #0b3363 bg, white, bold
.highlight_symbol("► ")               // keep existing U+25BA glyph (no change)
```

---

## 4. Status Bar

No border, 1 line tall. Two states:
- **Flash (2s):** `STATUS_OK` = `#00b2ff`
- **Help text:** `STATUS_HELP` = `#595959`

Logic unchanged from current implementation; only colors swapped to theme constants.

---

## 5. Background Fill

Before rendering any panel, fill the entire frame with a styled `Block` to paint the `#1e2124` dark canvas:

```rust
frame.render_widget(
    Block::default().style(Style::default().bg(theme::BG).fg(theme::FG)),
    frame.area(),
);
```

This ensures gaps between panels and margins display the dark background rather than the terminal default. Because `panel_block_style` sets border color only, panel interiors inherit `FG` from this background block.

---

## Implementation Notes

- All color changes confined to `src/tui.rs` and the new `src/theme.rs`
- No changes to data model, session loading, summary generation, or any other module
- Truecolor terminal required for correct rendering (iTerm2, Alacritty, WezTerm, Windows Terminal, kitty). Graceful degradation: crossterm maps to nearest 256-color in non-truecolor terminals automatically.
- After the refactor, `Color` in `src/tui.rs` becomes unused — remove it from the `use ratatui::style::{Color, Modifier, Style}` import. `Modifier` is encapsulated in `theme::sel_style()` and `theme::title_style()` — check if it remains needed after refactor and drop if not.

---

## Files Changed

| File | Action |
|---|---|
| `src/theme.rs` | **New** — all color/style constants and helper functions |
| `src/tui.rs` | **Edit** — replace inline colors with theme constants, add rounded borders, background fill, update imports |
| `src/lib.rs` | **Edit** — add `pub mod theme;` |
