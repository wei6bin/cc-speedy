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
| `BORDER_TOP` | `#4a4a6a` | Filter/top bar border |
| `BORDER_FOCUSED` | `#00b2ff` | Any focused panel border (= TITLE) |
| `SEL_BG` | `#0b3363` | List selection background (btop hi_bg) |
| `SEL_FG` | `#ffffff` | List selection text |
| `CC_BADGE` | `#0d8300` | [CC] badge — muted btop green |
| `OC_BADGE` | `#1e90ff` | [OC] badge — btop blue |
| `STATUS_OK` | `#00b2ff` | Flash status message (= TITLE) |
| `STATUS_HELP` | `#595959` | Help text in status bar (= FG_DIM) |

### Style Helpers

```rust
pub fn panel_block_style(border_color: Color) -> Style { ... }
pub fn title_style() -> Style { ... }  // TITLE color + BOLD
pub fn sel_style() -> Style { ... }    // SEL_BG + SEL_FG + BOLD
pub fn dim_style() -> Style { ... }    // FG_DIM color
```

### Border Type

All panels use `BorderType::Rounded` (`╭╮╰╯` corners).

---

## 2. Panel Borders & Focus States

Every `Block` gets:
- `border_type(theme::BORDER_TYPE)` — rounded corners
- `border_style` — panel's own muted color when unfocused, `BORDER_FOCUSED` when focused
- `title(Span::styled(..., theme::title_style()))` — `#00b2ff` bold title embedded in border

Focus rule: whichever panel has `app.focus` gets `BORDER_FOCUSED`; all others use their muted panel color.

| Panel | Unfocused Border | Focused Border |
|---|---|---|
| Filter / top bar | `#4a4a6a` | `#00b2ff` |
| Session list | `#2a6180` | `#00b2ff` |
| Preview | `#1e6680` | `#00b2ff` |
| Background jobs | `#6b4f00` | `#00b2ff` |

---

## 3. List Rows, Badges & Metadata

### Row Structure (5 spans)

1. **Timestamp** — `dim_style()` (`#595959`)
2. **Badge** — `CC_BADGE` (`#0d8300`) or `OC_BADGE` (`#1e90ff`)
3. **Title** — `FG` (`#d8d8d8`)
4. **Message count** — `dim_style()`
5. **Folder breadcrumb** — `dim_style()`

### Selection

```rust
.highlight_style(theme::sel_style())  // #0b3363 bg, white, bold
.highlight_symbol("▶ ")
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

This ensures gaps between panels and margins display the dark background rather than the terminal default.

---

## Implementation Notes

- All color changes are confined to `src/tui.rs` and the new `src/theme.rs`
- No changes to data model, session loading, summary generation, or any other module
- Truecolor terminal required for correct rendering (most modern terminals: iTerm2, Alacritty, WezTerm, Windows Terminal, kitty). Graceful degradation: colors map to nearest 256-color equivalent in non-truecolor terminals automatically by crossterm.
- `src/theme.rs` exported from `main.rs` as `mod theme`

---

## Files Changed

| File | Action |
|---|---|
| `src/theme.rs` | **New** — all color/style constants and helper functions |
| `src/tui.rs` | **Edit** — replace inline colors with theme constants, add rounded borders, background fill |
| `src/main.rs` | **Edit** — add `mod theme;` |
