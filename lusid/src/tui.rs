#![allow(clippy::collapsible_if)]

use std::collections::HashSet;
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::io;
use std::pin::Pin;
use std::time::Duration;

use ansi_to_tui::IntoText;
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
use ratatui::layout::Size;
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

    let mut app = TuiState::new();

    let mut stdout_done = false;
    let mut stderr_done = false;

    let mut outcome: Option<Result<(), TuiError>> = None;
    let mut should_quit = false;

    let mut wait = wait;

    loop {
        // State -> ViewModel happens outside rendering.
        // Rendering receives an already-safe, already-prepared view-model.
        let size = terminal_session.terminal.size()?;
        let view_model = app.view_model(size, outcome.as_ref());

        terminal_session
            .terminal
            .draw(|frame| render_ui(frame, &view_model))?;

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

    fn visible_rows(&mut self, tree: &FlatViewTree) -> Vec<TreeRow> {
        let rows = build_visible_rows(tree, self);

        if rows.is_empty() {
            self.selected_node = None;
            self.list_offset = 0;
            return rows;
        }

        let selection_is_visible = self
            .selected_node
            .and_then(|_| selected_row_index(&rows, self))
            .is_some();

        if self.selected_node.is_none() || !selection_is_visible {
            self.selected_node = Some(rows[0].index);
        }

        rows
    }

    fn move_selection(&mut self, tree: &FlatViewTree, delta: i32) {
        let rows = self.visible_rows(tree);
        if rows.is_empty() {
            return;
        }

        let current_row = selected_row_index(&rows, self).unwrap_or(0);

        let next_row = if delta >= 0 {
            (current_row + delta as usize).min(rows.len() - 1)
        } else {
            current_row.saturating_sub((-delta) as usize)
        };

        self.selected_node = Some(rows[next_row].index);
    }

    fn toggle_selected_branch(&mut self, tree: &FlatViewTree) {
        let rows = self.visible_rows(tree);
        if rows.is_empty() {
            return;
        }

        let selected_row = selected_row_index(&rows, self).unwrap_or(0);
        let row = &rows[selected_row];

        if row.is_branch {
            self.toggle(row.index);
        }
    }

    fn view_model(&mut self, title: &str, tree: &FlatViewTree, area: Rect) -> TreeViewModel {
        let rows = self.visible_rows(tree);

        let selected_row = selected_row_index(&rows, self);
        let inner_height = area.height.saturating_sub(2) as usize;

        if let Some(selected_row) = selected_row {
            self.ensure_visible_row(selected_row, inner_height);
        }

        let items = rows
            .iter()
            .map(|row| {
                let mut spans: Vec<Span<'static>> = Vec::new();
                spans.push(Span::raw("  ".repeat(row.depth)));

                if row.is_branch {
                    spans.push(Span::styled(
                        format!("{} ", if row.is_expanded { "▼" } else { "▶" }),
                        Style::default().fg(Color::Yellow),
                    ));
                } else {
                    spans.push(Span::styled("• ", Style::default().fg(Color::DarkGray)));
                }

                spans.push(Span::raw(row.label.clone()));

                ListItem::new(Line::from(spans))
            })
            .collect::<Vec<ListItem<'static>>>();

        TreeViewModel {
            title: title.to_string(),
            items,
            selected: selected_row,
            offset: self.list_offset,
        }
    }
}

#[derive(Debug, Default, Clone)]
struct OperationsApplyState {
    flat_index_to_epoch_operation: Vec<(usize, usize)>,
    selected_flat: Option<usize>,
    list_offset: usize,

    stdout_rendered: Text<'static>,
    stderr_rendered: Text<'static>,
    last_render_key: Option<(usize, usize, u64, u64)>,
    last_ansi_error: Option<String>,
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

        let len = self.flat_index_to_epoch_operation.len();
        if len == 0 {
            self.selected_flat = None;
            self.list_offset = 0;
        } else {
            let sel = self.selected_flat.unwrap_or(0).min(len - 1);
            self.selected_flat = Some(sel);
        }

        self.last_render_key = None;
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

    fn move_down(&mut self) {
        let len = self.visible_len();
        if len == 0 {
            self.selected_flat = None;
            self.list_offset = 0;
            return;
        }

        let selected = self.selected_flat.unwrap_or(0);
        self.selected_flat = Some((selected + 1).min(len.saturating_sub(1)));
    }

    fn move_up(&mut self) {
        let len = self.visible_len();
        if len == 0 {
            self.selected_flat = None;
            self.list_offset = 0;
            return;
        }

        let selected = self.selected_flat.unwrap_or(0);
        self.selected_flat = Some(selected.saturating_sub(1));
    }

    fn refresh_selected_logs(&mut self, epochs: &[Vec<OperationView>]) {
        let Some(sel) = self.selected_flat else {
            self.stdout_rendered = Text::default();
            self.stderr_rendered = Text::default();
            self.last_render_key = None;
            self.last_ansi_error = None;
            return;
        };

        let Some((e, o)) = self.flat_index_to_epoch_operation.get(sel).copied() else {
            self.stdout_rendered = Text::default();
            self.stderr_rendered = Text::default();
            self.last_render_key = None;
            self.last_ansi_error = None;
            return;
        };

        let Some(op) = epochs.get(e).and_then(|v| v.get(o)) else {
            self.stdout_rendered = Text::default();
            self.stderr_rendered = Text::default();
            self.last_render_key = None;
            self.last_ansi_error = None;
            return;
        };

        let out_hash = fingerprint(&op.stdout);
        let err_hash = fingerprint(&op.stderr);

        let key = (e, o, out_hash, err_hash);
        if self.last_render_key == Some(key) {
            return;
        }

        self.last_render_key = Some(key);
        self.last_ansi_error = None;

        let (stdout_text, stdout_err) = ansi_to_text_or_fallback(&op.stdout, "stdout");
        let (stderr_text, stderr_err) = ansi_to_text_or_fallback(&op.stderr, "stderr");

        self.stdout_rendered = stdout_text;
        self.stderr_rendered = stderr_text;

        self.last_ansi_error = stdout_err.or(stderr_err);
    }

    fn view_model(
        &mut self,
        epochs: &[Vec<OperationView>],
        area: Rect,
    ) -> OperationsApplyViewModel {
        if self.flat_index_to_epoch_operation.is_empty() {
            self.rebuild_index(epochs);
        }

        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)].as_ref())
            .split(area);

        let mut items: Vec<ListItem<'static>> = Vec::new();
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

        if self.visible_len() == 0 {
            self.selected_flat = None;
            self.list_offset = 0;
        } else {
            let sel = self.selected_flat.unwrap_or(0).min(self.visible_len() - 1);
            self.selected_flat = Some(sel);
        }

        let list_inner_height = layout[0].height.saturating_sub(2) as usize;
        if let Some(sel) = self.selected_flat {
            self.ensure_visible_row(sel, list_inner_height);
        }

        self.refresh_selected_logs(epochs);

        let mut stdout = self.stdout_rendered.clone();
        let stderr = self.stderr_rendered.clone();

        if let Some(err) = self.last_ansi_error.clone() {
            let header = Line::from(Span::styled(
                format!("ansi-to-tui parse issue: {err}"),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ));

            let mut lines = vec![header, Line::raw("")];
            lines.extend(stdout.lines);
            stdout = Text::from(lines);
        }

        OperationsApplyViewModel {
            operations: ListViewModel {
                title: "operations epochs:".to_string(),
                items,
                selected: self.selected_flat,
                offset: self.list_offset,
            },
            stdout,
            stderr,
        }
    }
}

#[derive(Debug, Clone)]
struct TuiState {
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

impl TuiState {
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
            // Rebuild mapping if shape changed; this keeps selection stable and safe.
            self.operations_apply_state.rebuild_index(epochs);
            self.operations_apply_state.refresh_selected_logs(epochs);
        }

        Ok(())
    }

    fn push_stderr(&mut self, line: String) {
        self.stderr_tail.push(line);
    }

    fn handle_event(&mut self, event: CEvent) -> Result<bool, TuiError> {
        match event {
            CEvent::Key(KeyEvent {
                code, modifiers, ..
            }) => {
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

                        KeyCode::Tab => {
                            self.follow_pipeline = false;
                            self.navigate_stage_relative(1);
                        }

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

            CEvent::Resize(_, _) => {
                // No-op: we clamp offsets during view-model construction.
            }

            _ => {}
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
                self.operations_apply_state.move_down();
                if let Some(epochs) = app_view_epochs(&self.app_view) {
                    self.operations_apply_state.refresh_selected_logs(epochs);
                }
            }
            _ => {
                if let Some((tree, state)) = self.tree_for_stage_mut() {
                    state.move_selection(tree, 1);
                }
            }
        }
    }

    fn move_up(&mut self) {
        match self.stage {
            PipelineStage::OperationsEpochs => {
                self.operations_apply_state.move_up();
                if let Some(epochs) = app_view_epochs(&self.app_view) {
                    self.operations_apply_state.refresh_selected_logs(epochs);
                }
            }
            _ => {
                if let Some((tree, state)) = self.tree_for_stage_mut() {
                    state.move_selection(tree, -1);
                }
            }
        }
    }

    fn toggle_selected(&mut self) {
        if let Some((tree, state)) = self.tree_for_stage_mut() {
            state.toggle_selected_branch(tree);
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

    fn view_model(&mut self, size: Size, outcome: Option<&Result<(), TuiError>>) -> TuiViewModel {
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
            .split(Rect {
                x: 0,
                y: 0,
                width: size.width,
                height: size.height,
            });

        let pipeline = self.pipeline_view_model(outcome);
        let main = self.main_view_model(layout[1]);
        let status = self.status_view_model(outcome);

        TuiViewModel {
            pipeline_area: layout[0],
            main_area: layout[1],
            status_area: layout[2],
            pipeline,
            main,
            status,
        }
    }

    fn pipeline_view_model(&self, outcome: Option<&Result<(), TuiError>>) -> PipelineViewModel {
        let mut pipeline_spans: Vec<Span<'static>> = Vec::new();

        for (index, stage) in PipelineStage::ALL.iter().copied().enumerate() {
            if index > 0 {
                pipeline_spans.push(Span::styled(
                    " -> ".to_string(),
                    Style::default().fg(Color::DarkGray),
                ));
            }

            let available = stage.is_available(&self.app_view);
            let selected = stage == self.stage;

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

            pipeline_spans.push(Span::styled(stage.label().to_string(), style));
        }

        let feedback = pipeline_feedback_line(self, outcome);

        let lines = vec![
            Line::from(pipeline_spans),
            Line::from(Span::styled(feedback, Style::default().fg(Color::Yellow))),
        ];

        PipelineViewModel {
            title: if self.follow_pipeline {
                "pipeline (following)".to_string()
            } else {
                "pipeline".to_string()
            },
            text: Text::from(lines),
        }
    }

    fn main_view_model(&mut self, area: Rect) -> MainViewModel {
        match self.stage {
            PipelineStage::ResourceParams => match app_view_params(&self.app_view) {
                Some(tree) => {
                    MainViewModel::Tree(self.params_state.view_model("resource params", tree, area))
                }
                None => MainViewModel::Placeholder(PlaceholderViewModel {
                    title: "lusid".to_string(),
                    text: Text::from("Waiting for resource params..."),
                    alignment: Alignment::Center,
                }),
            },

            PipelineStage::Resources => match app_view_resources(&self.app_view) {
                Some(tree) => {
                    MainViewModel::Tree(self.resources_state.view_model("resources", tree, area))
                }
                None => MainViewModel::Placeholder(PlaceholderViewModel {
                    title: "lusid".to_string(),
                    text: Text::from("Resources are not available yet."),
                    alignment: Alignment::Center,
                }),
            },

            PipelineStage::ResourceStates => match app_view_states(&self.app_view) {
                Some(tree) => {
                    MainViewModel::Tree(self.states_state.view_model("resource states", tree, area))
                }
                None => MainViewModel::Placeholder(PlaceholderViewModel {
                    title: "lusid".to_string(),
                    text: Text::from("Resource states are not available yet."),
                    alignment: Alignment::Center,
                }),
            },

            PipelineStage::ResourceChanges => match app_view_changes(&self.app_view) {
                Some(tree) => MainViewModel::Tree(self.changes_state.view_model(
                    "resource changes",
                    tree,
                    area,
                )),
                None => MainViewModel::Placeholder(PlaceholderViewModel {
                    title: "lusid".to_string(),
                    text: Text::from("Resource changes are not available yet."),
                    alignment: Alignment::Center,
                }),
            },

            PipelineStage::OperationsTree => match app_view_operations(&self.app_view) {
                Some(tree) => MainViewModel::Tree(self.operations_state.view_model(
                    "operations tree",
                    tree,
                    area,
                )),
                None => MainViewModel::Placeholder(PlaceholderViewModel {
                    title: "lusid".to_string(),
                    text: Text::from("Operations tree is not available yet."),
                    alignment: Alignment::Center,
                }),
            },

            PipelineStage::OperationsEpochs => match app_view_epochs(&self.app_view) {
                Some(epochs) => MainViewModel::OperationsApply(
                    self.operations_apply_state.view_model(epochs, area),
                ),
                None => MainViewModel::Placeholder(PlaceholderViewModel {
                    title: "lusid".to_string(),
                    text: Text::from("Operations epochs are not available."),
                    alignment: Alignment::Center,
                }),
            },
        }
    }

    fn status_view_model(&self, outcome: Option<&Result<(), TuiError>>) -> StatusViewModel {
        let hints =
            "Left and Right navigate stages  Up and Down move  Enter toggles tree  f follow  q quit";

        let phase = match &self.app_view {
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

        let stderr_last = self.stderr_tail.iter().last().cloned().unwrap_or_default();

        let lines = vec![
            Line::from(Span::styled(
                format!("{phase:<40}"),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                hints.to_string(),
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                if !outcome_line.is_empty() {
                    outcome_line
                } else {
                    stderr_last
                },
                Style::default().fg(Color::Red),
            )),
        ];

        StatusViewModel {
            text: Text::from(lines),
        }
    }
}

/* ----------------------------- View model types ---------------------------- */

#[derive(Debug, Clone)]
struct TuiViewModel {
    pipeline_area: Rect,
    main_area: Rect,
    status_area: Rect,

    pipeline: PipelineViewModel,
    main: MainViewModel,
    status: StatusViewModel,
}

#[derive(Debug, Clone)]
struct PipelineViewModel {
    title: String,
    text: Text<'static>,
}

#[derive(Debug, Clone)]
struct StatusViewModel {
    text: Text<'static>,
}

#[derive(Debug, Clone)]
enum MainViewModel {
    Placeholder(PlaceholderViewModel),
    Tree(TreeViewModel),
    OperationsApply(OperationsApplyViewModel),
}

#[derive(Debug, Clone)]
struct PlaceholderViewModel {
    title: String,
    text: Text<'static>,
    alignment: Alignment,
}

#[derive(Debug, Clone)]
struct TreeViewModel {
    title: String,
    items: Vec<ListItem<'static>>,
    selected: Option<usize>,
    offset: usize,
}

#[derive(Debug, Clone)]
struct ListViewModel {
    title: String,
    items: Vec<ListItem<'static>>,
    selected: Option<usize>,
    offset: usize,
}

#[derive(Debug, Clone)]
struct OperationsApplyViewModel {
    operations: ListViewModel,
    stdout: Text<'static>,
    stderr: Text<'static>,
}

/* -------------------------------- Rendering -------------------------------- */

fn render_ui(frame: &mut ratatui::Frame, vm: &TuiViewModel) {
    render_pipeline(frame, vm.pipeline_area, &vm.pipeline);
    render_main(frame, vm.main_area, &vm.main);
    render_status(frame, vm.status_area, &vm.status);
}

fn render_pipeline(frame: &mut ratatui::Frame, area: Rect, vm: &PipelineViewModel) {
    let widget = Paragraph::new(vm.text.clone())
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .title(vm.title.clone()),
        )
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });

    frame.render_widget(widget, area);
}

fn render_status(frame: &mut ratatui::Frame, area: Rect, vm: &StatusViewModel) {
    let widget = Paragraph::new(vm.text.clone())
        .block(Block::default().borders(Borders::TOP))
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });

    frame.render_widget(widget, area);
}

fn render_main(frame: &mut ratatui::Frame, area: Rect, vm: &MainViewModel) {
    match vm {
        MainViewModel::Placeholder(ph) => {
            let widget = Paragraph::new(ph.text.clone())
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(ph.title.clone()),
                )
                .alignment(ph.alignment);

            frame.render_widget(widget, area);
        }

        MainViewModel::Tree(tree) => render_list(
            frame,
            area,
            &tree.title,
            &tree.items,
            tree.selected,
            tree.offset,
        ),

        MainViewModel::OperationsApply(apply) => render_apply(frame, area, apply),
    }
}

fn render_list(
    frame: &mut ratatui::Frame,
    area: Rect,
    title: &str,
    items: &[ListItem<'static>],
    selected: Option<usize>,
    offset: usize,
) {
    let mut list_state = ListState::default();
    list_state.select(selected);
    *list_state.offset_mut() = offset;

    let widget = List::new(items.to_vec())
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_stateful_widget(widget, area, &mut list_state);
}

fn render_apply(frame: &mut ratatui::Frame<'_>, area: Rect, vm: &OperationsApplyViewModel) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)].as_ref())
        .split(area);

    let mut list_state = ListState::default();
    list_state.select(vm.operations.selected);
    *list_state.offset_mut() = vm.operations.offset;

    let operations_list = List::new(vm.operations.items.clone())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(vm.operations.title.clone()),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_stateful_widget(operations_list, layout[0], &mut list_state);

    let logs_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)].as_ref())
        .split(layout[1]);

    let stdout_widget = Paragraph::new(vm.stdout.clone())
        .block(Block::default().borders(Borders::ALL).title("stdout"))
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(Color::White));

    let stderr_widget = Paragraph::new(vm.stderr.clone())
        .block(Block::default().borders(Borders::ALL).title("stderr"))
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(Color::Red));

    frame.render_widget(stdout_widget, logs_layout[0]);
    frame.render_widget(stderr_widget, logs_layout[1]);
}

/* --------------------------- AppView access helpers -------------------------- */

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

/* ---------------------------- Pipeline feedback ---------------------------- */

fn pipeline_feedback_line(app: &TuiState, outcome: Option<&Result<(), TuiError>>) -> String {
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

/* ------------------------------- Tree rows -------------------------------- */

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

/* --------------------------- ANSI conversion utils -------------------------- */

fn ansi_to_text_or_fallback(input: &str, label: &str) -> (Text<'static>, Option<String>) {
    if input.is_empty() {
        return (Text::default(), None);
    }

    match input.into_text() {
        Ok(text) => (text.to_owned(), None),
        Err(err) => {
            let msg = format!("{label} parse error: {err}");
            let fallback = Text::from(input.to_string());
            (fallback, Some(msg))
        }
    }
}

// A cheap, deterministic fingerprint so we only re-parse ANSI when content changes.
// (Not cryptographic.)
fn fingerprint(s: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;

    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/* ------------------------------ Small helpers ------------------------------ */

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
