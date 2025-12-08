#![allow(clippy::collapsible_if)]

use lusid_view::{View, ViewNode, ViewTree};
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone)]
pub enum FlatTreeUpdate<Node, Meta> {
    Node(FlatViewTreeNode),
    SubTree(ViewTree),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FlatViewTree {
    nodes: Vec<Option<FlatViewTreeNode>>,
}

impl FlatViewTree {
    pub fn from_view_tree_completed(view_tree: ViewTree) -> Self {
        let mut flat_tree = FlatViewTree::default();
        flat_tree.insert_subtree_at_completed(0, view_tree);
        flat_tree
    }

    pub fn insert_subtree_at_completed(
        &mut self,
        start_index: usize,
        view_tree: ViewTree,
    ) -> usize {
        fn helper(flat_tree: &mut FlatViewTree, index: usize, view_tree: ViewTree) -> usize {
            match view_tree {
                ViewTree::Leaf { view } => {
                    flat_tree.set_node(
                        index,
                        FlatViewTreeNode::Leaf {
                            view: ViewNode::Complete(view),
                        },
                    );
                    index + 1
                }
                ViewTree::Branch { view, children } => {
                    let mut next_free_index = index + 1;
                    let mut child_indices: Vec<usize> = Vec::with_capacity(children.len());

                    for child in children {
                        let child_index = next_free_index;
                        next_free_index = helper(flat_tree, child_index, child);
                        child_indices.push(child_index);
                    }

                    flat_tree.set_node(
                        index,
                        FlatViewTreeNode::Branch {
                            view: ViewNode::Complete(view),
                            children: child_indices,
                        },
                    );

                    next_free_index
                }
            }
        }

        self.ensure_index_exists(start_index);
        helper(self, start_index, view_tree)
    }

    pub fn set_leaf_started(&mut self, index: usize) {
        self.set_node(
            index,
            FlatViewTreeNode::Leaf {
                view: ViewNode::Started,
            },
        );
    }

    pub fn set_node_none(&mut self, index: usize) {
        self.ensure_index_exists(index);
        self.nodes[index] = None;
    }

    pub fn set_node_view(&mut self, index: usize, new_view: ViewNode) {
        self.ensure_index_exists(index);
        match self.nodes[index].as_mut() {
            Some(FlatViewTreeNode::Leaf { view }) => {
                *view = new_view;
            }
            Some(FlatViewTreeNode::Branch { view, .. }) => {
                *view = new_view;
            }
            None => {
                self.nodes[index] = Some(FlatViewTreeNode::Leaf { view: new_view });
            }
        }
    }

    pub fn set_node(&mut self, index: usize, node: FlatViewTreeNode) {
        self.ensure_index_exists(index);
        self.nodes[index] = Some(node);
    }

    pub fn get_node(&self, index: usize) -> Option<&FlatViewTreeNode> {
        self.nodes.get(index).and_then(|o| o.as_ref())
    }

    fn ensure_index_exists(&mut self, index: usize) {
        if self.nodes.len() <= index {
            self.nodes.resize(index + 1, None);
        }
    }
}

pub enum AppUpdate {
    ResourceParams {
        resource_params: ViewTree,
    },
    ResourcesStart,
    ResourcesNode {
        index: usize,
        update: FlatTreeUpdate,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationView {
    label: ViewNode,
    stdout: String,
    stderr: String,
    is_complete: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppView {
    resource_params: Option<FlatViewTree>,
    resources: Option<FlatViewTree>,
    resource_states: Option<FlatViewTree>,
    resource_changes: Option<FlatViewTree>,
    operations_tree: Option<FlatViewTree>,
    operations_epochs: Vec<Vec<OperationView>>,
}

impl AppView {
    fn ensure_tree(option: &mut Option<FlatViewTree>) -> &mut FlatViewTree {
        if option.is_none() {
            *option = Some(FlatViewTree::default());
        }
        option.as_mut().unwrap()
    }

    pub fn update(&mut self, update: AppUpdate) {
        match update {
            AppUpdate::ResourceParams { resource_params } => {
                self.resource_params =
                    Some(FlatViewTree::from_view_tree_completed(resource_params));
            }

            AppUpdate::ResourcesStart => {
                self.resources = Some(FlatViewTree::default());
            }
            AppUpdate::ResourcesNode { index, value } => {
                let tree = Self::ensure_tree(&mut self.resources);
                tree.insert_subtree_at_completed(index, value);
            }
            AppUpdate::ResourcesComplete => {}

            AppUpdate::ResourceStatesStart => {
                self.resource_states = Some(FlatViewTree::default());
            }
            AppUpdate::ResourceStatesStartNode { index } => {
                let tree = Self::ensure_tree(&mut self.resource_states);
                tree.set_leaf_started(index);
            }
            AppUpdate::ResourceStatesCompleteNode { index, value } => {
                let tree = Self::ensure_tree(&mut self.resource_states);
                match value {
                    Some(subtree) => {
                        tree.insert_subtree_at_completed(index, subtree);
                    }
                    None => {
                        tree.set_node_none(index);
                    }
                }
            }
            AppUpdate::ResourceStatesComplete => {}

            AppUpdate::ResourceChangesStart => {
                self.resource_changes = Some(FlatViewTree::default());
            }
            AppUpdate::ResourceChangesNode { index, value } => {
                let tree = Self::ensure_tree(&mut self.resource_changes);
                tree.insert_subtree_at_completed(index, value);
            }
            AppUpdate::ResourceChangesComplete => {}

            AppUpdate::OperationsStart => {
                self.operations_tree = Some(FlatViewTree::default());
            }
            AppUpdate::OperationsNode { index, operations } => {
                let tree = Self::ensure_tree(&mut self.operations_tree);
                tree.insert_subtree_at_completed(index, operations);
            }
            AppUpdate::OperationsComplete => {}

            AppUpdate::OperationsApplyStart { operations } => {
                self.operations_epochs = operations
                    .into_iter()
                    .map(|epoch| {
                        epoch
                            .into_iter()
                            .map(|view| OperationView {
                                // Keep the descriptive label available from the start.
                                label: ViewNode::Complete(view),
                                stdout: String::new(),
                                stderr: String::new(),
                                is_complete: false,
                            })
                            .collect::<Vec<OperationView>>()
                    })
                    .collect::<Vec<Vec<OperationView>>>();
            }
            AppUpdate::OperationApplyStart { index } => {
                if let Some(epoch) = self.operations_epochs.get_mut(index.0) {
                    if let Some(operation) = epoch.get_mut(index.1) {
                        operation.stdout.clear();
                        operation.stderr.clear();
                        operation.is_complete = false;
                    }
                }
            }
            AppUpdate::OperationApplyStdout { index, stdout } => {
                if let Some(epoch) = self.operations_epochs.get_mut(index.0) {
                    if let Some(operation) = epoch.get_mut(index.1) {
                        operation.stdout.push_str(&stdout);
                    }
                }
            }
            AppUpdate::OperationApplyStderr { index, stderr } => {
                if let Some(epoch) = self.operations_epochs.get_mut(index.0) {
                    if let Some(operation) = epoch.get_mut(index.1) {
                        operation.stderr.push_str(&stderr);
                    }
                }
            }
            AppUpdate::OperationApplyComplete { index } => {
                if let Some(epoch) = self.operations_epochs.get_mut(index.0) {
                    if let Some(operation) = epoch.get_mut(index.1) {
                        operation.is_complete = true;
                    }
                }
            }
            AppUpdate::OperationsApplyComplete => {}
        }
    }
}
