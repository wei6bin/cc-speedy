use crate::theme;
use crate::unified::{list_all_sessions, SessionSource, UnifiedSession};
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Terminal,
};
use std::io::stdout;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(PartialEq, Copy, Clone)]
enum Focus {
    ActiveList,
    ArchivedList,
    Preview,
}

#[derive(PartialEq, Copy, Clone)]
enum SettingsField {
    Path,
    VaultName,
    DailyPush,
}

#[derive(PartialEq)]
enum AppMode {
    Normal,
    Filter,
    Grep,
    Rename,
    ActionMenu,
    Settings,
    Library,
    LibraryFilter,
    Projects,
    ProjectsFilter,
    TagEdit,
    LinkPicker,
    LinkPickerFilter,
    Digest,
    Help,
}

#[derive(PartialEq, Copy, Clone)]
pub enum ProjectSort {
    LastActive,
    SessionCount,
    Alphabetical,
}

pub struct ProjectRow {
    pub project_path: String,
    pub name: String,
    pub session_count: usize,
    pub pinned_count: usize,
    pub last_active: std::time::SystemTime,
}

struct AppState {
    sessions: Vec<UnifiedSession>,
    filtered_active: Vec<usize>,
    filtered_archived: Vec<usize>,
    list_state_active: ListState,
    list_state_archived: ListState,
    filter: String,
    grep_query: String,
    /// Lowercased haystack per session index — rebuilt on Grep mode entry.
    /// Empty when Grep mode is inactive.
    grep_haystacks: Vec<String>,
    mode: AppMode,
    rename_input: String,
    summaries: Arc<Mutex<std::collections::HashMap<String, String>>>,
    summary_generated_at: Arc<Mutex<std::collections::HashMap<String, i64>>>,
    generating: Arc<Mutex<std::collections::HashSet<String>>>,
    focus: Focus,
    preview_scroll: u16,
    status_msg: Option<(String, Instant)>,
    source_filter: Option<SessionSource>, // None = all, Some(CC) = CC only, Some(OC) = OC only
    pinned: std::collections::HashSet<String>,
    archived: std::collections::HashSet<String>,
    has_learnings: std::collections::HashSet<String>,
    obsidian_synced: std::collections::HashSet<String>,
    db: Arc<Mutex<rusqlite::Connection>>,
    /// Live git status per unique project_path. Populated by a startup batch
    /// and refreshed on selection change (30s stale) and manual `g`.
    git_status:
        Arc<Mutex<std::collections::HashMap<String, (crate::git_status::GitStatus, Instant)>>>,
    /// Learning Library state — populated on `L` entry, cleared on Esc.
    library_entries: Vec<crate::store::LearningEntry>,
    library_filter: String,
    library_category: Option<String>, // None = all
    library_filtered: Vec<usize>,
    library_list_state: ListState,
    /// Project Dashboard state.
    projects: Vec<ProjectRow>,
    projects_filtered: Vec<usize>,
    projects_filter: String,
    projects_sort: ProjectSort,
    projects_list_state: ListState,
    /// When set, only sessions with matching project_path are shown in the main list.
    project_filter: Option<String>,
    /// Tag cache: session_id → sorted list of normalized tags. Loaded at
    /// startup, updated on every `set_tags` write.
    tags_by_session: std::collections::HashMap<String, Vec<String>>,
    /// Input buffer for TagEdit mode.
    tag_edit_input: String,
    /// Session linking: child → parent map. Updated on set_link / unset_link.
    parent_of: std::collections::HashMap<String, String>,
    /// LinkPicker state.
    link_picker_filter: String,
    link_picker_filtered: Vec<usize>,
    link_picker_list_state: ListState,
    digest_text: String,
    digest_scroll: u16,
    settings: crate::settings::AppSettings,
    // Settings panel state (used by AppMode::Settings, added in Task 6)
    settings_editing: bool,
    settings_field: SettingsField,
    settings_input: String,
    settings_error: Option<String>,
}

impl AppState {
    fn new(sessions: Vec<UnifiedSession>, conn: rusqlite::Connection) -> anyhow::Result<Self> {
        let n = sessions.len();
        let mut list_state_active = ListState::default();
        if n > 0 {
            list_state_active.select(Some(0));
        }
        let list_state_archived = ListState::default();
        let mut summaries_map = crate::store::load_all_summaries(&conn)?;
        // For sessions that already have accumulated learnings, build the combined display string
        for (sid, factual) in summaries_map.iter_mut() {
            if let Ok(learnings) = crate::store::load_learnings(&conn, sid) {
                if !learnings.is_empty() {
                    *factual = crate::summary::build_combined_display(factual, &learnings);
                }
            }
        }
        let generated_at = crate::store::load_all_generated_at(&conn)?;
        let pinned = crate::store::load_pinned(&conn)?;
        let archived = crate::store::load_all_archived(&conn)?;
        let has_learnings = crate::store::load_sessions_with_learnings(&conn)?;
        let obsidian_synced = crate::store::load_obsidian_synced(&conn).unwrap_or_default();
        let tags_by_session = crate::store::load_all_tags(&conn).unwrap_or_default();
        let parent_of = crate::store::load_all_links(&conn).unwrap_or_default();
        let settings = crate::settings::load(&conn);
        let mut state = Self {
            filtered_active: (0..n).collect(),
            filtered_archived: vec![],
            sessions,
            list_state_active,
            list_state_archived,
            filter: String::new(),
            grep_query: String::new(),
            grep_haystacks: Vec::new(),
            mode: AppMode::Normal,
            rename_input: String::new(),
            summaries: Arc::new(Mutex::new(summaries_map)),
            summary_generated_at: Arc::new(Mutex::new(generated_at)),
            generating: Arc::new(Mutex::new(std::collections::HashSet::new())),
            focus: Focus::ActiveList,
            preview_scroll: 0,
            status_msg: None,
            source_filter: None,
            pinned,
            archived,
            has_learnings,
            obsidian_synced,
            db: Arc::new(Mutex::new(conn)),
            git_status: Arc::new(Mutex::new(std::collections::HashMap::new())),
            library_entries: Vec::new(),
            library_filter: String::new(),
            library_category: None,
            library_filtered: Vec::new(),
            library_list_state: ListState::default(),
            projects: Vec::new(),
            projects_filtered: Vec::new(),
            projects_filter: String::new(),
            projects_sort: ProjectSort::LastActive,
            projects_list_state: ListState::default(),
            project_filter: None,
            tags_by_session,
            tag_edit_input: String::new(),
            parent_of,
            link_picker_filter: String::new(),
            link_picker_filtered: Vec::new(),
            link_picker_list_state: ListState::default(),
            digest_text: String::new(),
            digest_scroll: 0,
            settings,
            settings_editing: false,
            settings_field: SettingsField::Path,
            settings_input: String::new(),
            settings_error: None,
        };
        // Split archived out of the active list on startup so the "all" view
        // correctly shows archived sessions in the bottom-left panel.
        state.apply_filter();
        Ok(state)
    }

    /// Rebuild per-session haystacks for grep mode. Each haystack is lowercased
    /// once so live keystrokes do O(N × len) substring checks with no allocs.
    fn rebuild_grep_haystacks(&mut self) {
        let summaries = self.summaries.lock().unwrap_or_else(|e| e.into_inner());
        self.grep_haystacks = self
            .sessions
            .iter()
            .map(|s| {
                let summary_body = summaries
                    .get(&s.session_id)
                    .map(|v| v.as_str())
                    .unwrap_or("");
                format!(
                    "{}\n{}\n{}\n{}",
                    s.summary, s.project_path, s.git_branch, summary_body,
                )
                .to_lowercase()
            })
            .collect();
    }

    /// Rebuild `link_picker_filtered` — every session except the currently
    /// selected one, substring-matched against `link_picker_filter`.
    fn apply_link_picker_filter(&mut self) {
        let current = self.selected_session().map(|s| s.session_id.clone());
        let q = self.link_picker_filter.to_lowercase();
        self.link_picker_filtered = self
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                if current
                    .as_ref()
                    .map(|id| id == &s.session_id)
                    .unwrap_or(false)
                {
                    return false;
                }
                q.is_empty()
                    || s.summary.to_lowercase().contains(&q)
                    || s.project_name.to_lowercase().contains(&q)
            })
            .map(|(i, _)| i)
            .collect();
        if self.link_picker_filtered.is_empty() {
            self.link_picker_list_state.select(None);
        } else {
            self.link_picker_list_state.select(Some(0));
        }
    }

    /// Rebuild the `projects` list by grouping sessions on project_path.
    /// Sorts per `projects_sort`. Filter is applied separately via
    /// `apply_projects_filter()`.
    fn rebuild_projects(&mut self) {
        self.projects = build_project_rows(&self.sessions, &self.pinned);
        self.sort_projects();
        self.apply_projects_filter();
    }

    fn sort_projects(&mut self) {
        match self.projects_sort {
            ProjectSort::LastActive => {
                self.projects
                    .sort_by(|a, b| b.last_active.cmp(&a.last_active));
            }
            ProjectSort::SessionCount => {
                self.projects.sort_by(|a, b| {
                    b.session_count
                        .cmp(&a.session_count)
                        .then(b.last_active.cmp(&a.last_active))
                });
            }
            ProjectSort::Alphabetical => {
                self.projects
                    .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            }
        }
    }

    fn apply_projects_filter(&mut self) {
        let q = self.projects_filter.to_lowercase();
        self.projects_filtered = self
            .projects
            .iter()
            .enumerate()
            .filter(|(_, p)| q.is_empty() || p.name.to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect();
        if self.projects_filtered.is_empty() {
            self.projects_list_state.select(None);
        } else {
            self.projects_list_state.select(Some(0));
        }
    }

    /// Rebuild `library_filtered` based on the current category + text filter.
    /// Called on every edit to library_filter or library_category.
    fn apply_library_filter(&mut self) {
        let q = self.library_filter.to_lowercase();
        let cat = self.library_category.as_deref();
        self.library_filtered = self
            .library_entries
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                if let Some(c) = cat {
                    if e.category != c {
                        return false;
                    }
                }
                q.is_empty() || e.point.to_lowercase().contains(&q)
            })
            .map(|(i, _)| i)
            .collect();
        if self.library_filtered.is_empty() {
            self.library_list_state.select(None);
        } else {
            self.library_list_state.select(Some(0));
        }
    }

    fn apply_filter(&mut self) {
        // Split filter query into tag tokens (#tag) and text tokens.
        let (tag_tokens, text_tokens) = parse_filter_tokens(&self.filter);
        let tag_tokens_lc: Vec<String> = tag_tokens.iter().map(|t| t.to_lowercase()).collect();
        let text_tokens_lc: Vec<String> = text_tokens.iter().map(|t| t.to_lowercase()).collect();

        let grep_q = self.grep_query.to_lowercase();
        let grep_active = self.mode == AppMode::Grep && !grep_q.is_empty();
        let pinned = &self.pinned;
        let archived = &self.archived;

        let matches_grep = |idx: usize| -> bool {
            if !grep_active {
                return true;
            }
            self.grep_haystacks
                .get(idx)
                .map(|h| h.contains(&grep_q))
                .unwrap_or(false)
        };

        let pf = self.project_filter.clone();

        // Separate into active and archived
        let mut active_indices: Vec<usize> = self
            .sessions
            .iter()
            .enumerate()
            .filter(|(i, s)| {
                if archived.contains(&s.session_id) {
                    return false;
                }
                if let Some(ref sf) = self.source_filter {
                    if &s.source != sf {
                        return false;
                    }
                }
                if let Some(ref pp) = pf {
                    if &s.project_path != pp {
                        return false;
                    }
                }
                if !matches_grep(*i) {
                    return false;
                }
                // All tag tokens must be present on this session.
                if !tag_tokens_lc.is_empty() {
                    let tags = self.tags_by_session.get(&s.session_id);
                    for wanted in &tag_tokens_lc {
                        let ok = tags.map(|v| v.iter().any(|t| t == wanted)).unwrap_or(false);
                        if !ok {
                            return false;
                        }
                    }
                }
                // All text tokens must match title or project_name.
                for tok in &text_tokens_lc {
                    if !(s.project_name.to_lowercase().contains(tok)
                        || s.summary.to_lowercase().contains(tok))
                    {
                        return false;
                    }
                }
                true
            })
            .map(|(i, _)| i)
            .collect();

        let mut archived_indices: Vec<usize> = self
            .sessions
            .iter()
            .enumerate()
            .filter(|(i, s)| {
                if !archived.contains(&s.session_id) {
                    return false;
                }
                if let Some(ref sf) = self.source_filter {
                    if &s.source != sf {
                        return false;
                    }
                }
                if let Some(ref pp) = pf {
                    if &s.project_path != pp {
                        return false;
                    }
                }
                if !matches_grep(*i) {
                    return false;
                }
                // All tag tokens must be present on this session.
                if !tag_tokens_lc.is_empty() {
                    let tags = self.tags_by_session.get(&s.session_id);
                    for wanted in &tag_tokens_lc {
                        let ok = tags.map(|v| v.iter().any(|t| t == wanted)).unwrap_or(false);
                        if !ok {
                            return false;
                        }
                    }
                }
                // All text tokens must match title or project_name.
                for tok in &text_tokens_lc {
                    if !(s.project_name.to_lowercase().contains(tok)
                        || s.summary.to_lowercase().contains(tok))
                    {
                        return false;
                    }
                }
                true
            })
            .map(|(i, _)| i)
            .collect();

        // Sort active: pinned first, then by recency
        active_indices.sort_by_key(|&i| {
            if pinned.contains(&self.sessions[i].session_id) {
                0u8
            } else {
                1u8
            }
        });

        // Sort archived by recency
        archived_indices.sort_by_key(|&i| std::cmp::Reverse(self.sessions[i].modified));

        self.filtered_active = active_indices;
        self.filtered_archived = archived_indices;

        // Select first item in active list if non-empty
        if !self.filtered_active.is_empty() {
            self.list_state_active.select(Some(0));
            self.list_state_archived.select(Some(0));
        } else if !self.filtered_archived.is_empty() {
            self.list_state_active.select(None);
            self.list_state_archived.select(Some(0));
        } else {
            self.list_state_active.select(None);
            self.list_state_archived.select(None);
        }
    }

    fn selected_session(&self) -> Option<&UnifiedSession> {
        match self.focus {
            Focus::ActiveList => {
                let idx = self.list_state_active.selected()?;
                let raw = *self.filtered_active.get(idx)?;
                self.sessions.get(raw)
            }
            Focus::ArchivedList => {
                let idx = self.list_state_archived.selected()?;
                let raw = *self.filtered_archived.get(idx)?;
                self.sessions.get(raw)
            }
            Focus::Preview => {
                // Use active list selection when in preview
                let idx = self.list_state_active.selected()?;
                let raw = *self.filtered_active.get(idx)?;
                self.sessions.get(raw)
            }
        }
    }
}

/// Split a filter query into `(tag_tokens, text_tokens)`. Whitespace-delimited
/// tokens starting with `#` are tag tokens (the `#` stripped); everything else
/// is a text token. Empty tokens are dropped.
pub fn parse_filter_tokens(query: &str) -> (Vec<String>, Vec<String>) {
    let mut tags = Vec::new();
    let mut texts = Vec::new();
    for tok in query.split_whitespace() {
        if let Some(t) = tok.strip_prefix('#') {
            if !t.is_empty() {
                tags.push(t.to_string());
            }
        } else {
            texts.push(tok.to_string());
        }
    }
    (tags, texts)
}

/// Group sessions by `project_path` into Project Dashboard rows.
/// Archived sessions are included in counts. Last-active is the max of
/// session.modified across the group. Pinned count is the number of sessions
/// in the group whose id is in the pinned set.
pub fn build_project_rows(
    sessions: &[UnifiedSession],
    pinned: &std::collections::HashSet<String>,
) -> Vec<ProjectRow> {
    use std::collections::HashMap;
    let mut acc: HashMap<String, ProjectRow> = HashMap::new();
    for s in sessions {
        let row = acc
            .entry(s.project_path.clone())
            .or_insert_with(|| ProjectRow {
                project_path: s.project_path.clone(),
                name: crate::util::path_last_n(&s.project_path, 2),
                session_count: 0,
                pinned_count: 0,
                last_active: std::time::UNIX_EPOCH,
            });
        row.session_count += 1;
        if pinned.contains(&s.session_id) {
            row.pinned_count += 1;
        }
        if s.modified > row.last_active {
            row.last_active = s.modified;
        }
    }
    acc.into_values().collect()
}

/// Walk every unique project_path across all sessions and dispatch a git
/// status check per path. Each check runs on a blocking worker; results land
/// in `app.git_status` asynchronously. Safe to call multiple times.
fn spawn_git_status_batch(app: &AppState) {
    let paths: std::collections::HashSet<String> = app
        .sessions
        .iter()
        .map(|s| s.project_path.clone())
        .collect();
    for path in paths {
        spawn_git_status_check(&app.git_status, path);
    }
}

/// If the currently selected session's git status is stale (> 30s) or missing,
/// enqueue a background refresh. Called once per event-loop tick; cheap when
/// fresh (hashmap lookup + instant comparison).
fn maybe_refresh_selected_git(app: &AppState) {
    const STALE_AFTER: std::time::Duration = std::time::Duration::from_secs(30);
    let Some(s) = app.selected_session() else {
        return;
    };
    let path = s.project_path.clone();
    let needs_refresh = {
        let cache = app.git_status.lock().unwrap_or_else(|e| e.into_inner());
        match cache.get(&path) {
            Some((_, at)) => at.elapsed() >= STALE_AFTER,
            None => true,
        }
    };
    if needs_refresh {
        spawn_git_status_check(&app.git_status, path);
    }
}

/// Refresh the git status for one path in the background. Non-blocking.
fn spawn_git_status_check(
    cache: &Arc<Mutex<std::collections::HashMap<String, (crate::git_status::GitStatus, Instant)>>>,
    path: String,
) {
    let cache = cache.clone();
    tokio::task::spawn_blocking(move || {
        let status = crate::git_status::check(&path, 500);
        cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(path, (status, Instant::now()));
    });
}

pub async fn run() -> Result<()> {
    let sessions = list_all_sessions()?;

    let conn = crate::store::open_db()?;
    crate::store::migrate_from_files(&conn)?;

    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = AppState::new(sessions, conn)?;

    // Kick off git status checks for each unique project path in parallel.
    // Cache is shared; results land while the TUI renders. First frame may
    // show blank indicators; subsequent redraws pick up completed entries.
    spawn_git_status_batch(&app);

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
        maybe_refresh_selected_git(app);
        terminal.draw(|f| draw(f, app))?;

        // Tick faster while a summary is generating so the spinner animates smoothly;
        // idle redraws stay at 200ms to avoid wasted work.
        let poll_ms = if app
            .generating
            .lock()
            .map(|g| !g.is_empty())
            .unwrap_or(false)
        {
            120
        } else {
            200
        };
        if event::poll(std::time::Duration::from_millis(poll_ms))? {
            if let Event::Key(key) = event::read()? {
                match (&app.mode, key.modifiers, key.code) {
                    // --- Global ---
                    (_, KeyModifiers::CONTROL, KeyCode::Char('c')) => break,
                    (AppMode::Normal, _, KeyCode::Char('q')) => break,

                    // Esc in Normal clears the project filter (if any). `q` still quits.
                    (AppMode::Normal, _, KeyCode::Esc) if app.project_filter.is_some() => {
                        app.project_filter = None;
                        app.apply_filter();
                        app.status_msg =
                            Some(("Project filter cleared".to_string(), Instant::now()));
                    }

                    // --- Weekly Digest ---
                    (AppMode::Normal, _, KeyCode::Char('D')) => {
                        let conn = app.db.lock().unwrap_or_else(|e| e.into_inner());
                        let learnings_raw =
                            crate::store::load_all_learnings(&conn).unwrap_or_default();
                        drop(conn);
                        let joined: Vec<crate::digest::LearningWithSession> = learnings_raw
                            .into_iter()
                            .map(|e| crate::digest::LearningWithSession {
                                session_id: e.session_id,
                                category: e.category,
                                point: e.point,
                                captured_at: e.captured_at,
                            })
                            .collect();
                        let data = crate::digest::build_digest(
                            &app.sessions,
                            &joined,
                            7,
                            std::time::SystemTime::now(),
                        );
                        app.digest_text = crate::digest::render_digest(&data);
                        app.digest_scroll = 0;
                        app.mode = AppMode::Digest;
                    }
                    (AppMode::Digest, _, KeyCode::Esc) => {
                        app.digest_text.clear();
                        app.digest_scroll = 0;
                        app.mode = AppMode::Normal;
                    }
                    (AppMode::Digest, _, KeyCode::Down)
                    | (AppMode::Digest, _, KeyCode::Char('j')) => {
                        app.digest_scroll = app.digest_scroll.saturating_add(1);
                    }
                    (AppMode::Digest, _, KeyCode::Up)
                    | (AppMode::Digest, _, KeyCode::Char('k')) => {
                        app.digest_scroll = app.digest_scroll.saturating_sub(1);
                    }
                    (AppMode::Digest, _, KeyCode::Char('e')) => {
                        let vault = app.settings.obsidian_kb_path.clone();
                        match vault {
                            Some(path) => {
                                match crate::obsidian::export_digest(&path, &app.digest_text) {
                                    Ok(rel) => {
                                        app.status_msg = Some((
                                            format!("Digest exported: {rel}"),
                                            Instant::now(),
                                        ));
                                    }
                                    Err(e) => {
                                        app.status_msg = Some((
                                            format!("Digest export failed: {e}"),
                                            Instant::now(),
                                        ));
                                    }
                                }
                            }
                            None => {
                                app.status_msg = Some((
                                    "No Obsidian path set — see `s` for settings".to_string(),
                                    Instant::now(),
                                ));
                            }
                        }
                    }

                    // --- Session linking ---
                    (AppMode::Normal, KeyModifiers::NONE, KeyCode::Char('l')) => {
                        if app.selected_session().is_some() {
                            app.link_picker_filter.clear();
                            app.apply_link_picker_filter();
                            app.mode = AppMode::LinkPicker;
                        }
                    }
                    (AppMode::Normal, KeyModifiers::NONE, KeyCode::Char('u')) => {
                        if let Some(sid) = app.selected_session().map(|s| s.session_id.clone()) {
                            let removed = app.parent_of.remove(&sid).is_some();
                            if removed {
                                let _ = crate::store::unset_link(
                                    &app.db.lock().unwrap_or_else(|e| e.into_inner()),
                                    &sid,
                                );
                                app.status_msg = Some(("Link removed".to_string(), Instant::now()));
                            }
                        }
                    }
                    (AppMode::LinkPicker, _, KeyCode::Esc) => {
                        app.link_picker_filter.clear();
                        app.link_picker_filtered.clear();
                        app.mode = AppMode::Normal;
                    }
                    (AppMode::LinkPicker, _, KeyCode::Char('/')) => {
                        app.mode = AppMode::LinkPickerFilter;
                    }
                    (AppMode::LinkPicker, _, KeyCode::Down)
                    | (AppMode::LinkPicker, _, KeyCode::Char('j')) => {
                        let n = app.link_picker_filtered.len();
                        if n > 0 {
                            let i = app.link_picker_list_state.selected().unwrap_or(0);
                            app.link_picker_list_state.select(Some((i + 1).min(n - 1)));
                        }
                    }
                    (AppMode::LinkPicker, _, KeyCode::Up)
                    | (AppMode::LinkPicker, _, KeyCode::Char('k')) => {
                        let i = app.link_picker_list_state.selected().unwrap_or(0);
                        app.link_picker_list_state.select(Some(i.saturating_sub(1)));
                    }
                    (AppMode::LinkPicker, _, KeyCode::Enter) => {
                        let child = app.selected_session().map(|s| s.session_id.clone());
                        let parent = app
                            .link_picker_list_state
                            .selected()
                            .and_then(|li| app.link_picker_filtered.get(li).copied())
                            .and_then(|si| app.sessions.get(si))
                            .map(|s| s.session_id.clone());
                        if let (Some(cid), Some(pid)) = (child, parent) {
                            match crate::store::set_link(
                                &app.db.lock().unwrap_or_else(|e| e.into_inner()),
                                &cid,
                                &pid,
                            ) {
                                Ok(()) => {
                                    app.parent_of.insert(cid, pid);
                                    app.status_msg = Some(("Linked".to_string(), Instant::now()));
                                }
                                Err(e) => {
                                    app.status_msg =
                                        Some((format!("Link failed: {e}"), Instant::now()));
                                }
                            }
                        }
                        app.link_picker_filter.clear();
                        app.link_picker_filtered.clear();
                        app.mode = AppMode::Normal;
                    }
                    (AppMode::LinkPickerFilter, _, KeyCode::Esc) => {
                        app.link_picker_filter.clear();
                        app.apply_link_picker_filter();
                        app.mode = AppMode::LinkPicker;
                    }
                    (AppMode::LinkPickerFilter, _, KeyCode::Enter) => {
                        app.mode = AppMode::LinkPicker;
                    }
                    (AppMode::LinkPickerFilter, _, KeyCode::Backspace) => {
                        app.link_picker_filter.pop();
                        app.apply_link_picker_filter();
                    }
                    (AppMode::LinkPickerFilter, KeyModifiers::NONE, KeyCode::Char(c))
                    | (AppMode::LinkPickerFilter, KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                        app.link_picker_filter.push(c);
                        app.apply_link_picker_filter();
                    }

                    // --- Tag edit mode ---
                    (AppMode::Normal, KeyModifiers::NONE, KeyCode::Char('t')) => {
                        if let Some(s) = app.selected_session() {
                            let sid = s.session_id.clone();
                            let current =
                                app.tags_by_session.get(&sid).cloned().unwrap_or_default();
                            app.tag_edit_input = current.join(", ");
                            app.mode = AppMode::TagEdit;
                        }
                    }
                    (AppMode::TagEdit, _, KeyCode::Esc) => {
                        app.tag_edit_input.clear();
                        app.mode = AppMode::Normal;
                    }
                    (AppMode::TagEdit, _, KeyCode::Enter) => {
                        let parsed = crate::store::parse_tags(&app.tag_edit_input);
                        if let Some(sid) = app.selected_session().map(|s| s.session_id.clone()) {
                            let write_result = crate::store::set_tags(
                                &app.db.lock().unwrap_or_else(|e| e.into_inner()),
                                &sid,
                                &parsed,
                            );
                            match write_result {
                                Ok(()) => {
                                    if parsed.is_empty() {
                                        app.tags_by_session.remove(&sid);
                                    } else {
                                        app.tags_by_session.insert(sid, parsed);
                                    }
                                    app.status_msg =
                                        Some(("Tags saved".to_string(), Instant::now()));
                                    app.apply_filter();
                                }
                                Err(e) => {
                                    app.status_msg =
                                        Some((format!("Tag save failed: {e}"), Instant::now()));
                                }
                            }
                        }
                        app.tag_edit_input.clear();
                        app.mode = AppMode::Normal;
                    }
                    (AppMode::TagEdit, _, KeyCode::Backspace) => {
                        app.tag_edit_input.pop();
                    }
                    (AppMode::TagEdit, KeyModifiers::NONE, KeyCode::Char(c))
                    | (AppMode::TagEdit, KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                        app.tag_edit_input.push(c);
                    }

                    // --- Project Dashboard mode ---
                    (AppMode::Normal, _, KeyCode::Char('P')) => {
                        app.projects_filter.clear();
                        app.rebuild_projects();
                        app.mode = AppMode::Projects;
                    }
                    (AppMode::Projects, _, KeyCode::Esc) => {
                        app.mode = AppMode::Normal;
                        app.projects.clear();
                        app.projects_filtered.clear();
                        app.projects_filter.clear();
                    }
                    (AppMode::Projects, _, KeyCode::Char('/')) => {
                        app.mode = AppMode::ProjectsFilter;
                    }
                    (AppMode::Projects, _, KeyCode::Char('s')) => {
                        app.projects_sort = match app.projects_sort {
                            ProjectSort::LastActive => ProjectSort::SessionCount,
                            ProjectSort::SessionCount => ProjectSort::Alphabetical,
                            ProjectSort::Alphabetical => ProjectSort::LastActive,
                        };
                        app.sort_projects();
                        app.apply_projects_filter();
                    }
                    (AppMode::Projects, _, KeyCode::Down)
                    | (AppMode::Projects, _, KeyCode::Char('j')) => {
                        let n = app.projects_filtered.len();
                        if n > 0 {
                            let i = app.projects_list_state.selected().unwrap_or(0);
                            app.projects_list_state.select(Some((i + 1).min(n - 1)));
                        }
                    }
                    (AppMode::Projects, _, KeyCode::Up)
                    | (AppMode::Projects, _, KeyCode::Char('k')) => {
                        let i = app.projects_list_state.selected().unwrap_or(0);
                        app.projects_list_state.select(Some(i.saturating_sub(1)));
                    }
                    (AppMode::Projects, _, KeyCode::Enter) => {
                        let target = app
                            .projects_list_state
                            .selected()
                            .and_then(|li| app.projects_filtered.get(li).copied())
                            .and_then(|pi| app.projects.get(pi))
                            .map(|p| p.project_path.clone());
                        if let Some(path) = target {
                            app.project_filter = Some(path);
                            app.mode = AppMode::Normal;
                            app.projects.clear();
                            app.projects_filtered.clear();
                            app.apply_filter();
                        }
                    }
                    (AppMode::ProjectsFilter, _, KeyCode::Esc) => {
                        app.projects_filter.clear();
                        app.apply_projects_filter();
                        app.mode = AppMode::Projects;
                    }
                    (AppMode::ProjectsFilter, _, KeyCode::Enter) => {
                        app.mode = AppMode::Projects;
                    }
                    (AppMode::ProjectsFilter, _, KeyCode::Backspace) => {
                        app.projects_filter.pop();
                        app.apply_projects_filter();
                    }
                    (AppMode::ProjectsFilter, KeyModifiers::NONE, KeyCode::Char(c))
                    | (AppMode::ProjectsFilter, KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                        app.projects_filter.push(c);
                        app.apply_projects_filter();
                    }

                    // --- Learning Library mode ---
                    (AppMode::Normal, _, KeyCode::Char('L')) => {
                        let conn = app.db.lock().unwrap_or_else(|e| e.into_inner());
                        match crate::store::load_all_learnings(&conn) {
                            Ok(entries) => {
                                drop(conn);
                                app.library_entries = entries;
                                app.library_filter.clear();
                                app.library_category = None;
                                app.apply_library_filter();
                                app.mode = AppMode::Library;
                            }
                            Err(e) => {
                                app.status_msg =
                                    Some((format!("Library load failed: {e}"), Instant::now()));
                            }
                        }
                    }
                    (AppMode::Library, _, KeyCode::Esc) => {
                        app.mode = AppMode::Normal;
                        app.library_entries.clear();
                        app.library_filtered.clear();
                        app.library_filter.clear();
                    }
                    (AppMode::Library, _, KeyCode::Char('/')) => {
                        app.mode = AppMode::LibraryFilter;
                    }
                    (AppMode::LibraryFilter, _, KeyCode::Esc) => {
                        app.library_filter.clear();
                        app.apply_library_filter();
                        app.mode = AppMode::Library;
                    }
                    (AppMode::LibraryFilter, _, KeyCode::Enter) => {
                        app.mode = AppMode::Library;
                    }
                    (AppMode::LibraryFilter, _, KeyCode::Backspace) => {
                        app.library_filter.pop();
                        app.apply_library_filter();
                    }
                    (AppMode::LibraryFilter, KeyModifiers::NONE, KeyCode::Char(c))
                    | (AppMode::LibraryFilter, KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                        app.library_filter.push(c);
                        app.apply_library_filter();
                    }
                    (AppMode::Library, _, KeyCode::Char('0')) => {
                        app.library_category = None;
                        app.apply_library_filter();
                    }
                    (AppMode::Library, _, KeyCode::Char('1')) => {
                        app.library_category = Some("decision_points".to_string());
                        app.apply_library_filter();
                    }
                    (AppMode::Library, _, KeyCode::Char('2')) => {
                        app.library_category = Some("lessons_gotchas".to_string());
                        app.apply_library_filter();
                    }
                    (AppMode::Library, _, KeyCode::Char('3')) => {
                        app.library_category = Some("tools_commands".to_string());
                        app.apply_library_filter();
                    }
                    (AppMode::Library, _, KeyCode::Down)
                    | (AppMode::Library, _, KeyCode::Char('j')) => {
                        let n = app.library_filtered.len();
                        if n > 0 {
                            let i = app.library_list_state.selected().unwrap_or(0);
                            app.library_list_state.select(Some((i + 1).min(n - 1)));
                        }
                    }
                    (AppMode::Library, _, KeyCode::Up)
                    | (AppMode::Library, _, KeyCode::Char('k')) => {
                        let i = app.library_list_state.selected().unwrap_or(0);
                        app.library_list_state.select(Some(i.saturating_sub(1)));
                    }
                    (AppMode::Library, _, KeyCode::Enter) => {
                        let target_id = app
                            .library_list_state
                            .selected()
                            .and_then(|li| app.library_filtered.get(li).copied())
                            .and_then(|ei| app.library_entries.get(ei))
                            .map(|e| e.session_id.clone());
                        if let Some(id) = target_id {
                            // Try to find in active list
                            let active_pos = app
                                .filtered_active
                                .iter()
                                .position(|&i| app.sessions[i].session_id == id);
                            let archived_pos = app
                                .filtered_archived
                                .iter()
                                .position(|&i| app.sessions[i].session_id == id);
                            if let Some(pos) = active_pos {
                                app.list_state_active.select(Some(pos));
                                app.focus = Focus::ActiveList;
                                app.preview_scroll = 0;
                                app.mode = AppMode::Normal;
                                app.library_entries.clear();
                                app.library_filtered.clear();
                            } else if let Some(pos) = archived_pos {
                                app.list_state_archived.select(Some(pos));
                                app.focus = Focus::ArchivedList;
                                app.preview_scroll = 0;
                                app.mode = AppMode::Normal;
                                app.library_entries.clear();
                                app.library_filtered.clear();
                            } else {
                                app.status_msg = Some((
                                    "Session not in current view — Esc then 0 to unfilter"
                                        .to_string(),
                                    Instant::now(),
                                ));
                            }
                        }
                    }

                    // --- Grep mode ---
                    (AppMode::Normal, _, KeyCode::Char('?')) => {
                        app.mode = AppMode::Grep;
                        app.grep_query.clear();
                        app.rebuild_grep_haystacks();
                        app.apply_filter();
                    }
                    (AppMode::Grep, _, KeyCode::Esc) => {
                        app.mode = AppMode::Normal;
                        app.grep_query.clear();
                        app.grep_haystacks.clear();
                        app.apply_filter();
                    }
                    (AppMode::Grep, _, KeyCode::Backspace) => {
                        app.grep_query.pop();
                        app.apply_filter();
                    }
                    // Only NONE/SHIFT chars go into the query; Ctrl/Alt+char fall
                    // through to the action handlers below (Ctrl+R regen, Ctrl+Y
                    // yolo, etc.), so those shortcuts still work during grep.
                    (AppMode::Grep, KeyModifiers::NONE, KeyCode::Char(c))
                    | (AppMode::Grep, KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                        app.grep_query.push(c);
                        app.apply_filter();
                    }

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
                                if let Some(idx) = app.list_state_active.selected() {
                                    if let Some(&raw) = app.filtered_active.get(idx) {
                                        if let Some(s) = app.sessions.get_mut(raw) {
                                            s.summary = title;
                                        }
                                    }
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
                    (AppMode::Normal, _, KeyCode::Tab) | (AppMode::Grep, _, KeyCode::Tab) => {
                        app.focus = match app.focus {
                            Focus::ActiveList => Focus::ArchivedList,
                            Focus::ArchivedList => Focus::Preview,
                            Focus::Preview => Focus::ActiveList,
                        };
                    }

                    (AppMode::Normal, _, KeyCode::Down)
                    | (AppMode::Normal, _, KeyCode::Char('j'))
                    | (AppMode::Filter, _, KeyCode::Down)
                    | (AppMode::Grep, _, KeyCode::Down) => {
                        if app.focus == Focus::Preview {
                            app.preview_scroll = app.preview_scroll.saturating_add(1);
                        } else if app.focus == Focus::ActiveList {
                            let n = app.filtered_active.len();
                            if n > 0 {
                                let i = app.list_state_active.selected().unwrap_or(0);
                                let next = (i + 1).min(n - 1);
                                if next != i {
                                    app.preview_scroll = 0;
                                }
                                app.list_state_active.select(Some(next));
                            }
                        } else {
                            let n = app.filtered_archived.len();
                            if n > 0 {
                                let i = app.list_state_archived.selected().unwrap_or(0);
                                let next = (i + 1).min(n - 1);
                                if next != i {
                                    app.preview_scroll = 0;
                                }
                                app.list_state_archived.select(Some(next));
                            }
                        }
                    }
                    (AppMode::Normal, _, KeyCode::Up)
                    | (AppMode::Normal, _, KeyCode::Char('k'))
                    | (AppMode::Filter, _, KeyCode::Up)
                    | (AppMode::Grep, _, KeyCode::Up) => {
                        if app.focus == Focus::Preview {
                            app.preview_scroll = app.preview_scroll.saturating_sub(1);
                        } else if app.focus == Focus::ActiveList {
                            let i = app.list_state_active.selected().unwrap_or(0);
                            let prev = i.saturating_sub(1);
                            if prev != i {
                                app.preview_scroll = 0;
                            }
                            app.list_state_active.select(Some(prev));
                        } else {
                            let i = app.list_state_archived.selected().unwrap_or(0);
                            let prev = i.saturating_sub(1);
                            if prev != i {
                                app.preview_scroll = 0;
                            }
                            app.list_state_archived.select(Some(prev));
                        }
                    }

                    // g: force-refresh all git status entries
                    (AppMode::Normal, KeyModifiers::NONE, KeyCode::Char('g')) => {
                        spawn_git_status_batch(app);
                        app.status_msg = Some(("refreshing git…".to_string(), Instant::now()));
                    }

                    // Ctrl+R: regenerate summary + knowledge extraction
                    (AppMode::Normal, KeyModifiers::CONTROL, KeyCode::Char('r'))
                    | (AppMode::Grep, KeyModifiers::CONTROL, KeyCode::Char('r')) => {
                        if let Some(s) = app.selected_session() {
                            let id = s.session_id.clone();
                            let jsonl = s.jsonl_path.clone();
                            let source = s.source.clone();
                            let session = s.clone();
                            let summaries = app.summaries.clone();
                            let generated_at = app.summary_generated_at.clone();
                            let generating = app.generating.clone();
                            let db = app.db.clone();
                            let obsidian_path = app.settings.obsidian_kb_path.clone();

                            // Load existing learnings before clearing cache
                            let existing_learnings = crate::store::load_learnings(
                                &app.db.lock().unwrap_or_else(|e| e.into_inner()),
                                &id,
                            )
                            .unwrap_or_default();

                            // Clear cached summary (learning rows in DB are kept)
                            app.summaries
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .remove(&id);
                            app.summary_generated_at
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .remove(&id);

                            spawn_summary_generation(
                                id,
                                jsonl,
                                source,
                                session,
                                existing_learnings,
                                obsidian_path,
                                summaries,
                                generated_at,
                                generating,
                                db,
                            );
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
                    (AppMode::Normal, KeyModifiers::NONE, KeyCode::Char('3')) => {
                        app.source_filter = Some(SessionSource::Copilot);
                        app.apply_filter();
                    }
                    (AppMode::Normal, KeyModifiers::NONE, KeyCode::Char('0')) => {
                        app.source_filter = None;
                        app.apply_filter();
                    }

                    (AppMode::Normal, _, KeyCode::Enter) | (AppMode::Grep, _, KeyCode::Enter) => {
                        if let Some(s) = app.selected_session() {
                            let path = s.project_path.clone();
                            let id = s.session_id.clone();
                            let title = window_title_from_session(s);
                            let result = match s.source {
                                SessionSource::ClaudeCode => {
                                    let name = crate::tmux::cc_session_name(&path);
                                    crate::tmux::resume_in_tmux(&name, &path, &id, false, &title)
                                }
                                SessionSource::OpenCode => {
                                    let name = crate::tmux::oc_session_name(&path);
                                    crate::tmux::resume_opencode_in_tmux(&name, &path, &id, &title)
                                }
                                SessionSource::Copilot => {
                                    let name = crate::tmux::copilot_session_name(&path);
                                    crate::tmux::resume_copilot_in_tmux(
                                        &name, &path, &id, false, &title,
                                    )
                                }
                            };
                            match result {
                                Ok(()) => return Ok(()),
                                Err(e) => {
                                    app.status_msg =
                                        Some((format!("Resume failed: {e}"), Instant::now()))
                                }
                            }
                        }
                    }

                    // n: new conversation in project folder
                    // Ctrl+Y: yolo mode
                    (AppMode::Normal, KeyModifiers::CONTROL, KeyCode::Char('y'))
                    | (AppMode::Grep, KeyModifiers::CONTROL, KeyCode::Char('y')) => {
                        if let Some(s) = app.selected_session() {
                            let path = s.project_path.clone();
                            let id = s.session_id.clone();
                            let title = window_title_from_session(s);
                            let result = match s.source {
                                SessionSource::ClaudeCode => {
                                    let name = crate::tmux::cc_session_name(&path);
                                    crate::tmux::resume_in_tmux(&name, &path, &id, true, &title)
                                }
                                SessionSource::OpenCode => {
                                    // OpenCode has no --dangerously-skip-permissions; fall back to normal resume
                                    let name = crate::tmux::oc_session_name(&path);
                                    crate::tmux::resume_opencode_in_tmux(&name, &path, &id, &title)
                                }
                                SessionSource::Copilot => {
                                    let name = crate::tmux::copilot_session_name(&path);
                                    crate::tmux::resume_copilot_in_tmux(
                                        &name, &path, &id, true, &title,
                                    )
                                }
                            };
                            match result {
                                Ok(()) => return Ok(()),
                                Err(e) => {
                                    app.status_msg =
                                        Some((format!("Resume failed: {e}"), Instant::now()))
                                }
                            }
                        }
                    }

                    // o: save current summary + learnings to Obsidian (no regeneration)
                    (AppMode::Normal, KeyModifiers::NONE, KeyCode::Char('o')) => {
                        let msg = save_selected_to_obsidian(app);
                        app.status_msg = Some((msg, Instant::now()));
                    }

                    // c: copy summary to clipboard
                    (AppMode::Normal, KeyModifiers::NONE, KeyCode::Char('c')) => {
                        let content = build_preview_content(app);
                        let msg = match copy_to_clipboard(&content) {
                            Ok(_) => "Copied to clipboard".to_string(),
                            Err(e) => format!("Copy failed: {}", e),
                        };
                        app.status_msg = Some((msg, Instant::now()));
                    }

                    // x: open pin/unpin popup
                    (AppMode::Normal, KeyModifiers::NONE, KeyCode::Char('x')) => {
                        if app.selected_session().is_some() {
                            app.mode = AppMode::ActionMenu;
                        }
                    }

                    // a: toggle archive status
                    (AppMode::Normal, KeyModifiers::NONE, KeyCode::Char('a')) => {
                        if let Some(s) = app.selected_session() {
                            let id = s.session_id.clone();
                            let now_archived = if app.archived.contains(&id) {
                                app.archived.remove(&id);
                                false
                            } else {
                                app.archived.insert(id.clone());
                                true
                            };
                            let _ = crate::store::set_archived(
                                &app.db.lock().unwrap_or_else(|e| e.into_inner()),
                                &id,
                                now_archived,
                            );
                            app.apply_filter();
                            let msg = if now_archived {
                                "Archived"
                            } else {
                                "Unarchived"
                            };
                            app.status_msg = Some((msg.to_string(), Instant::now()));
                        }
                    }

                    // --- ActionMenu mode ---
                    (AppMode::ActionMenu, _, KeyCode::Esc) => {
                        app.mode = AppMode::Normal;
                    }
                    (AppMode::ActionMenu, _, KeyCode::Char('p')) => {
                        if let Some(id) = app.selected_session().map(|s| s.session_id.clone()) {
                            let newly_pinned = if app.pinned.contains(&id) {
                                app.pinned.remove(&id);
                                false
                            } else {
                                app.pinned.insert(id.clone());
                                true
                            };
                            let _ = crate::store::set_pinned(
                                &app.db.lock().unwrap_or_else(|e| e.into_inner()),
                                &id,
                                newly_pinned,
                            );
                            app.apply_filter();
                            let msg = if newly_pinned { "Pinned" } else { "Unpinned" };
                            app.status_msg = Some((msg.to_string(), Instant::now()));
                        }
                        app.mode = AppMode::Normal;
                    }
                    // n / N: start a new session in the selected folder (lowercase=normal, uppercase=yolo)
                    (AppMode::ActionMenu, _, KeyCode::Char(k @ ('n' | 'N'))) => {
                        let yolo = k == 'N';
                        if let Some(s) = app.selected_session() {
                            let path = s.project_path.clone();
                            let title = format!("new:{}", crate::util::path_last_n(&path, 1));
                            let result = match s.source {
                                SessionSource::ClaudeCode => {
                                    let name = crate::tmux::new_cc_session_name(&path);
                                    crate::tmux::new_cc_in_tmux(&name, &path, yolo, &title, None)
                                }
                                SessionSource::OpenCode => {
                                    let name = crate::tmux::new_oc_session_name(&path);
                                    crate::tmux::new_oc_in_tmux(&name, &path, &title, None)
                                }
                                SessionSource::Copilot => {
                                    let name = crate::tmux::new_copilot_session_name(&path);
                                    crate::tmux::new_copilot_in_tmux(
                                        &name, &path, yolo, &title, None,
                                    )
                                }
                            };
                            match result {
                                Ok(()) => return Ok(()),
                                Err(e) => {
                                    app.status_msg =
                                        Some((format!("Launch failed: {e}"), Instant::now()))
                                }
                            }
                        }
                        app.mode = AppMode::Normal;
                    }
                    // s / S: start a new session with prior summary pre-pasted as context
                    (AppMode::ActionMenu, _, KeyCode::Char(k @ ('s' | 'S'))) => {
                        let yolo = k == 'S';
                        let (path, source, title, combined) = {
                            let Some(s) = app.selected_session() else {
                                app.mode = AppMode::Normal;
                                continue;
                            };
                            let id = s.session_id.clone();
                            let combined = app
                                .summaries
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .get(&id)
                                .cloned();
                            (
                                s.project_path.clone(),
                                s.source.clone(),
                                format!("new:{}", crate::util::path_last_n(&s.project_path, 1)),
                                combined,
                            )
                        };
                        let Some(combined) = combined else {
                            app.status_msg = Some((
                                "No summary available — press Ctrl+R to generate first".to_string(),
                                Instant::now(),
                            ));
                            app.mode = AppMode::Normal;
                            continue;
                        };
                        let context = crate::summary::build_new_session_context(&combined);
                        let result = match source {
                            SessionSource::ClaudeCode => {
                                let name = crate::tmux::new_cc_session_name(&path);
                                crate::tmux::new_cc_in_tmux(
                                    &name,
                                    &path,
                                    yolo,
                                    &title,
                                    Some(&context),
                                )
                            }
                            SessionSource::OpenCode => {
                                let name = crate::tmux::new_oc_session_name(&path);
                                crate::tmux::new_oc_in_tmux(&name, &path, &title, Some(&context))
                            }
                            SessionSource::Copilot => {
                                let name = crate::tmux::new_copilot_session_name(&path);
                                crate::tmux::new_copilot_in_tmux(
                                    &name,
                                    &path,
                                    yolo,
                                    &title,
                                    Some(&context),
                                )
                            }
                        };
                        match result {
                            Ok(()) => return Ok(()),
                            Err(e) => {
                                app.status_msg =
                                    Some((format!("Launch failed: {e}"), Instant::now()))
                            }
                        }
                        app.mode = AppMode::Normal;
                    }

                    // --- Help popup ---
                    (AppMode::Normal, _, KeyCode::F(1)) => {
                        app.mode = AppMode::Help;
                    }
                    (AppMode::Help, _, _) => {
                        app.mode = AppMode::Normal;
                    }

                    // --- Settings panel ---
                    (AppMode::Normal, KeyModifiers::NONE, KeyCode::Char('s')) => {
                        app.settings_field = SettingsField::Path;
                        app.settings_input =
                            app.settings.obsidian_kb_path.clone().unwrap_or_default();
                        app.settings_error = None;
                        app.settings_editing = false;
                        app.mode = AppMode::Settings;
                    }
                    (AppMode::Settings, _, KeyCode::Tab) if !app.settings_editing => {
                        app.settings_field = match app.settings_field {
                            SettingsField::Path => SettingsField::VaultName,
                            SettingsField::VaultName => SettingsField::DailyPush,
                            SettingsField::DailyPush => SettingsField::Path,
                        };
                        // Reseed input from the new focused field's stored value.
                        app.settings_input = match app.settings_field {
                            SettingsField::Path => {
                                app.settings.obsidian_kb_path.clone().unwrap_or_default()
                            }
                            SettingsField::VaultName => {
                                app.settings.obsidian_vault_name.clone().unwrap_or_default()
                            }
                            SettingsField::DailyPush => String::new(), // boolean — no text input
                        };
                        app.settings_error = None;
                    }
                    (AppMode::Settings, _, KeyCode::Esc) => {
                        if app.settings_editing {
                            app.settings_editing = false;
                            app.settings_error = None;
                        } else {
                            app.mode = AppMode::Normal;
                        }
                    }
                    (AppMode::Settings, _, KeyCode::Enter) => {
                        if !app.settings_editing && app.settings_field == SettingsField::DailyPush {
                            // Boolean — Enter toggles directly, no edit mode.
                            let new_val = !app.settings.obsidian_daily_push;
                            let result = crate::settings::save_obsidian_daily_push(
                                &app.db.lock().unwrap_or_else(|e| e.into_inner()),
                                new_val,
                            );
                            match result {
                                Ok(()) => {
                                    app.settings.obsidian_daily_push = new_val;
                                    app.status_msg = Some((
                                        format!(
                                            "Daily push: {}",
                                            if new_val { "on" } else { "off" }
                                        ),
                                        Instant::now(),
                                    ));
                                }
                                Err(e) => app.settings_error = Some(e.to_string()),
                            }
                        } else if !app.settings_editing {
                            app.settings_editing = true;
                            app.settings_error = None;
                        } else {
                            // Save the focused text field.
                            let value = app.settings_input.trim().to_string();
                            let conn = app.db.lock().unwrap_or_else(|e| e.into_inner());
                            let result = match app.settings_field {
                                SettingsField::Path => {
                                    crate::settings::save_obsidian_path(&conn, &value)
                                }
                                SettingsField::VaultName => {
                                    crate::settings::save_obsidian_vault_name(&conn, &value)
                                }
                                SettingsField::DailyPush => unreachable!(),
                            };
                            drop(conn);
                            match result {
                                Ok(()) => {
                                    match app.settings_field {
                                        SettingsField::Path => {
                                            app.settings.obsidian_kb_path =
                                                if value.is_empty() { None } else { Some(value) }
                                        }
                                        SettingsField::VaultName => {
                                            app.settings.obsidian_vault_name =
                                                if value.is_empty() { None } else { Some(value) }
                                        }
                                        SettingsField::DailyPush => unreachable!(),
                                    }
                                    app.settings_editing = false;
                                    app.settings_error = None;
                                    app.status_msg = Some(("Saved".to_string(), Instant::now()));
                                }
                                Err(e) => app.settings_error = Some(e.to_string()),
                            }
                        }
                    }
                    (AppMode::Settings, _, KeyCode::Backspace) if app.settings_editing => {
                        app.settings_input.pop();
                    }
                    (AppMode::Settings, _, KeyCode::Char(c)) if app.settings_editing => {
                        app.settings_input.push(c);
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
    jsonl: Option<String>,
    source: SessionSource,
    session: UnifiedSession,
    existing_learnings: Vec<crate::store::LearningPoint>,
    obsidian_path: Option<String>,
    summaries: Arc<Mutex<std::collections::HashMap<String, String>>>,
    summary_generated_at: Arc<Mutex<std::collections::HashMap<String, i64>>>,
    generating: Arc<Mutex<std::collections::HashSet<String>>>,
    db: Arc<Mutex<rusqlite::Connection>>,
) {
    generating
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id.clone());
    tokio::spawn(async move {
        let msgs = match source {
            SessionSource::ClaudeCode => {
                let Some(jsonl_path) = jsonl else {
                    generating
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .remove(&id);
                    return;
                };
                tokio::task::spawn_blocking({
                    let p = jsonl_path.clone();
                    move || crate::sessions::parse_messages(std::path::Path::new(&p))
                })
                .await
                .ok()
                .and_then(|r| r.ok())
            }
            SessionSource::OpenCode => {
                let session_id = id.clone();
                tokio::task::spawn_blocking(move || {
                    crate::opencode_sessions::parse_opencode_messages(&session_id)
                })
                .await
                .ok()
                .and_then(|r| r.ok())
            }
            SessionSource::Copilot => {
                let session_id = id.clone();
                tokio::task::spawn_blocking(move || {
                    crate::copilot_sessions::parse_copilot_messages(&session_id)
                })
                .await
                .ok()
                .and_then(|r| r.ok())
            }
        };

        if let Some(msgs) = msgs {
            let src_str = match source {
                SessionSource::ClaudeCode => "cc",
                SessionSource::OpenCode => "oc",
                SessionSource::Copilot => "co",
            };
            match crate::summary::generate_summary(&msgs, &existing_learnings).await {
                Ok((factual, new_points)) => {
                    // 1. Persist factual summary (overwrites existing)
                    let ts = crate::store::save_summary(
                        &db.lock().unwrap_or_else(|e| e.into_inner()),
                        &id,
                        src_str,
                        &factual,
                    )
                    .unwrap_or_else(|_| {
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64
                    });

                    // 2. Append new learning points (never overwrites old ones)
                    if !new_points.is_empty() {
                        let _ = crate::store::save_learnings(
                            &db.lock().unwrap_or_else(|e| e.into_inner()),
                            &id,
                            &new_points,
                        );
                    }

                    // 3. Load ALL learnings (existing + new) for combined display
                    let all_learnings = crate::store::load_learnings(
                        &db.lock().unwrap_or_else(|e| e.into_inner()),
                        &id,
                    )
                    .unwrap_or_default();

                    // 4. Build combined display string (factual + all learnings)
                    let combined = crate::summary::build_combined_display(&factual, &all_learnings);

                    // 5. Update in-memory cache with combined display
                    summaries
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert(id.clone(), combined);
                    summary_generated_at
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert(id.clone(), ts);

                    // 6. Export to Obsidian (non-fatal — failure only logs, never blocks display)
                    if let Some(ref vault_path) = obsidian_path {
                        let exported = crate::obsidian::export_to_obsidian(
                            &session,
                            &factual,
                            &all_learnings,
                            vault_path,
                        );
                        if exported.is_ok() {
                            let _ = crate::store::mark_obsidian_synced(
                                &db.lock().unwrap_or_else(|e| e.into_inner()),
                                &id,
                            );
                            // Snapshot settings now (we're outside the TUI, so we can't
                            // borrow AppState). Read fresh from the DB.
                            let settings_snapshot = {
                                let conn = db.lock().unwrap_or_else(|e| e.into_inner());
                                crate::settings::load(&conn)
                            };
                            if let Err(e) =
                                push_session_to_daily(&session, &factual, &settings_snapshot)
                            {
                                eprintln!("cc-speedy: daily push failed: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    summaries
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert(id.clone(), format!("Error generating summary: {}", e));
                }
            }
        }
        generating
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
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
        let mut items: Vec<String> = gen
            .iter()
            .map(|id| {
                let label = app
                    .sessions
                    .iter()
                    .find(|s| &s.session_id == id)
                    .map(|s| {
                        if !s.summary.is_empty() {
                            s.summary.clone()
                        } else {
                            s.project_name.clone()
                        }
                    })
                    .unwrap_or_else(|| id[..8.min(id.len())].to_string());
                let status = sum.get(id).map(|v| v.as_str()).unwrap_or("waiting...");
                format!("⟳  {} — {}", label, status)
            })
            .collect();
        if items.len() > MAX_JOB_LINES {
            let extra = items.len() - MAX_JOB_LINES;
            items.truncate(MAX_JOB_LINES);
            items.push(format!("   … and {} more", extra));
        }
        items
    };
    let jobs_height = if jobs.is_empty() {
        0
    } else {
        (jobs.len() + 2) as u16
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(jobs_height),
            Constraint::Length(1),
        ])
        .split(area);

    // Top bar: filter / grep / rename input / idle hint
    let (bar_text, bar_title) = match &app.mode {
        AppMode::Filter => (format!("> {}|", app.filter), " Filter "),
        AppMode::Grep => {
            let n = app.filtered_active.len() + app.filtered_archived.len();
            (
                format!(
                    "grep: {}|  ({} match{})",
                    app.grep_query,
                    n,
                    if n == 1 { "" } else { "es" }
                ),
                " Grep  [Esc: exit] ",
            )
        }
        AppMode::Rename => (
            format!("rename: {}|", app.rename_input),
            " Rename  [Enter: confirm  Esc: cancel] ",
        ),
        AppMode::ActionMenu => ("".to_string(), " cc-speedy "),
        AppMode::Settings => ("".to_string(), " cc-speedy — Settings "),
        AppMode::Library => {
            let cat_label = match app.library_category.as_deref() {
                Some("decision_points") => "decisions",
                Some("lessons_gotchas") => "lessons",
                Some("tools_commands") => "tools",
                _ => "all",
            };
            let n = app.library_filtered.len();
            (
                format!("  [{}]  {} entr{}  (/: filter  0:all  1:dec  2:lsn  3:tol  Enter: jump  Esc: exit)",
                        cat_label, n, if n == 1 { "y" } else { "ies" }),
                " Learning Library ",
            )
        }
        AppMode::LibraryFilter => (
            format!("library filter: {}|", app.library_filter),
            " Library — Filter  [Esc: clear  Enter: apply] ",
        ),
        AppMode::Projects => {
            let sort_label = match app.projects_sort {
                ProjectSort::LastActive => "last active",
                ProjectSort::SessionCount => "session count",
                ProjectSort::Alphabetical => "alphabetical",
            };
            let n = app.projects_filtered.len();
            (
                format!(
                    "  sort: {}  ·  {} project{}  (/: filter  s: sort  Enter: drill  Esc: exit)",
                    sort_label,
                    n,
                    if n == 1 { "" } else { "s" }
                ),
                " Project Dashboard ",
            )
        }
        AppMode::ProjectsFilter => (
            format!("projects filter: {}|", app.projects_filter),
            " Projects — Filter  [Esc: clear  Enter: apply] ",
        ),
        AppMode::TagEdit => (
            format!("tags: {}|", app.tag_edit_input),
            " Edit Tags  [Enter: save  Esc: cancel] ",
        ),
        AppMode::LinkPicker => {
            let n = app.link_picker_filtered.len();
            (
                format!("  pick parent session  ·  {} candidate{}  (/: filter  Enter: link  Esc: cancel)",
                        n, if n == 1 { "" } else { "s" }),
                " Link — Pick Parent ",
            )
        }
        AppMode::LinkPickerFilter => (
            format!("link picker filter: {}|", app.link_picker_filter),
            " Link Picker — Filter  [Esc: clear  Enter: apply] ",
        ),
        AppMode::Digest => (
            "  (j/k: scroll  e: export to Obsidian  Esc: exit)".to_string(),
            " Weekly Digest — last 7 days ",
        ),
        AppMode::Help => ("".to_string(), " cc-speedy — Help "),
        AppMode::Normal => {
            let hint = if let Some(ref pp) = app.project_filter {
                format!(
                    "  project: {}  (Esc to clear)",
                    crate::util::path_last_n(pp, 2)
                )
            } else if app.filter.is_empty() {
                "  (F1: help  /: filter  ?: grep  L: library  P: projects)".to_string()
            } else {
                format!("  filter: {}", app.filter)
            };
            (hint, " cc-speedy ")
        }
    };
    let filter_block = Paragraph::new(bar_text).block(
        Block::default()
            .border_type(theme::BORDER_TYPE)
            .borders(Borders::ALL)
            .border_style(theme::panel_block_style(theme::BORDER_TOP))
            .title(Span::styled(bar_title, theme::title_style())),
    );
    f.render_widget(filter_block, chunks[0]);

    // Library / Projects modes take over the main content area full-width.
    if app.mode == AppMode::Library || app.mode == AppMode::LibraryFilter {
        draw_library(f, app, chunks[1]);
    } else if app.mode == AppMode::Projects || app.mode == AppMode::ProjectsFilter {
        draw_projects(f, app, chunks[1]);
    } else if app.mode == AppMode::LinkPicker || app.mode == AppMode::LinkPickerFilter {
        draw_link_picker(f, app, chunks[1]);
    } else if app.mode == AppMode::Digest {
        draw_digest(f, app, chunks[1]);
    } else {
        // Main panes: left panel (split active/archived) and right preview
        let panes = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(chunks[1]);

        // Split left pane vertically: active sessions on top, archived below
        let archived_count = app.filtered_archived.len();
        let archived_height = if archived_count > 0 { 10 } else { 0 };
        let archived_height_constraint = if archived_count > 0 {
            Constraint::Length(archived_height as u16)
        } else {
            Constraint::Min(0)
        };

        let list_panes = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), archived_height_constraint])
            .split(panes[0]);

        // Clone the git cache once per frame so the two draw_list calls can each
        // pass a &HashMap without holding the mutex across both (would deadlock
        // with the background git-status writers).
        let git_cache = app
            .git_status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let generating_set = app
            .generating
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();

        draw_list(
            f,
            list_panes[0],
            &app.sessions,
            &app.pinned,
            &app.has_learnings,
            &app.obsidian_synced,
            &git_cache,
            &generating_set,
            &app.filtered_active,
            &mut app.list_state_active,
            "Sessions",
            Focus::ActiveList,
            app.focus,
        );
        if archived_count > 0 {
            draw_list(
                f,
                list_panes[1],
                &app.sessions,
                &app.pinned,
                &app.has_learnings,
                &app.obsidian_synced,
                &git_cache,
                &generating_set,
                &app.filtered_archived,
                &mut app.list_state_archived,
                "Archived",
                Focus::ArchivedList,
                app.focus,
            );
        }

        draw_preview(f, app, panes[1], app.preview_scroll);
    } // end of non-library branch

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
    const FOOTER_HINT: &str =
        " F1: help   /: filter   ?: grep   Enter: resume   n: new   Tab: focus   j/k: move   q: quit";
    let (status_text, status_style) = if let Some((msg, at)) = &app.status_msg {
        if at.elapsed().as_secs() < 2 {
            (msg.as_str(), Style::default().fg(theme::STATUS_OK))
        } else {
            (FOOTER_HINT, Style::default().fg(theme::STATUS_HELP))
        }
    } else {
        (FOOTER_HINT, Style::default().fg(theme::STATUS_HELP))
    };
    let version_text = concat!(" v", env!("CARGO_PKG_VERSION"), " ");
    let version_w = version_text.chars().count() as u16;
    let footer_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(version_w)])
        .split(chunks[3]);
    f.render_widget(
        Paragraph::new(status_text).style(status_style),
        footer_chunks[0],
    );
    f.render_widget(
        Paragraph::new(version_text)
            .style(Style::default().fg(theme::STATUS_HELP))
            .alignment(Alignment::Right),
        footer_chunks[1],
    );

    // Overlay popup for pin/unpin
    if app.mode == AppMode::ActionMenu {
        draw_pin_popup(f, app, area);
    }
    if app.mode == AppMode::Settings {
        draw_settings_popup(f, app, area);
    }
    if app.mode == AppMode::Help {
        draw_help_popup(f, area);
    }
}

fn draw_digest(f: &mut ratatui::Frame, app: &AppState, area: Rect) {
    let block = Block::default()
        .border_type(theme::BORDER_TYPE)
        .borders(Borders::ALL)
        .border_style(theme::panel_block_style(theme::BORDER_FOCUSED))
        .title(Span::styled(" Weekly Digest ", theme::title_style()));
    let para = Paragraph::new(app.digest_text.as_str())
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((app.digest_scroll, 0));
    f.render_widget(para, area);
}

fn draw_link_picker(f: &mut ratatui::Frame, app: &mut AppState, area: Rect) {
    let items: Vec<ListItem> = app
        .link_picker_filtered
        .iter()
        .filter_map(|&si| app.sessions.get(si))
        .map(|s| {
            let (badge_text, badge_color) = match s.source {
                SessionSource::ClaudeCode => ("[CC]", theme::CC_BADGE),
                SessionSource::OpenCode => ("[OC]", theme::OC_BADGE),
                SessionSource::Copilot => ("[CO]", theme::CO_BADGE),
            };
            let title = if !s.summary.is_empty() {
                truncate(&s.summary, 40)
            } else {
                s.project_name.clone()
            };
            Line::from(vec![
                Span::styled(format!("{} ", format_time(s.modified)), theme::dim_style()),
                Span::styled(format!("{} ", badge_text), Style::default().fg(badge_color)),
                Span::styled(title, Style::default().fg(theme::FG)),
                Span::styled(
                    format!("   {}", crate::util::path_last_n(&s.project_path, 2)),
                    theme::dim_style(),
                ),
            ])
        })
        .map(ListItem::new)
        .collect();

    let title = format!(
        " Link — Pick Parent  ({} candidate{}) ",
        items.len(),
        if items.len() == 1 { "" } else { "s" }
    );
    let list = List::new(items)
        .block(
            Block::default()
                .border_type(theme::BORDER_TYPE)
                .borders(Borders::ALL)
                .border_style(theme::panel_block_style(theme::BORDER_FOCUSED))
                .title(Span::styled(title, theme::title_style())),
        )
        .highlight_style(
            Style::default()
                .bg(theme::SEL_BG)
                .fg(theme::SEL_FG)
                .add_modifier(ratatui::style::Modifier::BOLD),
        );
    f.render_stateful_widget(list, area, &mut app.link_picker_list_state);
}

fn draw_projects(f: &mut ratatui::Frame, app: &mut AppState, area: Rect) {
    use crate::git_status::GitStatus;
    use ratatui::style::Color;

    let git_cache = app
        .git_status
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();

    let items: Vec<ListItem> = app
        .projects_filtered
        .iter()
        .filter_map(|&pi| app.projects.get(pi))
        .map(|p| {
            let (glyph, gcolor) = match git_cache.get(&p.project_path).map(|(s, _)| s) {
                Some(GitStatus::Dirty { .. }) => ("●", Color::Red),
                Some(GitStatus::Clean { .. }) => ("○", Color::Green),
                Some(GitStatus::NoGit) => ("·", theme::FG_DIM),
                Some(GitStatus::Error) => ("◦", Color::Yellow),
                None => (" ", theme::FG_DIM),
            };
            let branch_str = git_cache
                .get(&p.project_path)
                .and_then(|(s, _)| s.branch().map(|b| b.to_string()))
                .unwrap_or_default();
            let pin_str = if p.pinned_count > 0 {
                format!("   *{}", p.pinned_count)
            } else {
                String::new()
            };
            Line::from(vec![
                Span::styled(format!("{} ", glyph), Style::default().fg(gcolor)),
                Span::styled(
                    format!("{:<20} ", truncate(&branch_str, 20)),
                    theme::dim_style(),
                ),
                Span::styled(
                    format!("{:<28}", truncate(&p.name, 28)),
                    Style::default().fg(theme::FG),
                ),
                Span::styled(format!("{:>4} ", p.session_count), theme::dim_style()),
                Span::styled(
                    format!("last: {}", format_time(p.last_active)),
                    theme::dim_style(),
                ),
                Span::styled(pin_str, theme::pin_style()),
            ])
        })
        .map(ListItem::new)
        .collect();

    let title = format!(
        " Project Dashboard — {} project{} ",
        items.len(),
        if items.len() == 1 { "" } else { "s" },
    );
    let list = List::new(items)
        .block(
            Block::default()
                .border_type(theme::BORDER_TYPE)
                .borders(Borders::ALL)
                .border_style(theme::panel_block_style(theme::BORDER_FOCUSED))
                .title(Span::styled(title, theme::title_style())),
        )
        .highlight_style(
            Style::default()
                .bg(theme::SEL_BG)
                .fg(theme::SEL_FG)
                .add_modifier(ratatui::style::Modifier::BOLD),
        );

    f.render_stateful_widget(list, area, &mut app.projects_list_state);
}

fn draw_library(f: &mut ratatui::Frame, app: &mut AppState, area: Rect) {
    use ratatui::style::Color;

    // Build session_id → (title, modified) lookup from the in-memory sessions vec.
    let session_map: std::collections::HashMap<&str, (&str, std::time::SystemTime)> = app
        .sessions
        .iter()
        .map(|s| {
            let title = if !s.summary.is_empty() {
                s.summary.as_str()
            } else {
                s.project_name.as_str()
            };
            (s.session_id.as_str(), (title, s.modified))
        })
        .collect();

    let items: Vec<ListItem> = app
        .library_filtered
        .iter()
        .filter_map(|&ei| app.library_entries.get(ei))
        .map(|e| {
            let (cat_label, cat_color) = match e.category.as_str() {
                "decision_points" => ("DEC", Color::Rgb(30, 144, 255)), // blue
                "lessons_gotchas" => ("LSN", Color::Rgb(212, 160, 23)), // amber
                "tools_commands" => ("TOL", Color::Rgb(13, 131, 0)),    // green
                _ => ("???", theme::FG_DIM),
            };
            let (stitle, smodified) = session_map
                .get(e.session_id.as_str())
                .copied()
                .unwrap_or(("(unknown session)", std::time::UNIX_EPOCH));
            let date = format_time(smodified);
            Line::from(vec![
                Span::styled(format!("[{}] ", cat_label), Style::default().fg(cat_color)),
                Span::styled(e.point.clone(), Style::default().fg(theme::FG)),
                Span::styled("  —  ", theme::dim_style()),
                Span::styled(truncate(stitle, 30), Style::default().fg(theme::FG_DIM)),
                Span::styled(format!("  · {}", date), theme::dim_style()),
            ])
        })
        .map(ListItem::new)
        .collect();

    let border_color = theme::BORDER_FOCUSED;
    let title = format!(
        " Learning Library — {} entr{} ",
        items.len(),
        if items.len() == 1 { "y" } else { "ies" },
    );
    let list = List::new(items)
        .block(
            Block::default()
                .border_type(theme::BORDER_TYPE)
                .borders(Borders::ALL)
                .border_style(theme::panel_block_style(border_color))
                .title(Span::styled(title, theme::title_style())),
        )
        .highlight_style(
            Style::default()
                .bg(theme::SEL_BG)
                .fg(theme::SEL_FG)
                .add_modifier(ratatui::style::Modifier::BOLD),
        );

    f.render_stateful_widget(list, area, &mut app.library_list_state);
}

fn draw_list(
    f: &mut ratatui::Frame,
    area: Rect,
    sessions: &[UnifiedSession],
    pinned: &std::collections::HashSet<String>,
    has_learnings: &std::collections::HashSet<String>,
    obsidian_synced: &std::collections::HashSet<String>,
    git_cache: &std::collections::HashMap<String, (crate::git_status::GitStatus, Instant)>,
    generating: &std::collections::HashSet<String>,
    indices: &[usize],
    list_state: &mut ListState,
    title: &str,
    focus: Focus,
    current_focus: Focus,
) {
    let spinner = spinner_glyph();
    let items: Vec<ListItem> = indices
        .iter()
        .map(|&i| {
            let s = &sessions[i];
            let dt = format_time(s.modified);
            let folder = crate::util::path_last_n(&s.project_path, 3);
            let label = if s.summary.is_empty() {
                truncate(&format!("[{}]", s.project_name), 21)
            } else {
                truncate(&s.summary, 21)
            };
            let (badge_text, badge_color) = match s.source {
                SessionSource::ClaudeCode => ("[CC]", theme::CC_BADGE),
                SessionSource::OpenCode => ("[OC]", theme::OC_BADGE),
                SessionSource::Copilot => ("[CO]", theme::CO_BADGE),
            };
            let pin_span = if generating.contains(&s.session_id) {
                Span::styled(format!("{} ", spinner), Style::default().fg(theme::JOBS_FG))
            } else if pinned.contains(&s.session_id) {
                Span::styled("* ", theme::pin_style())
            } else {
                Span::raw("  ")
            };
            let kb_span = if has_learnings.contains(&s.session_id) {
                Span::styled("✓ ", Style::default().fg(theme::TITLE))
            } else {
                Span::raw("  ")
            };
            let obs_span = if obsidian_synced.contains(&s.session_id) {
                Span::styled("◆ ", Style::default().fg(theme::OBSIDIAN_PURPLE))
            } else {
                Span::raw("  ")
            };
            let git_span = git_status_span(&s.project_path, git_cache);
            let line = Line::from(vec![
                pin_span,
                Span::styled(format!("{} ", dt), theme::dim_style()),
                Span::styled(format!("{} ", badge_text), Style::default().fg(badge_color)),
                kb_span,
                obs_span,
                git_span,
                Span::styled(format!("{:<22}", label), Style::default().fg(theme::FG)),
                Span::styled(format!("{:>4} ", s.message_count), theme::dim_style()),
                Span::styled(folder, theme::dim_style()),
            ]);
            ListItem::new(line)
        })
        .collect();

    let count = items.len();
    let is_focused = current_focus == focus;
    let border_color = if is_focused {
        theme::BORDER_FOCUSED
    } else {
        theme::BORDER_LIST
    };
    let list = List::new(items)
        .block(
            Block::default()
                .border_type(theme::BORDER_TYPE)
                .borders(Borders::ALL)
                .border_style(theme::panel_block_style(border_color))
                .title(Span::styled(
                    format!(" {} ({}) ", title, count),
                    theme::title_style(),
                )),
        )
        .highlight_style(theme::sel_style())
        .highlight_symbol("► ");

    f.render_stateful_widget(list, area, list_state);
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

            let title_line = if !s.summary.is_empty() {
                format!("\nTITLE:    {}", s.summary)
            } else {
                String::new()
            };

            let file_line = if let Some(ref p) = s.jsonl_path {
                format!("\nFILE:     {}", p)
            } else {
                String::new()
            };

            let branch_line = {
                use crate::git_status::GitStatus;
                let live = app
                    .git_status
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(&s.project_path)
                    .map(|(g, _)| g.clone());
                match live {
                    Some(GitStatus::Clean { ref branch })
                    | Some(GitStatus::Dirty { ref branch }) => {
                        let dirty = matches!(live, Some(GitStatus::Dirty { .. }));
                        let ran_on = if !s.git_branch.is_empty() && s.git_branch != *branch {
                            format!("  (ran on {})", s.git_branch)
                        } else {
                            String::new()
                        };
                        let suffix = if dirty { "  (dirty)" } else { "" };
                        format!("\nBRANCH:   {}{}{}", branch, ran_on, suffix)
                    }
                    _ if !s.git_branch.is_empty() => format!("\nBRANCH:   {}", s.git_branch),
                    _ => String::new(),
                }
            };

            let tags_line = match app.tags_by_session.get(&s.session_id) {
                Some(tags) if !tags.is_empty() => format!("\nTAGS:     {}", tags.join(", ")),
                _ => String::new(),
            };

            let parent_line = app
                .parent_of
                .get(&s.session_id)
                .and_then(|pid| app.sessions.iter().find(|x| &x.session_id == pid))
                .map(|p| {
                    let badge = match p.source {
                        SessionSource::ClaudeCode => "[CC]",
                        SessionSource::OpenCode => "[OC]",
                        SessionSource::Copilot => "[CO]",
                    };
                    let title = if !p.summary.is_empty() {
                        truncate(&p.summary, 40)
                    } else {
                        p.project_name.clone()
                    };
                    format!(
                        "\nPARENT:   {} {} {}",
                        format_time(p.modified),
                        badge,
                        title
                    )
                })
                .unwrap_or_default();

            let children_line = {
                let child_ids: Vec<&String> = app
                    .parent_of
                    .iter()
                    .filter(|(_, pid)| pid.as_str() == s.session_id.as_str())
                    .map(|(cid, _)| cid)
                    .collect();
                if child_ids.is_empty() {
                    String::new()
                } else {
                    let mut entries: Vec<String> = child_ids
                        .iter()
                        .filter_map(|cid| app.sessions.iter().find(|x| &x.session_id == *cid))
                        .map(|c| {
                            let badge = match c.source {
                                SessionSource::ClaudeCode => "[CC]",
                                SessionSource::OpenCode => "[OC]",
                                SessionSource::Copilot => "[CO]",
                            };
                            let title = if !c.summary.is_empty() {
                                truncate(&c.summary, 40)
                            } else {
                                c.project_name.clone()
                            };
                            (
                                c.modified,
                                format!(
                                    "          {} {} {}",
                                    format_time(c.modified),
                                    badge,
                                    title
                                ),
                            )
                        })
                        .collect::<Vec<_>>()
                        .into_iter()
                        .map(|(_, line)| line)
                        .collect();
                    entries.sort();
                    let header = format!("\nCHILDREN: ({})", child_ids.len());
                    format!("{}\n{}", header, entries.join("\n"))
                }
            };

            let first_msg_line = if !s.first_user_msg.is_empty() {
                format!("\nFIRST:    {}", s.first_user_msg)
            } else {
                String::new()
            };

            let generated_line = {
                let gat = app
                    .summary_generated_at
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                match gat.get(&s.session_id) {
                    Some(&ts) => {
                        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(ts as u64);
                        format!("\n\n─── generated {} ───", format_time(t))
                    }
                    None => String::new(),
                }
            };

            format!(
                "PROJECT:  {}{}\nMSGS:     {}  |  {}{}{}{}{}{}{}\n\n{}{}",
                s.project_path,
                file_line,
                s.message_count,
                format_time(s.modified),
                title_line,
                branch_line,
                tags_line,
                parent_line,
                children_line,
                first_msg_line,
                summary,
                generated_line,
            )
        }
    }
}

fn draw_preview(f: &mut ratatui::Frame, app: &mut AppState, area: Rect, scroll: u16) {
    let content = build_preview_content(app);

    let grep_q_lc: String = if app.mode == AppMode::Grep && !app.grep_query.is_empty() {
        app.grep_query.to_lowercase()
    } else {
        String::new()
    };

    // Auto-scroll to first match only on selection change (preview_scroll reset to 0
    // on Up/Down); if the user has manually scrolled (non-zero), respect it.
    let effective_scroll = if !grep_q_lc.is_empty() && scroll == 0 {
        content
            .lines()
            .position(|l| l.to_lowercase().contains(&grep_q_lc))
            .map(|p| p as u16)
            .unwrap_or(0)
    } else {
        scroll
    };

    let lines: Vec<Line> = if grep_q_lc.is_empty() {
        content.lines().map(|l| Line::from(l.to_string())).collect()
    } else {
        content
            .lines()
            .map(|l| highlight_line(l, &grep_q_lc))
            .collect()
    };

    let focused = app.focus == Focus::Preview;
    let border_color = if focused {
        theme::BORDER_FOCUSED
    } else {
        theme::BORDER_PREVIEW
    };
    let block = Block::default()
        .border_type(theme::BORDER_TYPE)
        .borders(Borders::ALL)
        .border_style(theme::panel_block_style(border_color))
        .title(Span::styled(
            if focused {
                " Summary  [Tab: back to list] "
            } else {
                " Summary  [Tab: scroll] "
            },
            theme::title_style(),
        ));
    let preview = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((effective_scroll, 0));
    f.render_widget(preview, area);
}

/// Render the single-column git status glyph for a project path.
/// Returns a 2-column span: glyph + trailing space. Blank pair when the
/// cache has no entry yet (pending first check).
fn git_status_span(
    path: &str,
    git_cache: &std::collections::HashMap<String, (crate::git_status::GitStatus, Instant)>,
) -> Span<'static> {
    use crate::git_status::GitStatus;
    use ratatui::style::Color;
    let (glyph, color) = match git_cache.get(path).map(|(s, _)| s) {
        Some(GitStatus::Dirty { .. }) => ("●", Color::Red),
        Some(GitStatus::Clean { .. }) => ("○", Color::Green),
        Some(GitStatus::NoGit) => ("·", theme::FG_DIM),
        Some(GitStatus::Error) => ("◦", Color::Yellow),
        None => (" ", theme::FG_DIM),
    };
    Span::styled(format!("{} ", glyph), Style::default().fg(color))
}

/// Split `line` into alternating raw and styled spans wherever `query_lc` occurs
/// case-insensitively. Bails to a single raw span if lowercasing would change
/// the byte length (non-ASCII), since byte-offset indexing would misalign.
pub fn highlight_line(line: &str, query_lc: &str) -> Line<'static> {
    if query_lc.is_empty() {
        return Line::from(line.to_string());
    }
    let lc = line.to_lowercase();
    if lc.len() != line.len() {
        return Line::from(line.to_string());
    }
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut cursor = 0usize;
    while let Some(rel) = lc[cursor..].find(query_lc) {
        let abs = cursor + rel;
        if abs > cursor {
            spans.push(Span::raw(line[cursor..abs].to_string()));
        }
        let end = abs + query_lc.len();
        spans.push(Span::styled(
            line[abs..end].to_string(),
            theme::grep_match_style(),
        ));
        cursor = end;
    }
    if cursor < line.len() {
        spans.push(Span::raw(line[cursor..].to_string()));
    }
    Line::from(spans)
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

fn draw_pin_popup(f: &mut ratatui::Frame, app: &AppState, area: Rect) {
    let popup_area = centered_rect(56, 11, area);
    f.render_widget(Clear, popup_area);

    let (session_name, is_pinned, has_summary) = app
        .selected_session()
        .map(|s| {
            let name = if !s.summary.is_empty() {
                truncate(&s.summary, 44)
            } else {
                truncate(&s.project_name, 44)
            };
            let has = app
                .summaries
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .contains_key(&s.session_id);
            (name, app.pinned.contains(&s.session_id), has)
        })
        .unwrap_or_default();

    let pin_label = if is_pinned { "Unpin" } else { "Pin" };
    let summary_suffix = if has_summary { "" } else { "  (no summary)" };
    let content = format!(
        "\n  {}\n\n  [p] {}\n  [n] New session here        [N] New + yolo\n  [s] New w/ summary context{}\n  [S] New w/ summary + yolo\n  [Esc] Cancel",
        session_name, pin_label, summary_suffix
    );

    let popup = Paragraph::new(content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Actions ")
                .border_style(theme::pin_popup_style()),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(popup, popup_area);
}

fn draw_help_popup(f: &mut ratatui::Frame, area: Rect) {
    let popup_area = centered_rect(72, 34, area);
    f.render_widget(Clear, popup_area);

    let lines = vec![
        Line::from(""),
        Line::from(vec![Span::styled("  Navigation", theme::title_style())]),
        Line::from("    j / ↓        next session"),
        Line::from("    k / ↑        previous session"),
        Line::from("    Tab          toggle focus (active / archived / preview)"),
        Line::from(""),
        Line::from(vec![Span::styled("  Source filter", theme::title_style())]),
        Line::from("    1            Claude Code only"),
        Line::from("    2            OpenCode only"),
        Line::from("    3            Copilot only"),
        Line::from("    0            all sources"),
        Line::from(""),
        Line::from(vec![Span::styled("  Sessions", theme::title_style())]),
        Line::from("    Enter        resume in tmux"),
        Line::from("    n            new session in project"),
        Line::from("    Ctrl+Y       resume in yolo (skip permissions)"),
        Line::from("    Ctrl+N       new session in yolo"),
        Line::from("    /            filter (project + title)"),
        Line::from("    ?            cross-session grep (path, branch, summary, learnings)"),
        Line::from("    r            rename"),
        Line::from("    x            pin / unpin   |   a  archive / unarchive"),
        Line::from(""),
        Line::from(vec![Span::styled("  Summary", theme::title_style())]),
        Line::from("    Ctrl+R       (re)generate summary + extract learnings"),
        Line::from("    o            save current summary to Obsidian"),
        Line::from("    c            copy summary to clipboard"),
        Line::from(""),
        Line::from(vec![Span::styled("  Row glyphs", theme::title_style())]),
        Line::from(vec![
            Span::raw("    "),
            Span::styled("*", theme::pin_style()),
            Span::raw("  pinned    "),
            Span::styled("✓", Style::default().fg(theme::TITLE)),
            Span::raw("  has learnings    "),
            Span::styled("◆", Style::default().fg(theme::OBSIDIAN_PURPLE)),
            Span::raw("  synced to Obsidian"),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled("  App", theme::title_style())]),
        Line::from("    s            settings   |   F1  this help   |   q  quit"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  press any key to close",
            theme::dim_style(),
        )]),
    ];

    let popup = Paragraph::new(lines)
        .block(
            Block::default()
                .border_type(theme::BORDER_TYPE)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::BORDER_FOCUSED))
                .title(Span::styled(" Keyboard shortcuts ", theme::title_style())),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(popup, popup_area);
}

fn draw_settings_popup(f: &mut ratatui::Frame, app: &AppState, area: Rect) {
    let popup_area = centered_rect(70, 16, area);
    f.render_widget(Clear, popup_area);

    let path_val = app
        .settings
        .obsidian_kb_path
        .as_deref()
        .unwrap_or("(not set)");
    let vault_val = app
        .settings
        .obsidian_vault_name
        .as_deref()
        .unwrap_or("(auto-derived from path)");
    let push_val = if app.settings.obsidian_daily_push {
        "on"
    } else {
        "off"
    };

    let row = |focused: bool, label: &str, value: &str| -> Line<'_> {
        let marker = if focused { "▶ " } else { "  " };
        Line::from(vec![
            Span::raw(format!("{}{:<22}", marker, label)),
            Span::styled(value.to_string(), Style::default().fg(theme::FG)),
        ])
    };

    let path_line = if app.settings_editing && app.settings_field == SettingsField::Path {
        row(true, "Vault path:", &format!("{}|", app.settings_input))
    } else {
        row(
            app.settings_field == SettingsField::Path,
            "Vault path:",
            path_val,
        )
    };
    let name_line = if app.settings_editing && app.settings_field == SettingsField::VaultName {
        row(true, "Vault name:", &format!("{}|", app.settings_input))
    } else {
        row(
            app.settings_field == SettingsField::VaultName,
            "Vault name:",
            vault_val,
        )
    };
    let push_line = row(
        app.settings_field == SettingsField::DailyPush,
        "Push to daily note:",
        push_val,
    );

    let mut lines = vec![
        Line::from(""),
        path_line,
        name_line,
        push_line,
        Line::from(""),
    ];
    if let Some(ref err) = app.settings_error {
        lines.push(Line::from(Span::styled(
            format!("  ✗ {}", err),
            Style::default().fg(ratatui::style::Color::Red),
        )));
        lines.push(Line::from(""));
    }
    let hint = if app.settings_editing {
        "  [Enter] Save   [Esc] Cancel"
    } else {
        "  [Tab] Next field   [Enter] Edit / Toggle   [Esc] Close"
    };
    lines.push(Line::from(hint));

    let popup = Paragraph::new(lines)
        .block(
            Block::default()
                .border_type(theme::BORDER_TYPE)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::BORDER_SETTINGS))
                .title(Span::styled(" Settings ", theme::title_style())),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(popup, popup_area);
}

/// Animated spinner glyph; frame advances roughly every 120ms based on wall clock.
fn spinner_glyph() -> &'static str {
    const FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    FRAMES[((ms / 120) as usize) % FRAMES.len()]
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
    let label = if !s.summary.is_empty() {
        &s.summary
    } else {
        &s.project_name
    };
    truncate(label, 10)
}

/// Compute the daily-note line for a session and push it to today's daily
/// note. Idempotent via the wikilink marker. Caller decides whether to
/// surface the result. Returns `Ok(())` if the push succeeded OR was skipped
/// (push disabled / no vault name); returns `Err(reason)` only on actual CLI
/// failure when push was attempted.
fn push_session_to_daily(
    session: &crate::unified::UnifiedSession,
    factual: &str,
    settings: &crate::settings::AppSettings,
) -> Result<(), String> {
    if !settings.obsidian_daily_push {
        return Ok(());
    }
    let Some(vault) = settings.effective_vault_name() else {
        return Ok(()); // no vault configured — nothing to push to
    };

    let date_str = chrono::Local::now().format("%Y-%m-%d").to_string();
    let stem = crate::obsidian::note_stem_for_session(session, &date_str);
    let line = crate::obsidian::build_daily_line(
        session,
        &stem,
        crate::obsidian::parse_status_from_factual(factual),
        &crate::obsidian::extract_factual_title(factual),
    );
    let marker = format!("[[{}]]", stem);

    crate::obsidian_cli::daily_append(&vault, &line, Some(&marker)).map_err(|e| e.to_string())
}

/// Export the selected session's existing summary + learnings to Obsidian.
/// Returns the status message to flash. Does not regenerate the summary.
/// On success also persists the sync to SQLite and updates the in-memory set
/// so the row glyph appears immediately.
fn save_selected_to_obsidian(app: &mut AppState) -> String {
    let Some(session) = app.selected_session().cloned() else {
        return "No session selected".to_string();
    };
    let Some(vault_path) = app.settings.obsidian_kb_path.clone() else {
        return "Obsidian path not set (press s to configure)".to_string();
    };

    let (factual_opt, learnings) = {
        let conn = app.db.lock().unwrap_or_else(|e| e.into_inner());
        let factual = crate::store::load_summary_content(&conn, &session.session_id);
        let learnings =
            crate::store::load_learnings(&conn, &session.session_id).unwrap_or_default();
        (factual, learnings)
    };
    let Some(factual) = factual_opt else {
        return "No summary yet — press Ctrl+R to generate".to_string();
    };

    match crate::obsidian::export_to_obsidian(&session, &factual, &learnings, &vault_path) {
        Ok(()) => {
            let conn = app.db.lock().unwrap_or_else(|e| e.into_inner());
            let _ = crate::store::mark_obsidian_synced(&conn, &session.session_id);
            drop(conn);
            app.obsidian_synced.insert(session.session_id.clone());

            // Daily push runs off the main thread to keep the TUI responsive
            // (each CLI call costs 150–400 ms). Result is fire-and-forget;
            // failures only log to stderr because the main "Saved to Obsidian"
            // flash already confirmed the file write succeeded.
            let push_session = session.clone();
            let push_factual = factual.clone();
            let push_settings = app.settings.clone();
            tokio::task::spawn_blocking(move || {
                if let Err(e) = push_session_to_daily(&push_session, &push_factual, &push_settings)
                {
                    eprintln!("cc-speedy: daily push failed: {}", e);
                }
            });

            "Saved to Obsidian".to_string()
        }
        Err(e) => format!("Obsidian save failed: {}", e),
    }
}

/// Copy text to the system clipboard.
/// Tries clip.exe (WSL), xclip, xsel, pbcopy in order.
fn copy_to_clipboard(text: &str) -> anyhow::Result<()> {
    let candidates: &[(&str, &[&str])] = &[
        ("clip.exe", &[]),
        ("xclip", &["-selection", "clipboard"]),
        ("xsel", &["--clipboard", "--input"]),
        ("pbcopy", &[]),
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
            std::thread::spawn(move || {
                let _ = stdin.write_all(&bytes);
            });
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
