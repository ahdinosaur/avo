#![allow(clippy::collapsible_if)]

use std::collections::HashSet;
use std::future::Future;
use std::io;
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
use tokio::time::interval;

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

pub async fn tui<Stdout, Stderr, Wait, WaitError>(
    stdout: Stdout,
    stderr: Stderr,
    wait: Pin<Box<Wait>>,
) -> Result<(), TuiError>
where
    Stdout: AsyncRead + Unpin,
    Stderr: AsyncRead + Unpin,
    Wait: Future<Output = Result<(), WaitError>>,
    WaitError: Into<TuiError>,
{
    let mut stdout_lines = BufReader::new(stdout).lines();
    let mut stderr_lines = BufReader::new(stderr).lines();

    let mut terminal_session = TerminalSession::enter()?;
    let mut event_rx = spawn_crossterm_event_channel();
    let mut tick = interval(Duration::from_millis(33));
    let mut app = TuiApp::new();

    let mut stdout_done = false;
    let mut stderr_done = false;

    // We keep the outcome and only return it after the user exits the UI.
    let mut outcome: Option<Result<(), TuiError>> = None;

    let mut should_quit = false;

    tokio::pin!(wait);

    loop {
        terminal_session
            .terminal
            .draw(|frame| draw_ui(frame, &mut app))?;

        tokio::select! {
            // Child exit (only once)
            result = &mut wait, if outcome.is_none() => {
                let result = result.map_err(Into::into);
                app.child_exited = true;

                // Keep UI open; just remember the outcome.
                outcome = Some(result);
            }

            // Apply stdout (AppUpdate stream)
            line = stdout_lines.next_line(), if !stdout_done => {
                match line {
                    Ok(Some(line)) => {
                        if !line.trim().is_empty() {
                            let update: AppUpdate = serde_json::from_str(&line)?;
                            app.apply_update(update)?;
                        }
                    }
                    Ok(None) => {
                        stdout_done = true;
                    }
                    Err(err) => {
                        // Fatal: can't continue decoding updates.
                        outcome = Some(Err(TuiError::ReadApplyStdout(err)));
                        app.child_exited = true;
                    }
                }
            }

            // Apply stderr (display tail)
            line = stderr_lines.next_line(), if !stderr_done => {
                match line {
                    Ok(Some(line)) => {
                        app.push_stderr(line);
                    }
                    Ok(None) => {
                        stderr_done = true;
                    }
                    Err(err) => {
                        outcome = Some(Err(TuiError::ReadApplyStderr(err)));
                        app.child_exited = true;
                    }
                }
            }

            // Keyboard
            Some(event) = event_rx.recv() => {
                should_quit = app.handle_event(event)?;
            }

            _ = tick.tick() => {}
        }

        if should_quit {
            break;
        }

        // IMPORTANT: do NOT auto-exit just because pipes are closed.
        // That is exactly what makes the UI “flash and close” on “no changes”.
        //
        // If you want optional auto-exit, do it behind a flag or a short delay.
        if app.child_exited && outcome.is_some() && stdout_done && stderr_done {
            app.io_closed = true; // add this bool to your app if you want to show a banner
        }
    }

    // After user quits, return the child outcome (propagates failure correctly).
    match outcome {
        None => Ok(()),
        Some(result) => result,
    }
}

struct TerminalSession {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl TerminalSession {
    fn enter() -> Result<Self, TuiError> {
        enable_raw_mode().map_err(|_| TuiError::EnableRawMode)?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen).map_err(|_| TuiError::TerminalInit)?;

        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend).map_err(|_| TuiError::TerminalInit)?;
        terminal.clear()?;

        Ok(Self { terminal })
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

fn spawn_crossterm_event_channel() -> mpsc::Receiver<CEvent> {
    let (tx, rx) = mpsc::channel(64);
    std::thread::spawn(move || loop {
        let ready = crossterm::event::poll(Duration::from_millis(100)).unwrap_or(false);
        if !ready {
            continue;
        }
        if let Ok(evt) = crossterm::event::read() {
            if tx.blocking_send(evt).is_err() {
                break;
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

    fn from_app_view(view: &AppView) -> AppTab {
        match view {
            AppView::Start => AppTab::ResourceParams,
            AppView::ResourceParams { .. } => AppTab::ResourceParams,
            AppView::Resources { .. } => AppTab::Resources,
            AppView::ResourceStates { .. } => AppTab::ResourceStates,
            AppView::ResourceChanges { .. } => AppTab::ResourceChanges,
            AppView::Operations { .. } => AppTab::Operations,
            AppView::OperationsApply { .. } => AppTab::Apply,
            AppView::Done { .. } => AppTab::Apply,
        }
    }
}

#[derive(Debug, Default, Clone)]
struct TreeState {
    expanded: HashSet<usize>,
    selected_node: Option<usize>,
    list_offset: usize,
}

impl TreeState {
    fn expand_root(&mut self) {
        self.expanded.insert(FlatViewTree::root_index());
    }

    fn toggle(&mut self, node_index: usize) {
        if self.expanded.contains(&node_index) {
            self.expanded.remove(&node_index);
        } else {
            self.expanded.insert(node_index);
        }
    }

    fn ensure_visible_row(&mut self, selected_row: usize, height: usize) {
        if height == 0 {
            return;
        }
        let bottom = self.list_offset + height.saturating_sub(1);
        if selected_row < self.list_offset {
            self.list_offset = selected_row;
        } else if selected_row > bottom {
            self.list_offset = selected_row.saturating_sub(height.saturating_sub(1));
        }
    }
}

#[derive(Debug, Default, Clone)]
struct OperationsApplyState {
    flat_index_to_epoch_op: Vec<(usize, usize)>,
    selected_flat: Option<usize>,
    list_offset: usize,
}

impl OperationsApplyState {
    fn rebuild_index(&mut self, epochs: &[Vec<OperationView>]) {
        self.flat_index_to_epoch_op.clear();
        for (epoch_index, operations) in epochs.iter().enumerate() {
            for (operation_index, _) in operations.iter().enumerate() {
                self.flat_index_to_epoch_op
                    .push((epoch_index, operation_index));
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

    fn ensure_visible_row(&mut self, selected_row: usize, height: usize) {
        if height == 0 {
            return;
        }
        let bottom = self.list_offset + height.saturating_sub(1);
        if selected_row < self.list_offset {
            self.list_offset = selected_row;
        } else if selected_row > bottom {
            self.list_offset = selected_row.saturating_sub(height.saturating_sub(1));
        }
    }
}

#[derive(Debug, Clone)]
struct TuiApp {
    app_view: AppView,
    tab: AppTab,
    tabs: Vec<String>,
    follow_phase: bool,

    params_state: TreeState,
    resources_state: TreeState,
    states_state: TreeState,
    changes_state: TreeState,
    operations_state: TreeState,
    operations_apply_state: OperationsApplyState,

    stderr_tail: CircularBuffer<String>,
    child_exited: bool,
    io_closed: bool,
}

impl TuiApp {
    fn new() -> Self {
        let mut app = Self {
            app_view: AppView::default(),
            tab: AppTab::ResourceParams,
            tabs: AppTab::ALL.iter().map(|t| t.title().to_string()).collect(),
            follow_phase: true,
            params_state: TreeState::default(),
            resources_state: TreeState::default(),
            states_state: TreeState::default(),
            changes_state: TreeState::default(),
            operations_state: TreeState::default(),
            operations_apply_state: OperationsApplyState::default(),
            stderr_tail: CircularBuffer::new(200),
            child_exited: false,
            io_closed: false,
        };

        app.params_state.expand_root();
        app.resources_state.expand_root();
        app.states_state.expand_root();
        app.changes_state.expand_root();
        app.operations_state.expand_root();

        app
    }

    fn apply_update(&mut self, update: AppUpdate) -> Result<(), TuiError> {
        self.app_view = self.app_view.clone().update(update)?;

        if self.follow_phase {
            self.tab = AppTab::from_app_view(&self.app_view);
        }

        match &self.app_view {
            AppView::OperationsApply {
                operations_epochs, ..
            } => {
                self.operations_apply_state.rebuild_index(operations_epochs);
            }
            AppView::Done {
                operations_epochs, ..
            } => {
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
            code, modifiers, ..
        }) = event
        {
            if modifiers == KeyModifiers::NONE {
                match code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(true),

                    KeyCode::Char('f') => {
                        self.follow_phase = !self.follow_phase;
                        if self.follow_phase {
                            self.tab = AppTab::from_app_view(&self.app_view);
                        }
                    }

                    KeyCode::Tab => {
                        self.follow_phase = false;
                        let idx = (self.tab.index() + 1) % AppTab::ALL.len();
                        self.tab = AppTab::from_index(idx);
                    }

                    KeyCode::BackTab => {
                        self.follow_phase = false;
                        let idx = (self.tab.index() + AppTab::ALL.len() - 1) % AppTab::ALL.len();
                        self.tab = AppTab::from_index(idx);
                    }

                    KeyCode::Left => {
                        self.follow_phase = false;
                        let idx = (self.tab.index() + AppTab::ALL.len() - 1) % AppTab::ALL.len();
                        self.tab = AppTab::from_index(idx);
                    }

                    KeyCode::Right => {
                        self.follow_phase = false;
                        let idx = (self.tab.index() + 1) % AppTab::ALL.len();
                        self.tab = AppTab::from_index(idx);
                    }

                    KeyCode::Down | KeyCode::Char('j') => self.move_down(),
                    KeyCode::Up | KeyCode::Char('k') => self.move_up(),
                    KeyCode::Enter | KeyCode::Char(' ') => self.toggle_selected(),

                    _ => {}
                }
            }
        }
        Ok(false)
    }

    fn active_tree_state_and_tree_mut(&mut self) -> Option<(&FlatViewTree, &mut TreeState)> {
        match (&self.tab, &self.app_view) {
            (AppTab::ResourceParams, AppView::ResourceParams { resource_params }) => {
                Some((resource_params, &mut self.params_state))
            }
            (AppTab::Resources, AppView::Resources { resources, .. }) => {
                Some((resources, &mut self.resources_state))
            }
            (
                AppTab::ResourceStates,
                AppView::ResourceStates {
                    resource_states, ..
                },
            ) => Some((resource_states, &mut self.states_state)),
            (
                AppTab::ResourceChanges,
                AppView::ResourceChanges {
                    resource_changes, ..
                },
            ) => Some((resource_changes, &mut self.changes_state)),
            (
                AppTab::Operations,
                AppView::Operations {
                    operations_tree, ..
                },
            ) => Some((operations_tree, &mut self.operations_state)),
            _ => None,
        }
    }

    fn move_down(&mut self) {
        if let Some((tree, state)) = self.active_tree_state_and_tree_mut() {
            tree_move_selection(tree, state, 1);
            return;
        }

        if matches!(self.tab, AppTab::Apply) {
            let len = self.operations_apply_state.visible_len();
            if len == 0 {
                return;
            }
            let selected = self.operations_apply_state.selected_flat.unwrap_or(0);
            self.operations_apply_state.selected_flat =
                Some((selected + 1).min(len.saturating_sub(1)));
        }
    }

    fn move_up(&mut self) {
        if let Some((tree, state)) = self.active_tree_state_and_tree_mut() {
            tree_move_selection(tree, state, -1);
            return;
        }

        if matches!(self.tab, AppTab::Apply) {
            let selected = self.operations_apply_state.selected_flat.unwrap_or(0);
            self.operations_apply_state.selected_flat = Some(selected.saturating_sub(1));
        }
    }

    fn toggle_selected(&mut self) {
        if let Some((tree, state)) = self.active_tree_state_and_tree_mut() {
            let rows = build_visible_rows(tree, state);
            if rows.is_empty() {
                return;
            }
            let selected_row = selected_row_index(&rows, state).unwrap_or(0);
            let row = &rows[selected_row];
            if row.is_branch {
                state.toggle(row.index);
            }
        }
    }
}

fn draw_ui(frame: &mut ratatui::Frame<'_>, app: &mut TuiApp) {
    let size = frame.size();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(1),
                Constraint::Min(5),
                Constraint::Length(3),
            ]
            .as_ref(),
        )
        .split(size);

    draw_tabs(frame, layout[0], app);
    draw_main(frame, layout[1], app);
    draw_status(frame, layout[2], app);
}

fn draw_tabs(frame: &mut ratatui::Frame<'_>, area: Rect, app: &TuiApp) {
    let titles = app
        .tabs
        .iter()
        .map(|t| Line::from(vec![Span::styled(t, Style::default())]))
        .collect::<Vec<_>>();

    let suffix = if app.follow_phase { " (follow)" } else { "" };
    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .title(format!("lusid{suffix}")),
        )
        .select(app.tab.index())
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(tabs, area);
}

fn draw_main(frame: &mut ratatui::Frame<'_>, area: Rect, app: &mut TuiApp) {
    match &app.app_view {
        AppView::Start => draw_placeholder(frame, area, "waiting for plan..."),
        AppView::ResourceParams { resource_params } => draw_tree(
            frame,
            area,
            "resource params",
            resource_params,
            &mut app.params_state,
        ),
        AppView::Resources { resources, .. } => draw_tree(
            frame,
            area,
            "resources",
            resources,
            &mut app.resources_state,
        ),
        AppView::ResourceStates {
            resource_states, ..
        } => draw_tree(
            frame,
            area,
            "resource states",
            resource_states,
            &mut app.states_state,
        ),
        AppView::ResourceChanges {
            resource_changes,
            has_changes,
            ..
        } => {
            let title = match has_changes {
                Some(true) => "resource changes",
                Some(false) => "resource changes (no changes)",
                None => "resource changes (computing)",
            };
            draw_tree(frame, area, title, resource_changes, &mut app.changes_state)
        }
        AppView::Operations {
            operations_tree, ..
        } => draw_tree(
            frame,
            area,
            "operations",
            operations_tree,
            &mut app.operations_state,
        ),
        AppView::OperationsApply {
            operations_epochs, ..
        } => draw_apply(
            frame,
            area,
            operations_epochs,
            &mut app.operations_apply_state,
        ),
        AppView::Done {
            operations_epochs, ..
        } => draw_apply(
            frame,
            area,
            operations_epochs,
            &mut app.operations_apply_state,
        ),
    }
}

fn draw_status(frame: &mut ratatui::Frame<'_>, area: Rect, app: &TuiApp) {
    let hints = "←/→ tabs  ↑/↓ move  Enter toggle  f follow  q quit";
    let left = match &app.app_view {
        AppView::Start => "planning...",
        AppView::ResourceParams { .. } => "resource params planned",
        AppView::Resources { .. } => "resources planned",
        AppView::ResourceStates { .. } => "resource states fetched",
        AppView::ResourceChanges { has_changes, .. } => match has_changes {
            None => "changes computing...",
            Some(true) => "changes ready",
            Some(false) => "no changes",
        },
        AppView::Operations { .. } => "operations planned",
        AppView::OperationsApply { .. } => "operations applying...",
        AppView::Done { .. } => "complete",
    };

    let stderr_last = app.stderr_tail.iter().last().cloned().unwrap_or_default();

    let lines = vec![
        Line::from(Span::styled(
            format!("{left:<40}"),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(hints, Style::default().fg(Color::DarkGray))),
        Line::from(Span::styled(stderr_last, Style::default().fg(Color::Red))),
    ];

    let para = Paragraph::new(Text::from(lines))
        .block(Block::default().borders(Borders::TOP))
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });

    frame.render_widget(para, area);
}

fn draw_placeholder(frame: &mut ratatui::Frame<'_>, area: Rect, text: &str) {
    let para = Paragraph::new(Text::from(text))
        .block(Block::default().borders(Borders::ALL).title("lusid"))
        .alignment(Alignment::Center);
    frame.render_widget(para, area);
}

fn draw_apply(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    epochs: &[Vec<OperationView>],
    state: &mut OperationsApplyState,
) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)].as_ref())
        .split(area);

    let mut items: Vec<ListItem<'_>> = Vec::new();
    for (epoch_index, operations) in epochs.iter().enumerate() {
        for (operation_index, operation) in operations.iter().enumerate() {
            let status = if operation.is_complete { "✅" } else { "…" };
            let label = format!(
                "[{status}] ({epoch_index}, {operation_index}) {}",
                operation.label
            );
            items.push(ListItem::new(Line::from(Span::raw(label))));
        }
    }

    let mut list_state = ListState::default();
    if let Some(selected) = state.selected_flat {
        list_state.select(Some(selected));
    }
    *list_state.offset_mut() = state.list_offset;

    let height = layout[0].height.saturating_sub(2) as usize;
    if let Some(sel) = state.selected_flat {
        state.ensure_visible_row(sel, height);
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

    frame.render_stateful_widget(ops_list, layout[0], &mut list_state);

    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(sel) = state.selected_flat {
        if let Some((e, o)) = state.flat_index_to_epoch_op.get(sel).copied() {
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

    frame.render_widget(stdout_para, logs_layout[0]);
    frame.render_widget(stderr_para, logs_layout[1]);
}

fn draw_tree(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    title: &str,
    tree: &FlatViewTree,
    state: &mut TreeState,
) {
    let rows = build_visible_rows(tree, state);

    if state.selected_node.is_none() {
        state.selected_node = rows.first().map(|r| r.index);
    }

    let selected_row = selected_row_index(&rows, state);
    let items = rows
        .iter()
        .map(|row| {
            let mut segs = Vec::new();

            let indent = "  ".repeat(row.depth);
            segs.push(Span::raw(indent));

            if row.is_branch {
                segs.push(Span::styled(
                    format!("{} ", if row.is_expanded { "▼" } else { "▶" }),
                    Style::default().fg(Color::Yellow),
                ));
            } else {
                segs.push(Span::styled("• ", Style::default().fg(Color::DarkGray)));
            }

            segs.push(Span::raw(&row.label));
            ListItem::new(Line::from(segs))
        })
        .collect::<Vec<_>>();

    let mut list_state = ListState::default();
    list_state.select(selected_row);
    *list_state.offset_mut() = state.list_offset;

    let inner_height = area.height.saturating_sub(2) as usize;
    if let Some(selected_row) = selected_row {
        state.ensure_visible_row(selected_row, inner_height);
        *list_state.offset_mut() = state.list_offset;
    }

    let widget = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_stateful_widget(widget, area, &mut list_state);
}

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
    let mut visited = HashSet::new();
    build_visible_rows_rec(
        tree,
        FlatViewTree::root_index(),
        0,
        state,
        &mut out,
        &mut visited,
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
                    build_visible_rows_rec(tree, child, depth + 1, state, out, visited);
                }
            }
        }
    }
}

fn selected_row_index(rows: &[TreeRow], state: &TreeState) -> Option<usize> {
    let selected_node = state.selected_node?;
    rows.iter().position(|r| r.index == selected_node)
}

fn tree_move_selection(tree: &FlatViewTree, state: &mut TreeState, delta: i32) {
    let rows = build_visible_rows(tree, state);
    if rows.is_empty() {
        state.selected_node = None;
        state.list_offset = 0;
        return;
    }

    let current_row = selected_row_index(&rows, state).unwrap_or(0);
    let next_row = if delta >= 0 {
        (current_row + delta as usize).min(rows.len() - 1)
    } else {
        current_row.saturating_sub((-delta) as usize)
    };

    state.selected_node = Some(rows[next_row].index);
}

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
