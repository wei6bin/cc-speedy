use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Terminal,
};
use std::io::stdout;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use crate::unified::{list_all_sessions, UnifiedSession, SessionSource};
use crate::summary::{read_summary, summary_path, opencode_summary_path};
use crate::theme;

#[derive(PartialEq)]
enum Focus { List, Preview }

#[derive(PartialEq)]
enum AppMode { Normal, Filter, Rename }

struct AppState {
    sessions: Vec<UnifiedSession>,
    filtered: Vec<usize>,
    list_state: ListState,
    filter: String,
    mode: AppMode,
    rename_input: String,
    summaries: Arc<Mutex<std::collections::HashMap<String, String>>>,
    generating: Arc<Mutex<std::collections::HashSet<String>>>,
    focus: Focus,
    preview_scroll: u16,
    status_msg: Option<(String, Instant)>,
    source_filter: Option<SessionSource>,  // None = all, Some(CC) = CC only, Some(OC) = OC only
}

impl AppState {
    fn new(sessions: Vec<UnifiedSession>) -> Self {
        let n = sessions.len();
        let mut list_state = ListState::default();
        if n > 0 {
            list_state.select(Some(0));
        }
        Self {
            filtered: (0..n).collect(),
            sessions,
            list_state,
            filter: String::new(),
            mode: AppMode::Normal,
            rename_input: String::new(),
            summaries: Arc::new(Mutex::new(std::collections::HashMap::new())),
            generating: Arc::new(Mutex::new(std::collections::HashSet::new())),
            focus: Focus::List,
            preview_scroll: 0,
            status_msg: None,
            source_filter: None,
        }
    }

    fn apply_filter(&mut self) {
        let q = self.filter.to_lowercase();
        self.filtered = self
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                // Source filter
                if let Some(ref sf) = self.source_filter {
                    if &s.source != sf { return false; }
                }
                // Text filter
                q.is_empty()
                    || s.project_name.to_lowercase().contains(&q)
                    || s.summary.to_lowercase().contains(&q)
            })
            .map(|(i, _)| i)
            .collect();
        if !self.filtered.is_empty() {
            self.list_state.select(Some(0));
        } else {
            self.list_state.select(None);
        }
    }

    fn selected_session(&self) -> Option<&UnifiedSession> {
        let idx = self.list_state.selected()?;
        let raw = *self.filtered.get(idx)?;
        self.sessions.get(raw)
    }
}

pub async fn run() -> Result<()> {
    let sessions = list_all_sessions()?;

    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = AppState::new(sessions);

    // Pre-load existing summaries from disk (single lock acquisition)
    {
        let mut summaries = app.summaries.lock().unwrap_or_else(|e| e.into_inner());
        for session in &app.sessions {
            let path = match session.source {
                SessionSource::ClaudeCode => summary_path(&session.session_id),
                SessionSource::OpenCode   => opencode_summary_path(&session.session_id),
            };
            if let Some(content) = read_summary(&path) {
                summaries.insert(session.session_id.clone(), content);
            }
        }
    }

    // Run event loop, always clean up terminal regardless of result
    let result = run_event_loop(&mut terminal, &mut app).await;

    // Always clean up terminal
    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();

    result
}

async fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut AppState,
) -> Result<()> {
    loop {
        terminal.draw(|f| draw(f, app))?;

        if event::poll(std::time::Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                match (&app.mode, key.modifiers, key.code) {
                    // --- Global ---
                    (_, KeyModifiers::CONTROL, KeyCode::Char('c')) => break,
                    (AppMode::Normal, _, KeyCode::Char('q')) => break,

                    // --- Filter mode ---
                    (AppMode::Normal, _, KeyCode::Char('/')) => {
                        app.mode = AppMode::Filter;
                    }
                    (AppMode::Filter, _, KeyCode::Esc) => {
                        app.mode = AppMode::Normal;
                        app.filter.clear();
                        app.apply_filter();
                    }
                    (AppMode::Filter, _, KeyCode::Backspace) => {
                        app.filter.pop();
                        app.apply_filter();
                    }
                    (AppMode::Filter, _, KeyCode::Char(c)) => {
                        app.filter.push(c);
                        app.apply_filter();
                    }

                    // --- Rename mode ---
                    (AppMode::Normal, KeyModifiers::NONE, KeyCode::Char('r')) => {
                        if let Some(s) = app.selected_session() {
                            app.rename_input = s.summary.clone();
                            app.mode = AppMode::Rename;
                        }
                    }
                    (AppMode::Rename, _, KeyCode::Esc) => {
                        app.mode = AppMode::Normal;
                        app.rename_input.clear();
                    }
                    (AppMode::Rename, _, KeyCode::Backspace) => {
                        app.rename_input.pop();
                    }
                    (AppMode::Rename, _, KeyCode::Enter) => {
                        let title = app.rename_input.trim().to_string();
                        if !title.is_empty() {
                            if let Some(s) = app.selected_session() {
                                let id = s.session_id.clone();
                                let _ = crate::sessions::write_rename(&id, &title);
                                // Update in-memory immediately
                                if let Some(s) = app.filtered.get(
                                    app.list_state.selected().unwrap_or(0)
                                ).and_then(|&i| app.sessions.get_mut(i)) {
                                    s.summary = title;
                                }
                            }
                        }
                        app.mode = AppMode::Normal;
                        app.rename_input.clear();
                    }
                    (AppMode::Rename, _, KeyCode::Char(c)) => {
                        app.rename_input.push(c);
                    }

                    // --- Normal navigation ---
                    (AppMode::Normal, _, KeyCode::Tab) => {
                        app.focus = if app.focus == Focus::List { Focus::Preview } else { Focus::List };
                    }

                    (AppMode::Normal, _, KeyCode::Down) | (AppMode::Normal, _, KeyCode::Char('j')) => {
                        if app.focus == Focus::Preview {
                            app.preview_scroll = app.preview_scroll.saturating_add(1);
                        } else {
                            let n = app.filtered.len();
                            if n > 0 {
                                let i = app.list_state.selected().unwrap_or(0);
                                let next = (i + 1).min(n - 1);
                                if next != i { app.preview_scroll = 0; }
                                app.list_state.select(Some(next));
                            }
                        }
                    }
                    (AppMode::Normal, _, KeyCode::Up) | (AppMode::Normal, _, KeyCode::Char('k')) => {
                        if app.focus == Focus::Preview {
                            app.preview_scroll = app.preview_scroll.saturating_sub(1);
                        } else {
                            let i = app.list_state.selected().unwrap_or(0);
                            let prev = i.saturating_sub(1);
                            if prev != i { app.preview_scroll = 0; }
                            app.list_state.select(Some(prev));
                        }
                    }

                    // Ctrl+R: regenerate summary
                    (AppMode::Normal, KeyModifiers::CONTROL, KeyCode::Char('r')) => {
                        if let Some(s) = app.selected_session() {
                            let id = s.session_id.clone();
                            let jsonl = s.jsonl_path.clone();
                            let source = s.source.clone();
                            let summaries = app.summaries.clone();
                            let generating = app.generating.clone();
                    // Clear any cached (possibly stale) summary then kick off generation.
                    // Do NOT pre-remove from `generating`: spawn_summary_generation
                    // inserts the id immediately, so removing here creates a brief
                    // window where the session appears idle.
                    app.summaries.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
                    spawn_summary_generation(id, jsonl, source, summaries, generating);
                        }
                    }

                    // Source filter keys
                    (AppMode::Normal, KeyModifiers::NONE, KeyCode::Char('1')) => {
                        app.source_filter = Some(SessionSource::ClaudeCode);
                        app.apply_filter();
                    }
                    (AppMode::Normal, KeyModifiers::NONE, KeyCode::Char('2')) => {
                        app.source_filter = Some(SessionSource::OpenCode);
                        app.apply_filter();
                    }
                    (AppMode::Normal, KeyModifiers::NONE, KeyCode::Char('0')) => {
                        app.source_filter = None;
                        app.apply_filter();
                    }

                    (AppMode::Normal, _, KeyCode::Enter) => {
                        if let Some(s) = app.selected_session() {
                            let path  = s.project_path.clone();
                            let id    = s.session_id.clone();
                            let title = window_title_from_session(s);
                            match s.source {
                                SessionSource::ClaudeCode => {
                                    let name = crate::tmux::cc_session_name(&path);
                                    return crate::tmux::resume_in_tmux(&name, &path, &id, false, &title);
                                }
                                SessionSource::OpenCode => {
                                    let name = crate::tmux::oc_session_name(&path);
                                    return crate::tmux::resume_opencode_in_tmux(&name, &path, &id, &title);
                                }
                            }
                        }
                    }

                    // Ctrl+Y: yolo mode
                    (AppMode::Normal, KeyModifiers::CONTROL, KeyCode::Char('y')) => {
                        if let Some(s) = app.selected_session() {
                            let path  = s.project_path.clone();
                            let id    = s.session_id.clone();
                            let title = window_title_from_session(s);
                            match s.source {
                                SessionSource::ClaudeCode => {
                                    let name = crate::tmux::cc_session_name(&path);
                                    return crate::tmux::resume_in_tmux(&name, &path, &id, true, &title);
                                }
                                SessionSource::OpenCode => {
                                    // OpenCode has no --dangerously-skip-permissions; fall back to normal resume
                                    let name = crate::tmux::oc_session_name(&path);
                                    return crate::tmux::resume_opencode_in_tmux(&name, &path, &id, &title);
                                }
                            }
                        }
                    }

                    // c: copy summary to clipboard
                    (AppMode::Normal, KeyModifiers::NONE, KeyCode::Char('c')) => {
                        let content = build_preview_content(app);
                        let msg = match copy_to_clipboard(&content) {
                            Ok(_)  => "Copied to clipboard".to_string(),
                            Err(e) => format!("Copy failed: {}", e),
                        };
                        app.status_msg = Some((msg, Instant::now()));
                    }

                    _ => {}
                }
            }
        }

        // Summary generation is manual only — use Ctrl+R to generate for selected session
    }
    Ok(())
}

fn spawn_summary_generation(
    id: String,
    jsonl: Option<String>,   // Some for CC sessions, None for OC sessions
    source: SessionSource,
    summaries: Arc<Mutex<std::collections::HashMap<String, String>>>,
    generating: Arc<Mutex<std::collections::HashSet<String>>>,
) {
    generating.lock().unwrap_or_else(|e| e.into_inner()).insert(id.clone());
    tokio::spawn(async move {
        // Fetch messages: CC reads from JSONL file, OC queries SQLite directly
        let msgs = match source {
            SessionSource::ClaudeCode => {
                let Some(jsonl_path) = jsonl else {
                    generating.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
                    return;
                };
                tokio::task::spawn_blocking({
                    let p = jsonl_path.clone();
                    move || crate::sessions::parse_messages(std::path::Path::new(&p))
                }).await.ok().and_then(|r| r.ok())
            }
            SessionSource::OpenCode => {
                let session_id = id.clone();
                tokio::task::spawn_blocking(move || {
                    crate::opencode_sessions::parse_opencode_messages(&session_id)
                }).await.ok().and_then(|r| r.ok())
            }
        };

        if let Some(msgs) = msgs {
            match crate::summary::generate_summary(&msgs).await {
                Ok(text) => {
                    let out = match source {
                        SessionSource::ClaudeCode => crate::summary::summary_path(&id),
                        SessionSource::OpenCode   => crate::summary::opencode_summary_path(&id),
                    };
                    let _ = crate::summary::write_summary(&out, &text);
                    summaries.lock().unwrap_or_else(|e| e.into_inner()).insert(id.clone(), text);
                }
                Err(e) => {
                    summaries
                        .lock()
                        .expect("summary mutex poisoned")
                        .insert(id.clone(), format!("Error generating summary: {}", e));
                }
            }
        }
        generating.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
    });
}

fn draw(f: &mut ratatui::Frame, app: &mut AppState) {
    let area = f.area();
    // Paint dark canvas before any panels (btop #1e2124 background)
    f.render_widget(
        Block::default().style(Style::default().bg(theme::BG).fg(theme::FG)),
        area,
    );

    const MAX_JOB_LINES: usize = 3;
    let jobs: Vec<String> = {
        let gen = app.generating.lock().unwrap_or_else(|e| e.into_inner());
        let sum = app.summaries.lock().unwrap_or_else(|e| e.into_inner());
        let mut items: Vec<String> = gen.iter().map(|id| {
            let label = app.sessions.iter()
                .find(|s| &s.session_id == id)
                .map(|s| if !s.summary.is_empty() { s.summary.clone() } else { s.project_name.clone() })
                .unwrap_or_else(|| id[..8.min(id.len())].to_string());
            let status = sum.get(id).map(|v| v.as_str()).unwrap_or("waiting...");
            format!("⟳  {} — {}", label, status)
        }).collect();
        if items.len() > MAX_JOB_LINES {
            let extra = items.len() - MAX_JOB_LINES;
            items.truncate(MAX_JOB_LINES);
            items.push(format!("   … and {} more", extra));
        }
        items
    };
    let jobs_height = if jobs.is_empty() { 0 } else { (jobs.len() + 2) as u16 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(jobs_height),
            Constraint::Length(1),
        ])
        .split(area);

    // Top bar: filter / rename input / idle hint
    let (bar_text, bar_title) = match &app.mode {
        AppMode::Filter => (format!("> {}|", app.filter), " Filter "),
        AppMode::Rename => (format!("rename: {}|", app.rename_input), " Rename  [Enter: confirm  Esc: cancel] "),
        AppMode::Normal => {
            let hint = if app.filter.is_empty() {
                "  (press / to filter)".to_string()
            } else {
                format!("  filter: {}", app.filter)
            };
            (hint, " cc-speedy ")
        }
    };
    let filter_block = Paragraph::new(bar_text)
        .block(
            Block::default()
                .border_type(theme::BORDER_TYPE)
                .borders(Borders::ALL)
                .border_style(theme::panel_block_style(theme::BORDER_TOP))
                .title(Span::styled(bar_title, theme::title_style())),
        );
    f.render_widget(filter_block, chunks[0]);

    // Main panes
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(chunks[1]);

    draw_list(f, app, panes[0]);
    draw_preview(f, app, panes[1], app.preview_scroll);

    // Background jobs panel
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

    // Status bar: show timed flash message, or the key hint
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
}

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

fn build_preview_content(app: &AppState) -> String {
    match app.selected_session() {
        None => "No session selected".to_string(),
        Some(s) => {
            let fallback = if !s.summary.is_empty() {
                s.summary.clone()
            } else {
                "[press r to generate summary]".to_string()
            };
            let summary = app
                .summaries
                .lock()
                .expect("summary mutex poisoned")
                .get(&s.session_id)
                .cloned()
                .unwrap_or(fallback);

            let branch_line = if !s.git_branch.is_empty() {
                format!("\nBRANCH:   {}", s.git_branch)
            } else {
                String::new()
            };

            let first_msg_line = if !s.first_user_msg.is_empty() {
                format!("\nFIRST:    {}", s.first_user_msg)
            } else {
                String::new()
            };

            let generated_line = {
                let path = match s.source {
                    SessionSource::ClaudeCode => crate::summary::summary_path(&s.session_id),
                    SessionSource::OpenCode   => crate::summary::opencode_summary_path(&s.session_id),
                };
                match std::fs::metadata(&path).and_then(|m| m.modified()) {
                    Ok(t) => format!("\n\n─── generated {} ───", format_time(t)),
                    Err(_) => String::new(),
                }
            };

            format!(
                "PROJECT:  {}\nMSGS:     {}  |  {}{}{}\n\n{}{}",
                s.project_path,
                s.message_count,
                format_time(s.modified),
                branch_line,
                first_msg_line,
                summary,
                generated_line,
            )
        }
    }
}

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

fn format_time(t: std::time::SystemTime) -> String {
    let secs = t
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    chrono::DateTime::from_timestamp(secs as i64, 0)
        .map(|utc| utc.with_timezone(&chrono::Local))
        .unwrap_or_default()
        .format("%m-%d %H:%M")
        .to_string()
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    }
}

fn window_title_from_session(s: &UnifiedSession) -> String {
    let label = if !s.summary.is_empty() { &s.summary } else { &s.project_name };
    truncate(label, 10)
}

/// Copy text to the system clipboard.
/// Tries clip.exe (WSL), xclip, xsel, pbcopy in order.
fn copy_to_clipboard(text: &str) -> anyhow::Result<()> {
    let candidates: &[(&str, &[&str])] = &[
        ("clip.exe",  &[]),
        ("xclip",     &["-selection", "clipboard"]),
        ("xsel",      &["--clipboard", "--input"]),
        ("pbcopy",    &[]),
    ];
    for (cmd, args) in candidates {
        let mut child = match std::process::Command::new(cmd)
            .args(*args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(_) => continue,
        };
        // Spawn a thread to write stdin so that `child.wait()` can drain the
        // process's stdout/stderr concurrently, preventing pipe-buffer deadlocks
        // on large summaries.
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let bytes = text.as_bytes().to_vec();
            std::thread::spawn(move || { let _ = stdin.write_all(&bytes); });
        }
        let status = child.wait()?;
        if status.success() {
            return Ok(());
        }
    }
    anyhow::bail!("no clipboard tool found (tried clip.exe, xclip, xsel, pbcopy)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_short_string_unchanged() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_exact_length_unchanged() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_long_string_adds_ellipsis() {
        let result = truncate("abcdefghij", 5);
        assert_eq!(result.chars().count(), 5);
        assert!(result.ends_with('…'));
    }

    #[test]
    fn test_truncate_unicode_counts_chars_not_bytes() {
        // Each emoji is 1 char; truncate to 3 should keep 2 chars + ellipsis
        let s = "😀😁😂😃😄";
        let result = truncate(s, 3);
        assert_eq!(result.chars().count(), 3);
    }

    #[test]
    fn test_format_time_produces_month_day_hhmm() {
        // epoch 0 = 1970-01-01 00:00:00 UTC; local offset may shift the hour but
        // the format must always be MM-DD HH:MM (10 chars)
        let t = std::time::UNIX_EPOCH;
        let s = format_time(t);
        assert_eq!(s.len(), 11, "expected 'MM-DD HH:MM', got: {}", s);
        assert_eq!(&s[2..3], "-");
        assert_eq!(&s[5..6], " ");
        assert_eq!(&s[8..9], ":");
    }
}

