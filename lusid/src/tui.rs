#![allow(clippy::collapsible_if)]

use std::collections::{HashMap, HashSet};
use std::io::{self};
use std::pin::Pin;
use std::time::Duration;

use crossterm::{
    event::{Event as CEvent, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use lusid_apply_stdio::{
    AppUpdate, AppView, AppViewError, FlatViewTree, FlatViewTreeError, FlatViewTreeNode,
    OperationView, ViewNode,
};
use lusid_cmd::CommandError;
use lusid_ssh::SshError;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs, Wrap},
    Terminal,
};
use serde_json::Error as SerdeJsonError;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::sync::mpsc;
use tokio::time::{interval, Interval};

#[derive(Error, Debug)]
pub enum TuiError {
    #[error(transparent)]
    Io(#[from] io::Error),

    #[error("terminal init failed")]
    TerminalInit,

    #[error("failed to enable raw mode")]
    EnableRawMode,

    #[error("failed to disable raw mode")]
    DisableRawMode,

    #[error("failed to parse apply stdout as json: {0}")]
    ParseApplyStdout(#[from] SerdeJsonError),

    #[error("failed to read stdout from apply")]
    ReadApplyStdout(#[source] tokio::io::Error),

    #[error("failed to read stderr from apply")]
    ReadApplyStderr(#[source] tokio::io::Error),

    #[error(transparent)]
    AppView(#[from] AppViewError),

    #[error(transparent)]
    FlatTree(#[from] FlatViewTreeError),

    #[error("apply command failed: {0}")]
    Command(#[from] CommandError),

    #[error("ssh failed: {0}")]
    Ssh(#[from] SshError),
}

pub async fn tui<Stdout, Stderr, Wait, Error>(
    stdout: Stdout,
    stderr: Stderr,
    wait: Pin<Box<Wait>>,
) -> Result<(), TuiError>
where
    Stdout: AsyncRead + Unpin + Send,
    Stderr: AsyncRead + Unpin + Send,
    Wait: Future<Output = Result<(), Error>> + Send,
    TuiError: From<Error>,
{
    let (update_tx, mut update_rx) = mpsc::channel::<AppUpdate>(256);
    let (stderr_tx, mut stderr_rx) = mpsc::channel::<String>(256);
    let (exit_tx, mut exit_rx) = mpsc::channel::<()>(1);

    // Task: read AppUpdate JSON lines from stdout of lusid-apply
    let read_updates = {
        let mut lines = BufReader::new(stdout).lines();
        async move {
            while let Some(line) = lines.next_line().await.map_err(TuiError::ReadApplyStdout)? {
                if line.trim().is_empty() {
                    continue;
                }
                let update: AppUpdate = serde_json::from_str(&line)?;
                if update_tx.send(update).await.is_err() {
                    break;
                }
            }
            Ok::<(), TuiError>(())
        }
    };

    // Task: forward stderr lines to the UI
    let read_stderr = {
        let mut lines = BufReader::new(stderr).lines();
        async move {
            while let Some(line) = lines.next_line().await.map_err(TuiError::ReadApplyStderr)? {
                if stderr_tx.send(line).await.is_err() {
                    break;
                }
            }
            Ok::<(), TuiError>(())
        }
    };

    // Task: wait for child exit
    let wait_task = async move {
        wait.await?;
        let _ = exit_tx.send(()).await;
        Ok::<(), TuiError>(())
    };

    // Join tasks
    tokio::try_join!(read_updates, read_stderr, wait_task)?;

    // UI loop
    let mut app = TuiApp::new();
    let event_rx = spawn_crossterm_event_channel();
    let mut tick = interval(Duration::from_millis(33));
    run_terminal_loop(
        &mut app,
        &mut update_rx,
        &mut stderr_rx,
        &mut exit_rx,
        event_rx,
        &mut tick,
    )
    .await
}

async fn run_terminal_loop(
    app: &mut TuiApp,
    update_rx: &mut mpsc::Receiver<AppUpdate>,
    stderr_rx: &mut mpsc::Receiver<String>,
    exit_rx: &mut mpsc::Receiver<()>,
    mut event_rx: mpsc::Receiver<CEvent>,
    tick: &mut Interval,
) -> Result<(), TuiError> {
    enable_raw_mode().map_err(|_| TuiError::EnableRawMode)?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).map_err(|_| TuiError::TerminalInit)?;

    let backend = CrosstermBackend::new(stdout);

    let mut terminal = Terminal::new(backend).map_err(|_| TuiError::TerminalInit)?;
    terminal.clear()?;

    let mut should_quit = false;

    loop {
        // Draw
        terminal.draw(|f| draw_ui(f, app))?;

        // Async multiplexing: updates, stderr, events, tick, and exit
        tokio::select! {
            Some(update) = update_rx.recv() => {
                app.apply_update(update)?;
            }
            Some(line) = stderr_rx.recv() => {
                app.push_stderr(line);
            }
            Some(_) = exit_rx.recv() => {
                app.child_exited = true;
            }
            Some(event) = event_rx.recv() => {
                should_quit = app.handle_event(event)?;
            }
            _ = tick.tick() => {
                // Drive animations/spinners if desired
            }
            else => {}
        }

        if should_quit {
            break;
        }

        if app.finished() && app.child_exited {
            // Automatically exit a moment after we finish to avoid flicker:
            // let the user see final state.
            // A tiny sleep could be added here if desired.
            break;
        }
    }

    disable_raw_mode().map_err(|_| TuiError::DisableRawMode)?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen).map_err(|_| TuiError::TerminalInit)?;
    terminal.show_cursor()?;
    Ok(())
}

fn spawn_crossterm_event_channel() -> mpsc::Receiver<CEvent> {
    let (tx, rx) = mpsc::channel(64);
    std::thread::spawn(move || loop {
        // Poll keyboard events
        if crossterm::event::poll(Duration::from_millis(100)).unwrap_or(false) {
            if let Ok(evt) = crossterm::event::read() {
                if tx.blocking_send(evt).is_err() {
                    break;
                }
            }
        }
    });
    rx
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppTab {
    ResourceParams,
    Resources,
    ResourceStates,
    ResourceChanges,
    Operations,
    Apply,
}

impl AppTab {
    const ALL: [AppTab; 6] = [
        AppTab::ResourceParams,
        AppTab::Resources,
        AppTab::ResourceStates,
        AppTab::ResourceChanges,
        AppTab::Operations,
        AppTab::Apply,
    ];

    fn title(self) -> &'static str {
        match self {
            AppTab::ResourceParams => "params",
            AppTab::Resources => "resources",
            AppTab::ResourceStates => "states",
            AppTab::ResourceChanges => "changes",
            AppTab::Operations => "operations",
            AppTab::Apply => "apply",
        }
    }

    fn index(self) -> usize {
        AppTab::ALL.iter().position(|t| *t == self).unwrap()
    }

    fn from_index(index: usize) -> Self {
        AppTab::ALL[index]
    }
}

#[derive(Debug, Default, Clone)]
struct TreeState {
    expanded: HashSet<usize>,
    selected: Option<usize>,
    list_offset: usize,
}

impl TreeState {
    fn select_next(&mut self, visible_len: usize) {
        if visible_len == 0 {
            self.selected = None;
            self.list_offset = 0;
            return;
        }
        let sel = self.selected.unwrap_or(0);
        let next = (sel + 1).min(visible_len.saturating_sub(1));
        self.selected = Some(next);
    }

    fn select_prev(&mut self, visible_len: usize) {
        if visible_len == 0 {
            self.selected = None;
            self.list_offset = 0;
            return;
        }
        let sel = self.selected.unwrap_or(0);
        let prev = sel.saturating_sub(1);
        self.selected = Some(prev);
    }

    fn page_down(&mut self, visible_len: usize, page: usize) {
        if visible_len == 0 {
            return;
        }
        let sel = self.selected.unwrap_or(0);
        let next = (sel + page).min(visible_len.saturating_sub(1));
        self.selected = Some(next);
    }

    fn page_up(&mut self, visible_len: usize, page: usize) {
        if visible_len == 0 {
            return;
        }
        let sel = self.selected.unwrap_or(0);
        let next = sel.saturating_sub(page);
        self.selected = Some(next);
    }

    fn ensure_visible(&mut self, selected: usize, height: usize) {
        if height == 0 {
            return;
        }
        let bottom = self.list_offset + height.saturating_sub(1);
        if selected < self.list_offset {
            self.list_offset = selected;
        } else if selected > bottom {
            self.list_offset = selected.saturating_sub(height.saturating_sub(1));
        }
    }

    fn toggle(&mut self, node_index: usize) {
        if self.expanded.contains(&node_index) {
            self.expanded.remove(&node_index);
        } else {
            self.expanded.insert(node_index);
        }
    }

    fn expand_root(&mut self) {
        self.expanded.insert(FlatViewTree::root_index());
    }
}

#[derive(Debug, Default, Clone)]
struct OperationsApplyState {
    // Flattened selection through epochs -> operations
    flat_index_to_epoch_op: Vec<(usize, usize)>,
    selected_flat: Option<usize>,
    list_offset: usize,
}

impl OperationsApplyState {
    fn rebuild_index(&mut self, epochs: &[Vec<OperationView>]) {
        self.flat_index_to_epoch_op.clear();
        for (e, ops) in epochs.iter().enumerate() {
            for (o, _op) in ops.iter().enumerate() {
                self.flat_index_to_epoch_op.push((e, o));
            }
        }
        if self.flat_index_to_epoch_op.is_empty() {
            self.selected_flat = None;
            self.list_offset = 0;
        } else {
            let sel = self
                .selected_flat
                .unwrap_or(0)
                .min(self.flat_index_to_epoch_op.len() - 1);
            self.selected_flat = Some(sel);
        }
    }

    fn visible_len(&self) -> usize {
        self.flat_index_to_epoch_op.len()
    }

    fn select_next(&mut self) {
        if self.visible_len() == 0 {
            return;
        }
        let sel = self.selected_flat.unwrap_or(0);
        self.selected_flat = Some((sel + 1).min(self.visible_len().saturating_sub(1)));
    }

    fn select_prev(&mut self) {
        if self.visible_len() == 0 {
            return;
        }
        let sel = self.selected_flat.unwrap_or(0);
        self.selected_flat = Some(sel.saturating_sub(1));
    }

    fn page_down(&mut self, page: usize) {
        if self.visible_len() == 0 {
            return;
        }
        let sel = self.selected_flat.unwrap_or(0);
        let next = (sel + page).min(self.visible_len().saturating_sub(1));
        self.selected_flat = Some(next);
    }

    fn page_up(&mut self, page: usize) {
        if self.visible_len() == 0 {
            return;
        }
        let sel = self.selected_flat.unwrap_or(0);
        let next = sel.saturating_sub(page);
        self.selected_flat = Some(next);
    }

    fn ensure_visible(&mut self, selected: usize, height: usize) {
        if height == 0 {
            return;
        }
        let bottom = self.list_offset + height.saturating_sub(1);
        if selected < self.list_offset {
            self.list_offset = selected;
        } else if selected > bottom {
            self.list_offset = selected.saturating_sub(height.saturating_sub(1));
        }
    }
}

#[derive(Debug, Clone)]
struct TuiApp {
    app_view: AppView,
    tab: AppTab,
    tabs: Vec<String>,
    params_state: TreeState,
    resources_state: TreeState,
    states_state: TreeState,
    changes_state: TreeState,
    operations_state: TreeState,
    operations_apply_state: OperationsApplyState,
    stderr_tail: CircularBuffer<String>,
    child_exited: bool,
}

impl TuiApp {
    fn new() -> Self {
        let mut app = Self {
            app_view: AppView::default(),
            tab: AppTab::ResourceParams,
            tabs: AppTab::ALL.iter().map(|t| t.title().to_string()).collect(),
            params_state: TreeState::default(),
            resources_state: TreeState::default(),
            states_state: TreeState::default(),
            changes_state: TreeState::default(),
            operations_state: TreeState::default(),
            operations_apply_state: OperationsApplyState::default(),
            stderr_tail: CircularBuffer::new(200),
            child_exited: false,
        };
        // Expand root in all tree states by default
        app.params_state.expand_root();
        app.resources_state.expand_root();
        app.states_state.expand_root();
        app.changes_state.expand_root();
        app.operations_state.expand_root();
        app
    }

    fn finished(&self) -> bool {
        matches!(self.app_view, AppView::Done { .. })
    }

    fn apply_update(&mut self, update: AppUpdate) -> Result<(), TuiError> {
        self.app_view = self.app_view.clone().update(update.clone())?;
        // Rebuild indices or expand as we move through phases
        match &self.app_view {
            AppView::ResourceParams { .. } => {
                self.tab = AppTab::ResourceParams;
            }
            AppView::Resources { .. } => {
                self.tab = AppTab::Resources;
            }
            AppView::ResourceStates { .. } => {
                self.tab = AppTab::ResourceStates;
            }
            AppView::ResourceChanges { .. } => {
                self.tab = AppTab::ResourceChanges;
            }
            AppView::Operations { .. } => {
                self.tab = AppTab::Operations;
            }
            AppView::OperationsApply {
                operations_epochs, ..
            } => {
                self.tab = AppTab::Apply;
                self.operations_apply_state.rebuild_index(operations_epochs);
            }
            AppView::Done {
                operations_epochs, ..
            } => {
                self.tab = AppTab::Apply;
                self.operations_apply_state.rebuild_index(operations_epochs);
            }
            _ => {}
        }
        Ok(())
    }

    fn push_stderr(&mut self, line: String) {
        self.stderr_tail.push(line);
    }

    fn handle_event(&mut self, event: CEvent) -> Result<bool, TuiError> {
        if let CEvent::Key(KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            ..
        }) = event
        {
            match code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(true),
                KeyCode::Tab => {
                    let idx = (self.tab.index() + 1) % AppTab::ALL.len();
                    self.tab = AppTab::from_index(idx);
                }
                KeyCode::BackTab => {
                    let idx = (self.tab.index() + AppTab::ALL.len() - 1) % AppTab::ALL.len();
                    self.tab = AppTab::from_index(idx);
                }
                KeyCode::Left => {
                    let idx = (self.tab.index() + AppTab::ALL.len() - 1) % AppTab::ALL.len();
                    self.tab = AppTab::from_index(idx);
                }
                KeyCode::Right => {
                    let idx = (self.tab.index() + 1) % AppTab::ALL.len();
                    self.tab = AppTab::from_index(idx);
                }
                KeyCode::Down | KeyCode::Char('j') => self.move_selection_down(),
                KeyCode::Up | KeyCode::Char('k') => self.move_selection_up(),
                KeyCode::PageDown => self.page_down(),
                KeyCode::PageUp => self.page_up(),
                KeyCode::Home => self.select_top(),
                KeyCode::End => self.select_bottom(),
                KeyCode::Enter | KeyCode::Char(' ') => self.toggle_selected(),
                _ => {}
            }
        }
        Ok(false)
    }

    fn move_selection_down(&mut self) {
        match (self.tab, &self.app_view) {
            (AppTab::ResourceParams, AppView::ResourceParams { resource_params }) => {
                tree_select_next(resource_params, &mut self.params_state)
            }
            (AppTab::Resources, AppView::Resources { resources, .. }) => {
                tree_select_next(resources, &mut self.resources_state)
            }
            (
                AppTab::ResourceStates,
                AppView::ResourceStates {
                    resource_states, ..
                },
            ) => tree_select_next(resource_states, &mut self.states_state),
            (
                AppTab::ResourceChanges,
                AppView::ResourceChanges {
                    resource_changes, ..
                },
            ) => tree_select_next(resource_changes, &mut self.changes_state),
            (
                AppTab::Operations,
                AppView::Operations {
                    operations_tree, ..
                },
            ) => tree_select_next(operations_tree, &mut self.operations_state),
            (AppTab::Apply, AppView::OperationsApply { .. }) => {
                self.operations_apply_state.select_next()
            }
            (AppTab::Apply, AppView::Done { .. }) => self.operations_apply_state.select_next(),
            _ => {}
        }
    }

    fn move_selection_up(&mut self) {
        match (self.tab, &self.app_view) {
            (AppTab::ResourceParams, AppView::ResourceParams { resource_params }) => {
                tree_select_prev(resource_params, &mut self.params_state)
            }
            (AppTab::Resources, AppView::Resources { resources, .. }) => {
                tree_select_prev(resources, &mut self.resources_state)
            }
            (
                AppTab::ResourceStates,
                AppView::ResourceStates {
                    resource_states, ..
                },
            ) => tree_select_prev(resource_states, &mut self.states_state),
            (
                AppTab::ResourceChanges,
                AppView::ResourceChanges {
                    resource_changes, ..
                },
            ) => tree_select_prev(resource_changes, &mut self.changes_state),
            (
                AppTab::Operations,
                AppView::Operations {
                    operations_tree, ..
                },
            ) => tree_select_prev(operations_tree, &mut self.operations_state),
            (AppTab::Apply, AppView::OperationsApply { .. }) => {
                self.operations_apply_state.select_prev()
            }
            (AppTab::Apply, AppView::Done { .. }) => self.operations_apply_state.select_prev(),
            _ => {}
        }
    }

    fn page_down(&mut self) {
        match (self.tab, &self.app_view) {
            (AppTab::ResourceParams, AppView::ResourceParams { resource_params }) => {
                tree_page_down(resource_params, &mut self.params_state, 10)
            }
            (AppTab::Resources, AppView::Resources { resources, .. }) => {
                tree_page_down(resources, &mut self.resources_state, 10)
            }
            (
                AppTab::ResourceStates,
                AppView::ResourceStates {
                    resource_states, ..
                },
            ) => tree_page_down(resource_states, &mut self.states_state, 10),
            (
                AppTab::ResourceChanges,
                AppView::ResourceChanges {
                    resource_changes, ..
                },
            ) => tree_page_down(resource_changes, &mut self.changes_state, 10),
            (
                AppTab::Operations,
                AppView::Operations {
                    operations_tree, ..
                },
            ) => tree_page_down(operations_tree, &mut self.operations_state, 10),
            (AppTab::Apply, AppView::OperationsApply { .. }) => {
                self.operations_apply_state.page_down(10)
            }
            (AppTab::Apply, AppView::Done { .. }) => self.operations_apply_state.page_down(10),
            _ => {}
        }
    }

    fn page_up(&mut self) {
        match (self.tab, &self.app_view) {
            (AppTab::ResourceParams, AppView::ResourceParams { resource_params }) => {
                tree_page_up(resource_params, &mut self.params_state, 10)
            }
            (AppTab::Resources, AppView::Resources { resources, .. }) => {
                tree_page_up(resources, &mut self.resources_state, 10)
            }
            (
                AppTab::ResourceStates,
                AppView::ResourceStates {
                    resource_states, ..
                },
            ) => tree_page_up(resource_states, &mut self.states_state, 10),
            (
                AppTab::ResourceChanges,
                AppView::ResourceChanges {
                    resource_changes, ..
                },
            ) => tree_page_up(resource_changes, &mut self.changes_state, 10),
            (
                AppTab::Operations,
                AppView::Operations {
                    operations_tree, ..
                },
            ) => tree_page_up(operations_tree, &mut self.operations_state, 10),
            (AppTab::Apply, AppView::OperationsApply { .. }) => {
                self.operations_apply_state.page_up(10)
            }
            (AppTab::Apply, AppView::Done { .. }) => self.operations_apply_state.page_up(10),
            _ => {}
        }
    }

    fn select_top(&mut self) {
        match (self.tab, &self.app_view) {
            (AppTab::ResourceParams, AppView::ResourceParams { resource_params }) => {
                tree_select_top(resource_params, &mut self.params_state)
            }
            (AppTab::Resources, AppView::Resources { resources, .. }) => {
                tree_select_top(resources, &mut self.resources_state)
            }
            (
                AppTab::ResourceStates,
                AppView::ResourceStates {
                    resource_states, ..
                },
            ) => tree_select_top(resource_states, &mut self.states_state),
            (
                AppTab::ResourceChanges,
                AppView::ResourceChanges {
                    resource_changes, ..
                },
            ) => tree_select_top(resource_changes, &mut self.changes_state),
            (
                AppTab::Operations,
                AppView::Operations {
                    operations_tree, ..
                },
            ) => tree_select_top(operations_tree, &mut self.operations_state),
            (AppTab::Apply, AppView::OperationsApply { .. }) => {
                self.operations_apply_state.selected_flat = Some(0)
            }
            (AppTab::Apply, AppView::Done { .. }) => {
                self.operations_apply_state.selected_flat = Some(0)
            }
            _ => {}
        }
    }

    fn select_bottom(&mut self) {
        match (self.tab, &self.app_view) {
            (AppTab::ResourceParams, AppView::ResourceParams { resource_params }) => {
                tree_select_bottom(resource_params, &mut self.params_state)
            }
            (AppTab::Resources, AppView::Resources { resources, .. }) => {
                tree_select_bottom(resources, &mut self.resources_state)
            }
            (
                AppTab::ResourceStates,
                AppView::ResourceStates {
                    resource_states, ..
                },
            ) => tree_select_bottom(resource_states, &mut self.states_state),
            (
                AppTab::ResourceChanges,
                AppView::ResourceChanges {
                    resource_changes, ..
                },
            ) => tree_select_bottom(resource_changes, &mut self.changes_state),
            (
                AppTab::Operations,
                AppView::Operations {
                    operations_tree, ..
                },
            ) => tree_select_bottom(operations_tree, &mut self.operations_state),
            (AppTab::Apply, AppView::OperationsApply { .. }) => {
                let n = self.operations_apply_state.visible_len();
                if n > 0 {
                    self.operations_apply_state.selected_flat = Some(n - 1);
                }
            }
            (AppTab::Apply, AppView::Done { .. }) => {
                let n = self.operations_apply_state.visible_len();
                if n > 0 {
                    self.operations_apply_state.selected_flat = Some(n - 1);
                }
            }
            _ => {}
        }
    }

    fn toggle_selected(&mut self) {
        match (self.tab, &self.app_view) {
            (AppTab::ResourceParams, AppView::ResourceParams { resource_params }) => {
                tree_toggle_selected(resource_params, &mut self.params_state).ok();
            }
            (AppTab::Resources, AppView::Resources { resources, .. }) => {
                tree_toggle_selected(resources, &mut self.resources_state).ok();
            }
            (
                AppTab::ResourceStates,
                AppView::ResourceStates {
                    resource_states, ..
                },
            ) => {
                tree_toggle_selected(resource_states, &mut self.states_state).ok();
            }
            (
                AppTab::ResourceChanges,
                AppView::ResourceChanges {
                    resource_changes, ..
                },
            ) => {
                tree_toggle_selected(resource_changes, &mut self.changes_state).ok();
            }
            (
                AppTab::Operations,
                AppView::Operations {
                    operations_tree, ..
                },
            ) => {
                tree_toggle_selected(operations_tree, &mut self.operations_state).ok();
            }
            _ => {}
        }
    }
}

fn draw_ui(f: &mut ratatui::Frame, app: &mut TuiApp) {
    let size = f.size();

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(1), // tabs
                Constraint::Min(5),    // main
                Constraint::Length(3), // status / stderr
            ]
            .as_ref(),
        )
        .split(size);

    draw_tabs(f, layout[0], app);
    draw_main(f, layout[1], app);
    draw_status(f, layout[2], app);
}

fn draw_tabs(f: &mut ratatui::Frame, area: Rect, app: &TuiApp) {
    let titles = app
        .tabs
        .iter()
        .map(|t| Line::from(vec![Span::styled(t, Style::default())]))
        .collect::<Vec<_>>();
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::BOTTOM))
        .select(app.tab.index())
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    f.render_widget(tabs, area);
}

fn draw_main(f: &mut ratatui::Frame, area: Rect, app: &mut TuiApp) {
    match &app.app_view {
        AppView::Start => draw_placeholder(f, area, "waiting for plan..."),
        AppView::ResourceParams { resource_params } => draw_tree(
            f,
            area,
            "resource params",
            resource_params,
            &mut app.params_state,
        ),
        AppView::Resources {
            resource_params: _,
            resources,
        } => draw_tree(f, area, "resources", resources, &mut app.resources_state),
        AppView::ResourceStates {
            resource_params: _,
            resources: _,
            resource_states,
        } => draw_tree(
            f,
            area,
            "resource states",
            resource_states,
            &mut app.states_state,
        ),
        AppView::ResourceChanges {
            resource_params: _,
            resources: _,
            resource_states: _,
            resource_changes,
            has_changes,
        } => {
            let title = match has_changes {
                Some(true) => "resource changes",
                Some(false) => "resource changes (no changes)",
                None => "resource changes (computing)",
            };
            draw_tree(f, area, title, resource_changes, &mut app.changes_state)
        }
        AppView::Operations {
            resource_params: _,
            resources: _,
            resource_states: _,
            resource_changes: _,
            has_changes: _,
            operations_tree,
        } => draw_tree(
            f,
            area,
            "operations",
            operations_tree,
            &mut app.operations_state,
        ),
        AppView::OperationsApply {
            resource_params: _,
            resources: _,
            resource_states: _,
            resource_changes: _,
            has_changes: _,
            operations_tree: _,
            operations_epochs,
        } => draw_apply(f, area, operations_epochs, &mut app.operations_apply_state),
        AppView::Done {
            resource_params: _,
            resources: _,
            resource_states: _,
            resource_changes: _,
            has_changes: _,
            operations_tree: _,
            operations_epochs,
        } => draw_apply(f, area, operations_epochs, &mut app.operations_apply_state),
    }
}

fn draw_status(f: &mut ratatui::Frame, area: Rect, app: &TuiApp) {
    let hints = "←/→ tabs  ↑/↓ move  PgUp/PgDn page  Enter toggle  q quit";
    let (left, right) = match &app.app_view {
        AppView::Start => ("planning...", ""),
        AppView::ResourceParams { .. } => ("resource params planned", ""),
        AppView::Resources { .. } => ("resources planned", ""),
        AppView::ResourceStates { .. } => ("resource states fetched", ""),
        AppView::ResourceChanges { has_changes, .. } => match has_changes {
            None => ("changes computing...", ""),
            Some(true) => ("changes ready", ""),
            Some(false) => ("no changes", ""),
        },
        AppView::Operations { .. } => ("operations planned", ""),
        AppView::OperationsApply { .. } => ("operations applying...", ""),
        AppView::Done { .. } => ("complete", "press q to exit"),
    };

    let lines = vec![
        Line::from(Span::styled(
            format!("{:<40}", left),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(hints, Style::default().fg(Color::DarkGray))),
        Line::from(Span::styled(
            format!("{:>20}", right),
            Style::default().fg(Color::Cyan),
        )),
    ];
    let text = Text::from(lines);
    let para = Paragraph::new(text)
        .block(Block::default().borders(Borders::TOP))
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });

    // Also include stderr tail as an extra line above the status hints, if present
    let stderr_str = app.stderr_tail.iter().last().cloned();
    if let Some(stderr_str) = stderr_str {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)].as_ref())
            .split(area);
        let err = Paragraph::new(stderr_str)
            .style(Style::default().fg(Color::Red))
            .block(Block::default().borders(Borders::NONE))
            .wrap(Wrap { trim: true });
        f.render_widget(err, layout[0]);
        f.render_widget(para, layout[1]);
    } else {
        f.render_widget(para, area);
    }
}

fn draw_placeholder(f: &mut ratatui::Frame, area: Rect, text: &str) {
    let para = Paragraph::new(Text::from(text))
        .block(Block::default().borders(Borders::ALL).title("lusid"))
        .alignment(Alignment::Center);
    f.render_widget(para, area);
}

fn draw_apply(
    f: &mut ratatui::Frame,
    area: Rect,
    epochs: &[Vec<OperationView>],
    state: &mut OperationsApplyState,
) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)].as_ref())
        .split(area);

    // Top: operations (flattened)
    let mut items: Vec<ListItem<'_>> = Vec::new();
    let mut lines_by_index: HashMap<usize, String> = HashMap::new();

    let mut idx = 0usize;
    for (e, ops) in epochs.iter().enumerate() {
        for (o, op) in ops.iter().enumerate() {
            let label = format!(
                "[{}] ({}, {}) {}",
                if op.is_complete { "✅" } else { "…" },
                e,
                o,
                op.label
            );
            let line = Line::from(Span::raw(label.clone()));
            items.push(ListItem::new(line));
            lines_by_index.insert(idx, label);
            idx += 1;
        }
    }

    let mut list_state = ratatui::widgets::ListState::default();
    if let Some(selected) = state.selected_flat {
        list_state.select(Some(selected));
    }
    *list_state.offset_mut() = state.list_offset;

    // Adjust visibility after layout height is known
    let height = layout[0].height.saturating_sub(2) as usize; // borders
    if let Some(sel) = state.selected_flat {
        state.ensure_visible(sel, height);
        *list_state.offset_mut() = state.list_offset;
    }

    let ops_list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("apply: operations"),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(ops_list, layout[0], &mut list_state);

    // Bottom: logs for selected operation
    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(sel) = state.selected_flat {
        if let Some((e, o)) = state.flat_index_to_epoch_op.get(sel).cloned() {
            if let Some(op) = epochs.get(e).and_then(|v| v.get(o)) {
                stdout = op.stdout.clone();
                stderr = op.stderr.clone();
            }
        }
    }

    let logs_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)].as_ref())
        .split(layout[1]);

    let stdout_para = Paragraph::new(stdout)
        .block(Block::default().borders(Borders::ALL).title("stdout"))
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(Color::White));

    let stderr_para = Paragraph::new(stderr)
        .block(Block::default().borders(Borders::ALL).title("stderr"))
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(Color::Red));

    f.render_widget(stdout_para, logs_layout[0]);
    f.render_widget(stderr_para, logs_layout[1]);
}

fn draw_tree(
    f: &mut ratatui::Frame<'_>,
    area: Rect,
    title: &str,
    tree: &FlatViewTree,
    state: &mut TreeState,
) {
    let rows = build_visible_rows(tree, state);

    // Build list items
    let items = rows
        .iter()
        .map(|r| {
            let mut segs = Vec::new();
            let indent = "  ".repeat(r.depth);
            segs.push(Span::raw(indent));
            if r.is_branch {
                segs.push(Span::styled(
                    format!("{} ", if r.is_expanded { "▼" } else { "▶" }),
                    Style::default().fg(Color::Yellow),
                ));
            } else {
                segs.push(Span::styled("• ", Style::default().fg(Color::DarkGray)));
            }
            segs.push(Span::raw(&r.label));
            ListItem::new(Line::from(segs))
        })
        .collect::<Vec<_>>();

    // Stateful list with scrolling
    let mut list_state = ListState::default();
    if let Some(sel) = state.selected {
        list_state.select(Some(sel.min(rows.len().saturating_sub(1))));
    } else {
        list_state.select(None);
    }

    // Remember we have borders; the inner content height is area.height - 2
    let inner_height = area.height.saturating_sub(2) as usize;

    if let Some(sel) = state.selected {
        state.ensure_visible(sel, inner_height);
    }
    *list_state.offset_mut() = state.list_offset;

    let widget = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(widget, area, &mut list_state);
}

// Visible tree rows builder from scratch: depth-first with expand state
#[derive(Debug, Clone)]
struct TreeRow {
    index: usize,
    depth: usize,
    is_branch: bool,
    is_expanded: bool,
    label: String,
}

fn build_visible_rows(tree: &FlatViewTree, state: &TreeState) -> Vec<TreeRow> {
    let mut out = Vec::new();
    build_visible_rows_rec(
        tree,
        FlatViewTree::root_index(),
        0,
        state,
        &mut out,
        &mut HashSet::new(),
    );
    out
}

fn build_visible_rows_rec(
    tree: &FlatViewTree,
    index: usize,
    depth: usize,
    state: &TreeState,
    out: &mut Vec<TreeRow>,
    visited: &mut HashSet<usize>,
) {
    if !visited.insert(index) {
        return;
    }
    let node = match tree.get(index) {
        Ok(node) => node,
        Err(_) => return,
    };
    match node {
        FlatViewTreeNode::Leaf { view } => {
            let label = match view {
                ViewNode::NotStarted => "not started".to_string(),
                ViewNode::Started => "in progress".to_string(),
                ViewNode::Complete(v) => v.to_string(),
            };
            out.push(TreeRow {
                index,
                depth,
                is_branch: false,
                is_expanded: false,
                label,
            });
        }
        FlatViewTreeNode::Branch { view, children } => {
            let is_expanded = state.expanded.contains(&index);
            out.push(TreeRow {
                index,
                depth,
                is_branch: true,
                is_expanded,
                label: view.to_string(),
            });
            if is_expanded {
                for child in children.iter().copied() {
                    if tree.get(child).is_ok() {
                        build_visible_rows_rec(tree, child, depth + 1, state, out, visited);
                    }
                }
            }
        }
    }
}

fn tree_select_next(tree: &FlatViewTree, state: &mut TreeState) {
    let rows = build_visible_rows(tree, state);
    state.select_next(rows.len());
}

fn tree_select_prev(tree: &FlatViewTree, state: &mut TreeState) {
    let rows = build_visible_rows(tree, state);
    state.select_prev(rows.len());
}

fn tree_page_down(tree: &FlatViewTree, state: &mut TreeState, page: usize) {
    let rows = build_visible_rows(tree, state);
    state.page_down(rows.len(), page);
}

fn tree_page_up(tree: &FlatViewTree, state: &mut TreeState, page: usize) {
    let rows = build_visible_rows(tree, state);
    state.page_up(rows.len(), page);
}

fn tree_select_top(tree: &FlatViewTree, state: &mut TreeState) {
    let rows = build_visible_rows(tree, state);
    if !rows.is_empty() {
        state.selected = Some(0);
        state.list_offset = 0;
    }
}

fn tree_select_bottom(tree: &FlatViewTree, state: &mut TreeState) {
    let rows = build_visible_rows(tree, state);
    if !rows.is_empty() {
        let last = rows.len() - 1;
        state.selected = Some(last);
        state.list_offset = last.saturating_sub(1);
    }
}

fn tree_toggle_selected(tree: &FlatViewTree, state: &mut TreeState) -> Result<(), TuiError> {
    let rows = build_visible_rows(tree, state);
    if rows.is_empty() {
        return Ok(());
    }
    let selected = state.selected.unwrap_or(0).min(rows.len() - 1);
    let row = &rows[selected];
    if row.is_branch {
        state.toggle(row.index);
    }
    Ok(())
}

// Simple circular buffer for stderr view
#[derive(Debug, Clone)]
struct CircularBuffer<T> {
    buf: Vec<T>,
    cap: usize,
}

impl<T> CircularBuffer<T> {
    fn new(cap: usize) -> Self {
        Self {
            buf: Vec::with_capacity(cap),
            cap,
        }
    }

    fn push(&mut self, val: T) {
        if self.buf.len() == self.cap {
            self.buf.remove(0);
        }
        self.buf.push(val);
    }

    fn iter(&self) -> impl Iterator<Item = &T> {
        self.buf.iter()
    }
}
