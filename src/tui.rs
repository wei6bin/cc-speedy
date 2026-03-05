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

struct AppState {
    sessions: Vec<Session>,
    filtered: Vec<usize>,
    list_state: ListState,
    filter: String,
    filter_mode: bool,
    summaries: Arc<Mutex<std::collections::HashMap<String, String>>>,
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
        }
    }

    fn apply_filter(&mut self) {
        let q = self.filter.to_lowercase();
        self.filtered = self
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| q.is_empty() || s.project_name.to_lowercase().contains(&q))
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

    let generating: Arc<Mutex<std::collections::HashSet<String>>> =
        Arc::new(Mutex::new(std::collections::HashSet::new()));

    // Run event loop, always clean up terminal regardless of result
    let result = run_event_loop(&mut terminal, &mut app, &generating).await;

    // Always clean up terminal
    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();

    result
}

async fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut AppState,
    generating: &Arc<Mutex<std::collections::HashSet<String>>>,
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

                    (_, KeyCode::Down) | (_, KeyCode::Char('j')) if !app.filter_mode => {
                        let n = app.filtered.len();
                        if n > 0 {
                            let i = app.list_state.selected().unwrap_or(0);
                            app.list_state.select(Some((i + 1).min(n - 1)));
                        }
                    }
                    (_, KeyCode::Up) | (_, KeyCode::Char('k')) if !app.filter_mode => {
                        let i = app.list_state.selected().unwrap_or(0);
                        app.list_state.select(Some(i.saturating_sub(1)));
                    }

                    (_, KeyCode::Char('r')) if !app.filter_mode => {
                        if let Some(s) = app.selected_session() {
                            let id = s.session_id.clone();
                            let jsonl = s.jsonl_path.clone();
                            let summaries = app.summaries.clone();
                            app.summaries.lock().expect("summary mutex poisoned").remove(&id);
                            generating.lock().expect("generating mutex poisoned").remove(&id);
                            spawn_summary_generation(id, jsonl, summaries, generating.clone());
                        }
                    }

                    (_, KeyCode::Enter) if !app.filter_mode => {
                        if let Some(s) = app.selected_session() {
                            let name = crate::tmux::session_name_from_path(&s.project_path);
                            let path = s.project_path.clone();
                            let id = s.session_id.clone();
                            // Note: terminal cleanup happens in run() after this returns
                            return crate::tmux::resume_in_tmux(&name, &path, &id);
                        }
                    }

                    _ => {}
                }
            }
        }

        // Trigger on-demand summary generation for selected session
        if let Some(s) = app.selected_session() {
            let id = s.session_id.clone();
            let has_summary = app.summaries.lock().expect("summary mutex poisoned").contains_key(&id);
            let is_generating = generating.lock().expect("generating mutex poisoned").contains(&id);
            if !has_summary && !is_generating {
                let jsonl = s.jsonl_path.clone();
                let summaries = app.summaries.clone();
                app.summaries
                    .lock()
                    .expect("summary mutex poisoned")
                    .insert(id.clone(), "Generating summary...".to_string());
                spawn_summary_generation(id, jsonl, summaries, generating.clone());
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
        let path = std::path::Path::new(&jsonl);
        if let Ok(msgs) = crate::sessions::parse_messages(path) {
            match crate::summary::generate_summary(&id, &msgs).await {
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
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
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
    draw_preview(f, app, panes[1]);

    // Status bar
    let status = Paragraph::new(
        " Enter: resume  j/k: navigate  /: filter  Esc: clear filter  r: regenerate  q: quit",
    )
    .style(Style::default().fg(Color::DarkGray));
    f.render_widget(status, chunks[2]);
}

fn draw_list(f: &mut ratatui::Frame, app: &mut AppState, area: Rect) {
    let items: Vec<ListItem> = app
        .filtered
        .iter()
        .map(|&i| {
            let s = &app.sessions[i];
            let dt = format_time(s.modified);
            let line = Line::from(vec![
                Span::styled(
                    format!("{} ", dt),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw(s.project_name.clone()),
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
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("► ");

    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn draw_preview(f: &mut ratatui::Frame, app: &mut AppState, area: Rect) {
    let content = match app.selected_session() {
        None => "No session selected".to_string(),
        Some(s) => {
            let summary = app
                .summaries
                .lock()
                .expect("summary mutex poisoned")
                .get(&s.session_id)
                .cloned()
                .unwrap_or_else(|| "[hover to generate summary]".to_string());
            format!(
                "PROJECT:  {}\nMSGS:     {}  |  {}\nSESSION:  {}...\n\n{}",
                s.project_path,
                s.message_count,
                format_time(s.modified),
                &s.session_id[..8.min(s.session_id.len())],
                summary
            )
        }
    };

    let preview = Paragraph::new(content)
        .block(Block::default().borders(Borders::ALL).title(" Summary "))
        .wrap(Wrap { trim: false });
    f.render_widget(preview, area);
}

fn format_time(t: std::time::SystemTime) -> String {
    let secs = t
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    chrono::DateTime::from_timestamp(secs as i64, 0)
        .unwrap_or_default()
        .format("%Y-%m-%d %H:%M")
        .to_string()
}
