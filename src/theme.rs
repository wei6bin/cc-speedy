use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::BorderType;

// ── Canvas & base text ───────────────────────────────────────────────
pub const BG: Color = Color::Rgb(30, 33, 36); // #1e2124  btop main_bg
pub const FG: Color = Color::Rgb(216, 216, 216); // #d8d8d8  btop main_fg
pub const FG_DIM: Color = Color::Rgb(89, 89, 89); // #595959  btop inactive_fg

// ── Accent ───────────────────────────────────────────────────────────
pub const TITLE: Color = Color::Rgb(0, 178, 255); // #00b2ff  btop title blue

// ── Panel border colors (unfocused) ──────────────────────────────────
pub const BORDER_LIST: Color = Color::Rgb(42, 97, 128); // #2a6180
pub const BORDER_PREVIEW: Color = Color::Rgb(30, 102, 128); // #1e6680
pub const BORDER_JOBS: Color = Color::Rgb(107, 79, 0); // #6b4f00
pub const BORDER_TOP: Color = Color::Rgb(74, 74, 106); // #4a4a6a
pub const BORDER_SETTINGS: Color = Color::Rgb(128, 0, 128); // magenta — settings popup

// ── Focused panel border (same as TITLE) ─────────────────────────────
pub const BORDER_FOCUSED: Color = TITLE;

// ── Selection ────────────────────────────────────────────────────────
pub const SEL_BG: Color = Color::Rgb(11, 51, 99); // #0b3363  btop hi_bg
pub const SEL_FG: Color = Color::Rgb(255, 255, 255); // #ffffff

// ── Badges ───────────────────────────────────────────────────────────
pub const CC_BADGE: Color = Color::Rgb(13, 131, 0); // #0d8300  muted btop green
pub const OC_BADGE: Color = Color::Rgb(30, 144, 255); // #1e90ff  btop blue
pub const CO_BADGE: Color = Color::Rgb(255, 140, 0); // #ff8c00  orange
pub const ARCHIVED_BADGE: Color = Color::Rgb(128, 128, 128); // #808080  gray

// ── Jobs panel content text ───────────────────────────────────────────
pub const JOBS_FG: Color = Color::Rgb(212, 160, 23); // #d4a017  warm amber

// ── Obsidian sync indicator ──────────────────────────────────────────
pub const OBSIDIAN_PURPLE: Color = Color::Rgb(124, 58, 237); // #7c3aed  obsidian brand

// ── Status bar ───────────────────────────────────────────────────────
pub const STATUS_OK: Color = TITLE; // flash message
pub const STATUS_HELP: Color = FG_DIM; // help text

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
    Style::default()
        .bg(SEL_BG)
        .fg(SEL_FG)
        .add_modifier(Modifier::BOLD)
}

/// Dim metadata text: #595959
pub fn dim_style() -> Style {
    Style::default().fg(FG_DIM)
}

/// Pin indicator: magenta + bold
pub fn pin_style() -> Style {
    Style::default()
        .fg(Color::Magenta)
        .add_modifier(Modifier::BOLD)
}

/// Pin popup border: magenta
pub fn pin_popup_style() -> Style {
    Style::default().fg(Color::Magenta)
}

/// Highlight style for grep-mode substring hits in the preview pane.
/// Yellow background + black text — high contrast but still readable.
pub fn grep_match_style() -> Style {
    Style::default()
        .fg(Color::Black)
        .bg(Color::Rgb(212, 160, 23))
        .add_modifier(Modifier::BOLD)
}
