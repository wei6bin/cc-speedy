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
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Terminal,
};
use std::io::stdout;
use std::sync::{Arc, Mutex};
use crate::sessions::{list_sessions, Session};
use crate::summary::{read_summary, summary_path};

#[derive(PartialEq)]
enum Focus { List, Preview }

struct AppState {
    sessions: Vec<Session>,
    filtered: Vec<usize>,
    list_state: ListState,
    filter: String,
    filter_mode: bool,
    summaries: Arc<Mutex<std::collections::HashMap<String, String>>>,
    generating: Arc<Mutex<std::collections::HashSet<String>>>,
    focus: Focus,
    preview_scroll: u16,
}

impl AppState {
    fn new(sessions: Vec<Session>) -> Self {
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
            filter_mode: false,
            summaries: Arc::new(Mutex::new(std::collections::HashMap::new())),
            generating: Arc::new(Mutex::new(std::collections::HashSet::new())),
            focus: Focus::List,
            preview_scroll: 0,
        }
    }

    fn apply_filter(&mut self) {
        let q = self.filter.to_lowercase();
        self.filtered = self
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| {
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

    fn selected_session(&self) -> Option<&Session> {
        let idx = self.list_state.selected()?;
        let raw = *self.filtered.get(idx)?;
        self.sessions.get(raw)
    }
}

pub async fn run() -> Result<()> {
    let sessions = list_sessions()?;

    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = AppState::new(sessions);

    // Pre-load existing summaries from disk
    for session in &app.sessions {
        let path = summary_path(&session.session_id);
        if let Some(content) = read_summary(&path) {
            app.summaries
                .lock()
                .expect("summary mutex poisoned")
                .insert(session.session_id.clone(), content);
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
                match (key.modifiers, key.code) {
                    (_, KeyCode::Char('q')) if !app.filter_mode => break,
                    (KeyModifiers::CONTROL, KeyCode::Char('c')) => break,

                    (_, KeyCode::Char('/')) if !app.filter_mode => {
                        app.filter_mode = true;
                    }
                    (_, KeyCode::Esc) if app.filter_mode => {
                        app.filter_mode = false;
                        app.filter.clear();
                        app.apply_filter();
                    }
                    (_, KeyCode::Backspace) if app.filter_mode => {
                        app.filter.pop();
                        app.apply_filter();
                    }
                    (_, KeyCode::Char(c)) if app.filter_mode => {
                        app.filter.push(c);
                        app.apply_filter();
                    }

                    (_, KeyCode::Tab) if !app.filter_mode => {
                        app.focus = if app.focus == Focus::List { Focus::Preview } else { Focus::List };
                    }

                    (_, KeyCode::Down) | (_, KeyCode::Char('j')) if !app.filter_mode => {
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
                    (_, KeyCode::Up) | (_, KeyCode::Char('k')) if !app.filter_mode => {
                        if app.focus == Focus::Preview {
                            app.preview_scroll = app.preview_scroll.saturating_sub(1);
                        } else {
                            let i = app.list_state.selected().unwrap_or(0);
                            let prev = i.saturating_sub(1);
                            if prev != i { app.preview_scroll = 0; }
                            app.list_state.select(Some(prev));
                        }
                    }

                    (_, KeyCode::Char('r')) if !app.filter_mode => {
                        if let Some(s) = app.selected_session() {
                            let id = s.session_id.clone();
                            let jsonl = s.jsonl_path.clone();
                            let summaries = app.summaries.clone();
                            let generating = app.generating.clone();
                            app.summaries.lock().expect("summary mutex poisoned").remove(&id);
                            app.generating.lock().expect("generating mutex poisoned").remove(&id);
                            spawn_summary_generation(id, jsonl, summaries, generating);
                        }
                    }

                    (_, KeyCode::Enter) if !app.filter_mode => {
                        if let Some(s) = app.selected_session() {
                            let name = crate::tmux::session_name_from_path(&s.project_path);
                            let path = s.project_path.clone();
                            let id = s.session_id.clone();
                            return crate::tmux::resume_in_tmux(&name, &path, &id, false);
                        }
                    }

                    (KeyModifiers::CONTROL, KeyCode::Enter) if !app.filter_mode => {
                        if let Some(s) = app.selected_session() {
                            let name = crate::tmux::session_name_from_path(&s.project_path);
                            let path = s.project_path.clone();
                            let id = s.session_id.clone();
                            return crate::tmux::resume_in_tmux(&name, &path, &id, true);
                        }
                    }

                    _ => {}
                }
            }
        }

        // Auto-trigger summary for small sessions (≤20 msgs); larger sessions require manual `r`
        // Throttle: max 5 concurrent background processes (no queue — drop if at limit)
        if let Some(s) = app.selected_session() {
            if s.message_count <= 20 {
                let id = s.session_id.clone();
                let (has_summary, is_generating, active_count) = {
                    let sum = app.summaries.lock().expect("summary mutex poisoned");
                    let gen = app.generating.lock().expect("generating mutex poisoned");
                    (sum.contains_key(&id), gen.contains(&id), gen.len())
                };
                if !has_summary && !is_generating && active_count < 5 {
                    let jsonl = s.jsonl_path.clone();
                    let summaries = app.summaries.clone();
                    let generating = app.generating.clone();
                    app.summaries.lock().expect("summary mutex poisoned")
                        .insert(id.clone(), "Generating summary...".to_string());
                    spawn_summary_generation(id, jsonl, summaries, generating);
                }
            }
        }
    }
    Ok(())
}

fn spawn_summary_generation(
    id: String,
    jsonl: String,
    summaries: Arc<Mutex<std::collections::HashMap<String, String>>>,
    generating: Arc<Mutex<std::collections::HashSet<String>>>,
) {
    generating.lock().expect("generating mutex poisoned").insert(id.clone());
    tokio::spawn(async move {
        let msgs = tokio::task::spawn_blocking({
            let jsonl = jsonl.clone();
            move || crate::sessions::parse_messages(std::path::Path::new(&jsonl))
        }).await.ok().and_then(|r| r.ok());
        if let Some(msgs) = msgs {
            match crate::summary::generate_summary(&msgs).await {
                Ok(text) => {
                    let out = crate::summary::summary_path(&id);
                    let _ = crate::summary::write_summary(&out, &text);
                    summaries.lock().expect("summary mutex poisoned").insert(id.clone(), text);
                }
                Err(e) => {
                    summaries
                        .lock()
                        .expect("summary mutex poisoned")
                        .insert(id.clone(), format!("Error generating summary: {}", e));
                }
            }
        }
        generating.lock().expect("generating mutex poisoned").remove(&id);
    });
}

fn draw(f: &mut ratatui::Frame, app: &mut AppState) {
    let area = f.area();

    const MAX_JOB_LINES: usize = 3;
    let jobs: Vec<String> = {
        let gen = app.generating.lock().expect("generating mutex poisoned");
        let sum = app.summaries.lock().expect("summary mutex poisoned");
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

    // Filter bar
    let filter_display = if app.filter_mode {
        format!("> {}|", app.filter)
    } else if app.filter.is_empty() {
        "  (press / to filter)".to_string()
    } else {
        format!("  filter: {}", app.filter)
    };
    let filter_block = Paragraph::new(filter_display)
        .block(Block::default().borders(Borders::ALL).title(" cc-speedy "));
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
            .block(Block::default().borders(Borders::ALL)
                .title(" Background ")
                .border_style(Style::default().fg(Color::Yellow)))
            .style(Style::default().fg(Color::Yellow));
        f.render_widget(jobs_panel, chunks[2]);
    }

    // Status bar
    let status = Paragraph::new(
        " Enter: resume  Ctrl+Enter: yolo  Tab: focus preview  j/k: navigate/scroll  /: filter  r: regenerate  q: quit",
    )
    .style(Style::default().fg(Color::DarkGray));
    f.render_widget(status, chunks[3]);
}

fn draw_list(f: &mut ratatui::Frame, app: &mut AppState, area: Rect) {
    let items: Vec<ListItem> = app
        .filtered
        .iter()
        .map(|&i| {
            let s = &app.sessions[i];
            let dt = format_time(s.modified);
            let folder = path_last_n(&s.project_path, 3);
            let line = Line::from(vec![
                Span::styled(format!("{} ", dt), Style::default().fg(Color::DarkGray)),
                Span::raw(format!("{:<28}", if s.summary.is_empty() {
                    truncate(&format!("[{}]", s.project_name), 27)
                } else {
                    truncate(&s.summary, 27)
                })),
                Span::styled(folder, Style::default().fg(Color::DarkGray)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let count = items.len();
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" Sessions ({}) ", count)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("► ");

    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn draw_preview(f: &mut ratatui::Frame, app: &mut AppState, area: Rect, scroll: u16) {
    let content = match app.selected_session() {
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
                let path = crate::summary::summary_path(&s.session_id);
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
    };

    let focused = app.focus == Focus::Preview;
    let block = Block::default()
        .borders(Borders::ALL)
        .title(if focused { " Summary  [Tab: back to list] " } else { " Summary  [Tab: scroll] " })
        .border_style(if focused { Style::default().fg(Color::Cyan) } else { Style::default() });
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
    // SGT = UTC+8
    const SGT_OFFSET_SECS: i64 = 8 * 3600;
    chrono::DateTime::from_timestamp(secs as i64 + SGT_OFFSET_SECS, 0)
        .unwrap_or_default()
        .format("%m-%d %H:%M")
        .to_string()
}

fn path_last_n(path: &str, n: usize) -> String {
    let parts: Vec<&str> = path.trim_end_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    let start = parts.len().saturating_sub(n);
    parts[start..].join("/")
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    }
}
