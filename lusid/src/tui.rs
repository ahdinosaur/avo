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
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
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

    #[error("terminal initialization failed")]
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

    let mut outcome: Option<Result<(), TuiError>> = None;
    let mut should_quit = false;

    tokio::pin!(wait);

    loop {
        terminal_session
            .terminal
            .draw(|frame| draw_ui(frame, &mut app, outcome.as_ref()))?;

        tokio::select! {
            result = &mut wait, if outcome.is_none() => {
                app.child_exited = true;
                outcome = Some(result.map_err(Into::into));
            }

            line = stdout_lines.next_line(), if !stdout_done => {
                match line {
                    Ok(Some(line)) => {
                        if !line.trim().is_empty() {
                            let update: AppUpdate = serde_json::from_str(&line)?;
                            app.apply_update(update)?;
                        }
                    }
                    Ok(None) => stdout_done = true,
                    Err(err) => {
                        app.child_exited = true;
                        outcome = Some(Err(TuiError::ReadApplyStdout(err)));
                        stdout_done = true;
                    }
                }
            }

            line = stderr_lines.next_line(), if !stderr_done => {
                match line {
                    Ok(Some(line)) => app.push_stderr(line),
                    Ok(None) => stderr_done = true,
                    Err(err) => {
                        app.child_exited = true;
                        outcome = Some(Err(TuiError::ReadApplyStderr(err)));
                        stderr_done = true;
                    }
                }
            }

            Some(event) = event_rx.recv() => {
                should_quit = app.handle_event(event)?;
            }

            _ = tick.tick() => {}
        }

        if should_quit {
            break;
        }

        let _io_closed = stdout_done && stderr_done;
        let _ = _io_closed;
    }

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
enum PipelineStage {
    ResourceParams,
    Resources,
    ResourceStates,
    ResourceChanges,
    OperationsTree,
    OperationsEpochs,
}

impl PipelineStage {
    const ALL: [PipelineStage; 6] = [
        PipelineStage::ResourceParams,
        PipelineStage::Resources,
        PipelineStage::ResourceStates,
        PipelineStage::ResourceChanges,
        PipelineStage::OperationsTree,
        PipelineStage::OperationsEpochs,
    ];

    fn label(self) -> &'static str {
        match self {
            PipelineStage::ResourceParams => "resource params",
            PipelineStage::Resources => "resources",
            PipelineStage::ResourceStates => "resource states",
            PipelineStage::ResourceChanges => "resource changes",
            PipelineStage::OperationsTree => "operations tree",
            PipelineStage::OperationsEpochs => "operations epochs",
        }
    }

    fn index(self) -> usize {
        PipelineStage::ALL
            .iter()
            .position(|s| *s == self)
            .expect("PipelineStage must be in ALL")
    }

    fn from_index(index: usize) -> Self {
        PipelineStage::ALL[index]
    }

    fn is_available(self, view: &AppView) -> bool {
        match self {
            PipelineStage::ResourceParams => app_view_params(view).is_some(),
            PipelineStage::Resources => app_view_resources(view).is_some(),
            PipelineStage::ResourceStates => app_view_states(view).is_some(),
            PipelineStage::ResourceChanges => app_view_changes(view).is_some(),
            PipelineStage::OperationsTree => app_view_operations(view).is_some(),
            PipelineStage::OperationsEpochs => app_view_epochs(view).is_some(),
        }
    }

    fn from_app_view(view: &AppView) -> PipelineStage {
        match view {
            AppView::Start => PipelineStage::ResourceParams,
            AppView::ResourceParams { .. } => PipelineStage::ResourceParams,
            AppView::Resources { .. } => PipelineStage::Resources,
            AppView::ResourceStates { .. } => PipelineStage::ResourceStates,
            AppView::ResourceChanges { .. } => PipelineStage::ResourceChanges,
            AppView::Operations { .. } => PipelineStage::OperationsTree,
            AppView::OperationsApply { .. } => PipelineStage::OperationsEpochs,
            AppView::Done { .. } => PipelineStage::OperationsEpochs,
        }
    }
}

#[derive(Debug, Default, Clone)]
struct TreeState {
    collapsed: HashSet<usize>,
    selected_node: Option<usize>,
    list_offset: usize,
}

impl TreeState {
    fn toggle(&mut self, node_index: usize) {
        if self.collapsed.contains(&node_index) {
            self.collapsed.remove(&node_index);
        } else {
            self.collapsed.insert(node_index);
        }
    }

    fn is_expanded(&self, node_index: usize) -> bool {
        !self.collapsed.contains(&node_index)
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
    flat_index_to_epoch_operation: Vec<(usize, usize)>,
    selected_flat: Option<usize>,
    list_offset: usize,
}

impl OperationsApplyState {
    fn rebuild_index(&mut self, epochs: &[Vec<OperationView>]) {
        self.flat_index_to_epoch_operation.clear();

        for (epoch_index, operations) in epochs.iter().enumerate() {
            for (operation_index, _) in operations.iter().enumerate() {
                self.flat_index_to_epoch_operation
                    .push((epoch_index, operation_index));
            }
        }

        if self.flat_index_to_epoch_operation.is_empty() {
            self.selected_flat = None;
            self.list_offset = 0;
        } else {
            let sel = self
                .selected_flat
                .unwrap_or(0)
                .min(self.flat_index_to_epoch_operation.len() - 1);
            self.selected_flat = Some(sel);
        }
    }

    fn visible_len(&self) -> usize {
        self.flat_index_to_epoch_operation.len()
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
    stage: PipelineStage,
    follow_pipeline: bool,

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
        Self {
            app_view: AppView::default(),
            stage: PipelineStage::ResourceParams,
            follow_pipeline: true,

            params_state: TreeState::default(),
            resources_state: TreeState::default(),
            states_state: TreeState::default(),
            changes_state: TreeState::default(),
            operations_state: TreeState::default(),

            operations_apply_state: OperationsApplyState::default(),

            stderr_tail: CircularBuffer::new(200),
            child_exited: false,
        }
    }

    fn apply_update(&mut self, update: AppUpdate) -> Result<(), TuiError> {
        let current = std::mem::take(&mut self.app_view);

        self.app_view = current.update(update)?;

        if self.follow_pipeline {
            let next = PipelineStage::from_app_view(&self.app_view);
            if next.is_available(&self.app_view) {
                self.stage = next;
            }
        }

        if let Some(epochs) = app_view_epochs(&self.app_view) {
            self.operations_apply_state.rebuild_index(epochs);
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
                        self.follow_pipeline = !self.follow_pipeline;
                        if self.follow_pipeline {
                            let next = PipelineStage::from_app_view(&self.app_view);
                            if next.is_available(&self.app_view) {
                                self.stage = next;
                            }
                        }
                    }

                    KeyCode::Left => {
                        self.follow_pipeline = false;
                        self.navigate_stage_relative(-1);
                    }

                    KeyCode::Right => {
                        self.follow_pipeline = false;
                        self.navigate_stage_relative(1);
                    }

                    // Optional: keep Tab behavior as another way to move between stages.
                    KeyCode::Tab => {
                        self.follow_pipeline = false;
                        self.navigate_stage_relative(1);
                    }

                    // Optional: keep Shift-Tab behavior as another way to move between stages.
                    KeyCode::BackTab => {
                        self.follow_pipeline = false;
                        self.navigate_stage_relative(-1);
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

    fn navigate_stage_relative(&mut self, direction: i32) {
        if direction == 0 {
            return;
        }

        let current_index = self.stage.index();

        if direction > 0 {
            for next_index in (current_index + 1)..PipelineStage::ALL.len() {
                let candidate = PipelineStage::from_index(next_index);
                if candidate.is_available(&self.app_view) {
                    self.stage = candidate;
                    return;
                }
            }
        } else {
            for next_index in (0..current_index).rev() {
                let candidate = PipelineStage::from_index(next_index);
                if candidate.is_available(&self.app_view) {
                    self.stage = candidate;
                    return;
                }
            }
        }
    }

    fn move_down(&mut self) {
        match self.stage {
            PipelineStage::OperationsEpochs => {
                let len = self.operations_apply_state.visible_len();
                if len == 0 {
                    return;
                }
                let selected = self.operations_apply_state.selected_flat.unwrap_or(0);
                self.operations_apply_state.selected_flat =
                    Some((selected + 1).min(len.saturating_sub(1)));
            }
            _ => {
                if let Some((tree, state)) = self.tree_for_stage_mut() {
                    tree_move_selection(tree, state, 1);
                }
            }
        }
    }

    fn move_up(&mut self) {
        match self.stage {
            PipelineStage::OperationsEpochs => {
                let selected = self.operations_apply_state.selected_flat.unwrap_or(0);
                self.operations_apply_state.selected_flat = Some(selected.saturating_sub(1));
            }
            _ => {
                if let Some((tree, state)) = self.tree_for_stage_mut() {
                    tree_move_selection(tree, state, -1);
                }
            }
        }
    }

    fn toggle_selected(&mut self) {
        if let Some((tree, state)) = self.tree_for_stage_mut() {
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

    fn tree_for_stage_mut(&mut self) -> Option<(&FlatViewTree, &mut TreeState)> {
        match self.stage {
            PipelineStage::ResourceParams => {
                app_view_params(&self.app_view).map(|tree| (tree, &mut self.params_state))
            }
            PipelineStage::Resources => {
                app_view_resources(&self.app_view).map(|tree| (tree, &mut self.resources_state))
            }
            PipelineStage::ResourceStates => {
                app_view_states(&self.app_view).map(|tree| (tree, &mut self.states_state))
            }
            PipelineStage::ResourceChanges => {
                app_view_changes(&self.app_view).map(|tree| (tree, &mut self.changes_state))
            }
            PipelineStage::OperationsTree => {
                app_view_operations(&self.app_view).map(|tree| (tree, &mut self.operations_state))
            }
            PipelineStage::OperationsEpochs => None,
        }
    }
}

fn app_view_params(view: &AppView) -> Option<&FlatViewTree> {
    match view {
        AppView::ResourceParams { resource_params } => Some(resource_params),
        AppView::Resources {
            resource_params, ..
        } => Some(resource_params),
        AppView::ResourceStates {
            resource_params, ..
        } => Some(resource_params),
        AppView::ResourceChanges {
            resource_params, ..
        } => Some(resource_params),
        AppView::Operations {
            resource_params, ..
        } => Some(resource_params),
        AppView::OperationsApply {
            resource_params, ..
        } => Some(resource_params),
        AppView::Done {
            resource_params, ..
        } => Some(resource_params),
        AppView::Start => None,
    }
}

fn app_view_resources(view: &AppView) -> Option<&FlatViewTree> {
    match view {
        AppView::Resources { resources, .. } => Some(resources),
        AppView::ResourceStates { resources, .. } => Some(resources),
        AppView::ResourceChanges { resources, .. } => Some(resources),
        AppView::Operations { resources, .. } => Some(resources),
        AppView::OperationsApply { resources, .. } => Some(resources),
        AppView::Done { resources, .. } => Some(resources),
        _ => None,
    }
}

fn app_view_states(view: &AppView) -> Option<&FlatViewTree> {
    match view {
        AppView::ResourceStates {
            resource_states, ..
        } => Some(resource_states),
        AppView::ResourceChanges {
            resource_states, ..
        } => Some(resource_states),
        AppView::Operations {
            resource_states, ..
        } => Some(resource_states),
        AppView::OperationsApply {
            resource_states, ..
        } => Some(resource_states),
        AppView::Done {
            resource_states, ..
        } => Some(resource_states),
        _ => None,
    }
}

fn app_view_changes(view: &AppView) -> Option<&FlatViewTree> {
    match view {
        AppView::ResourceChanges {
            resource_changes, ..
        } => Some(resource_changes),
        AppView::Operations {
            resource_changes, ..
        } => Some(resource_changes),
        AppView::OperationsApply {
            resource_changes, ..
        } => Some(resource_changes),
        AppView::Done {
            resource_changes, ..
        } => Some(resource_changes),
        _ => None,
    }
}

fn app_view_operations(view: &AppView) -> Option<&FlatViewTree> {
    match view {
        AppView::Operations {
            operations_tree, ..
        } => Some(operations_tree),
        AppView::OperationsApply {
            operations_tree, ..
        } => Some(operations_tree),
        AppView::Done {
            operations_tree, ..
        } => Some(operations_tree),
        _ => None,
    }
}

fn app_view_epochs(view: &AppView) -> Option<&Vec<Vec<OperationView>>> {
    match view {
        AppView::OperationsApply {
            operations_epochs, ..
        } => Some(operations_epochs),
        AppView::Done {
            operations_epochs, ..
        } => Some(operations_epochs),
        _ => None,
    }
}

fn draw_ui(frame: &mut ratatui::Frame, app: &mut TuiApp, outcome: Option<&Result<(), TuiError>>) {
    let size = frame.size();

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(4),
                Constraint::Min(5),
                Constraint::Length(3),
            ]
            .as_ref(),
        )
        .split(size);

    draw_pipeline(frame, layout[0], app, outcome);
    draw_main(frame, layout[1], app);
    draw_status(frame, layout[2], app, outcome);
}

fn draw_pipeline(
    frame: &mut ratatui::Frame,
    area: Rect,
    app: &TuiApp,
    outcome: Option<&Result<(), TuiError>>,
) {
    let mut pipeline_spans: Vec<Span> = Vec::new();

    for (index, stage) in PipelineStage::ALL.iter().copied().enumerate() {
        if index > 0 {
            pipeline_spans.push(Span::styled(" -> ", Style::default().fg(Color::DarkGray)));
        }

        let available = stage.is_available(&app.app_view);
        let selected = stage == app.stage;

        let style = match (available, selected) {
            (true, true) => Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            (true, false) => Style::default().fg(Color::White),
            (false, true) => Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::CROSSED_OUT),
            (false, false) => Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::CROSSED_OUT),
        };

        pipeline_spans.push(Span::styled(stage.label(), style));
    }

    let feedback = pipeline_feedback_line(app, outcome);

    let lines = vec![
        Line::from(pipeline_spans),
        Line::from(Span::styled(feedback, Style::default().fg(Color::Yellow))),
    ];

    let widget = Paragraph::new(Text::from(lines))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .title(if app.follow_pipeline {
                    "pipeline (following)"
                } else {
                    "pipeline"
                }),
        )
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });

    frame.render_widget(widget, area);
}

fn pipeline_feedback_line(app: &TuiApp, outcome: Option<&Result<(), TuiError>>) -> String {
    if let Some(Err(err)) = outcome {
        return format!("Process error: {err}");
    }

    match &app.app_view {
        AppView::Start => "Waiting for planning output...".to_string(),

        AppView::ResourceParams { .. } => "Resource parameters planned.".to_string(),

        AppView::Resources { .. } => "Resources planned.".to_string(),

        AppView::ResourceStates { .. } => "Resource states are being fetched.".to_string(),

        AppView::ResourceChanges { has_changes, .. } => match has_changes {
            None => "Computing resource changes...".to_string(),
            Some(false) => "No changes.".to_string(),
            Some(true) => "Changes detected.".to_string(),
        },

        AppView::Operations { .. } => "Operations tree planned.".to_string(),

        AppView::OperationsApply { .. } => "Applying operations epochs.".to_string(),

        AppView::Done { .. } => {
            if app.child_exited {
                "Complete.".to_string()
            } else {
                "Complete (waiting for process to exit)...".to_string()
            }
        }
    }
}

fn draw_main(frame: &mut ratatui::Frame<'_>, area: Rect, app: &mut TuiApp) {
    match app.stage {
        PipelineStage::ResourceParams => match app_view_params(&app.app_view) {
            Some(tree) => draw_tree(frame, area, "resource params", tree, &mut app.params_state),
            None => draw_placeholder(frame, area, "Waiting for resource params..."),
        },

        PipelineStage::Resources => match app_view_resources(&app.app_view) {
            Some(tree) => draw_tree(frame, area, "resources", tree, &mut app.resources_state),
            None => draw_placeholder(frame, area, "Resources are not available yet."),
        },

        PipelineStage::ResourceStates => match app_view_states(&app.app_view) {
            Some(tree) => draw_tree(frame, area, "resource states", tree, &mut app.states_state),
            None => draw_placeholder(frame, area, "Resource states are not available yet."),
        },

        PipelineStage::ResourceChanges => match app_view_changes(&app.app_view) {
            Some(tree) => draw_tree(
                frame,
                area,
                "resource changes",
                tree,
                &mut app.changes_state,
            ),
            None => draw_placeholder(frame, area, "Resource changes are not available yet."),
        },

        PipelineStage::OperationsTree => match app_view_operations(&app.app_view) {
            Some(tree) => draw_tree(
                frame,
                area,
                "operations tree",
                tree,
                &mut app.operations_state,
            ),
            None => draw_placeholder(frame, area, "Operations tree is not available yet."),
        },

        PipelineStage::OperationsEpochs => match app_view_epochs(&app.app_view) {
            Some(epochs) => draw_apply(frame, area, epochs, &mut app.operations_apply_state),
            None => draw_placeholder(frame, area, "Operations epochs are not available."),
        },
    }
}

fn draw_status(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    app: &TuiApp,
    outcome: Option<&Result<(), TuiError>>,
) {
    let hints =
        "Left and Right navigate stages  Up and Down move  Enter toggles tree  f follow  q quit";

    let phase = match &app.app_view {
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

    let outcome_line = match outcome {
        None => String::new(),
        Some(Ok(())) => "process exited successfully".to_string(),
        Some(Err(err)) => format!("process error: {err}"),
    };

    let stderr_last = app.stderr_tail.iter().last().cloned().unwrap_or_default();

    let lines = vec![
        Line::from(Span::styled(
            format!("{phase:<40}"),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(hints, Style::default().fg(Color::DarkGray))),
        Line::from(Span::styled(
            if !outcome_line.is_empty() {
                outcome_line
            } else {
                stderr_last
            },
            Style::default().fg(Color::Red),
        )),
    ];

    let widget = Paragraph::new(Text::from(lines))
        .block(Block::default().borders(Borders::TOP))
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });

    frame.render_widget(widget, area);
}

fn draw_placeholder(frame: &mut ratatui::Frame<'_>, area: Rect, text: &str) {
    let widget = Paragraph::new(Text::from(text))
        .block(Block::default().borders(Borders::ALL).title("lusid"))
        .alignment(Alignment::Center);
    frame.render_widget(widget, area);
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

    if state.flat_index_to_epoch_operation.is_empty() {
        state.rebuild_index(epochs);
    }

    let mut items: Vec<ListItem<'_>> = Vec::new();
    for (epoch_index, operations) in epochs.iter().enumerate() {
        for (operation_index, operation) in operations.iter().enumerate() {
            let status = if operation.is_complete { "✅" } else { "…" };
            let label = format!(
                "[{status}] (epoch {epoch_index}, operation {operation_index}) {}",
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

    let operations_list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("operations epochs:"),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_stateful_widget(operations_list, layout[0], &mut list_state);

    let mut stdout = String::new();
    let mut stderr = String::new();

    if let Some(sel) = state.selected_flat {
        if let Some((e, o)) = state.flat_index_to_epoch_operation.get(sel).copied() {
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

    let stdout_widget = Paragraph::new(stdout)
        .block(Block::default().borders(Borders::ALL).title("stdout"))
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(Color::White));

    let stderr_widget = Paragraph::new(stderr)
        .block(Block::default().borders(Borders::ALL).title("stderr"))
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(Color::Red));

    frame.render_widget(stdout_widget, logs_layout[0]);
    frame.render_widget(stderr_widget, logs_layout[1]);
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
            let mut spans: Vec<Span> = Vec::new();
            spans.push(Span::raw("  ".repeat(row.depth)));

            if row.is_branch {
                spans.push(Span::styled(
                    format!("{} ", if row.is_expanded { "▼" } else { "▶" }),
                    Style::default().fg(Color::Yellow),
                ));
            } else {
                spans.push(Span::styled("• ", Style::default().fg(Color::DarkGray)));
            }

            spans.push(Span::raw(&row.label));

            ListItem::new(Line::from(spans))
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
            let is_expanded = state.is_expanded(index);

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
