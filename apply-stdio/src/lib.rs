use lusid_view::{Line, View};
use serde::{Deserialize, Serialize};
use std::fmt::Display;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ViewNode {
    NotStarted,
    Started(Option<View>),
    Complete(View),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ViewTree {
    Branch { view: View, children: Vec<ViewTree> },
    Leaf { view: View },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FlatViewTreeNode {
    Branch {
        view: ViewNode,
        children: Vec<usize>,
    },
    Leaf {
        view: ViewNode,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlatViewTree {
    nodes: Vec<Option<FlatViewTreeNode>>,
}

pub enum AppUpdate {
    ResourceParams {
        resource_params: ViewTree,
    },
    ResourcesStart,
    ResourcesNode {
        index: usize,
        value: ViewTree,
    },
    ResourcesComplete,
    ResourceStatesStart,
    ResourceStatesStartNode {
        index: usize,
    },
    ResourceStatesCompleteNode {
        index: usize,
        value: Option<ViewTree>,
    },
    ResourceStatesComplete,
    ResourceChangesStart,
    ResourceChangesNode {
        index: usize,
        value: ViewTree,
    },
    ResourceChangesComplete,
    OperationsStart,
    OperationsNode {
        index: usize,
        operations: ViewTree,
    },
    OperationsComplete,
    OperationsApplyStart {
        operations: Vec<Vec<View>>,
    },
    OperationApplyStart {
        index: (usize, usize),
    },
    OperationApplyStdout {
        index: (usize, usize),
        stdout: String,
    },
    OperationApplyStderr {
        index: (usize, usize),
        stderr: String,
    },
    OperationApplyComplete {
        index: (usize, usize),
    },
    OperationsApplyComplete,
}

pub struct AppView {
    resource_params: Option<FlatViewTree>,
    resources: Option<FlatViewTree>,
    resource_states: Option<FlatViewTree>,
    resource_changes: Option<FlatViewTree>,
    operations_tree: Option<FlatViewTree>,
    operations_epochs: Option<FlatViewTree>,
}

impl AppView {
    pub fn update(&mut self, update: AppUpdate) {
        match update {
            AppUpdate::ResourceParams { resource_params } => todo!(),
            AppUpdate::ResourcesStart => todo!(),
            AppUpdate::ResourcesNode { index, value } => todo!(),
            AppUpdate::ResourcesComplete => todo!(),
            AppUpdate::ResourceStatesStart => todo!(),
            AppUpdate::ResourceStatesStartNode { index } => todo!(),
            AppUpdate::ResourceStatesCompleteNode { index, value } => todo!(),
            AppUpdate::ResourceStatesComplete => todo!(),
            AppUpdate::ResourceChangesStart => todo!(),
            AppUpdate::ResourceChangesNode { index, value } => todo!(),
            AppUpdate::ResourceChangesComplete => todo!(),
            AppUpdate::OperationsStart => todo!(),
            AppUpdate::OperationsNode { index, operations } => todo!(),
            AppUpdate::OperationsComplete => todo!(),
            AppUpdate::OperationsApplyStart { operations } => todo!(),
            AppUpdate::OperationApplyStart { index } => todo!(),
            AppUpdate::OperationApplyStdout { index, stdout } => todo!(),
            AppUpdate::OperationApplyStderr { index, stderr } => todo!(),
            AppUpdate::OperationApplyComplete { index } => todo!(),
            AppUpdate::OperationsApplyComplete => todo!(),
        }
    }
}
