#![allow(clippy::collapsible_if)]

use std::fmt::Display;

use lusid_view::{Fragment, Render, View, ViewTree};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ViewNode {
    NotStarted,
    Started,
    Complete(View),
}

impl Render for ViewNode {
    fn render(&self) -> View {
        match self {
            ViewNode::NotStarted => View::Span("🟩".into()),
            ViewNode::Started => View::Span("⌛".into()),
            ViewNode::Complete(view) => {
                View::Fragment(Fragment::new(vec![View::Span("✅".into()), view.clone()]))
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FlatViewTreeNode {
    Branch { view: View, children: Vec<usize> },
    Leaf { view: ViewNode },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FlatViewTree {
    nodes: Vec<Option<FlatViewTreeNode>>,
}

impl FlatViewTree {
    pub fn from_nodes<I>(nodes: I) -> Self
    where
        I: Iterator<Item = Option<FlatViewTreeNode>>,
    {
        FlatViewTree {
            nodes: nodes.collect(),
        }
    }

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
                            view,
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

    pub fn set_leaf_view(&mut self, index: usize, new_view: ViewNode) {
        self.ensure_index_exists(index);
        match self.nodes[index].as_mut() {
            Some(FlatViewTreeNode::Leaf { view }) => {
                *view = new_view;
            }
            Some(FlatViewTreeNode::Branch { .. }) => {
                panic!("expected node to be leaf, not branch")
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AppUpdate {
    ResourceParams {
        resource_params: ViewTree,
    },
    ResourcesStart,
    ResourcesNode {
        index: usize,
        tree: ViewTree,
    },
    ResourcesComplete,
    ResourceStatesStart,
    ResourceStatesNodeStart {
        index: usize,
    },
    ResourceStatesNodeComplete {
        index: usize,
        node: View,
    },
    ResourceStatesComplete,
    ResourceChangesStart,
    ResourceChangesNode {
        index: usize,
        node: Option<View>,
    },
    ResourceChangesComplete {
        has_changes: bool,
    },
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
    pub label: View,
    pub stdout: String,
    pub stderr: String,
    pub is_complete: bool,
}

// TODO: change this so it's an enum for each "phase", adding new data each time.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppView {
    pub resource_params: Option<FlatViewTree>,
    pub resources: Option<FlatViewTree>,
    pub resource_states: Option<FlatViewTree>,
    pub resource_changes: Option<FlatViewTree>,
    pub has_changes: Option<bool>,
    pub operations_tree: Option<FlatViewTree>,
    pub operations_epochs: Option<Vec<Vec<OperationView>>>,
}

impl AppView {
    fn template_tree(template: &Option<FlatViewTree>) -> FlatViewTree {
        let Some(template) = template else {
            panic!("template is None")
        };
        FlatViewTree::from_nodes(template.clone().nodes.into_iter().map(|node| match node {
            None => None,
            Some(FlatViewTreeNode::Leaf { view: _ }) => Some(FlatViewTreeNode::Leaf {
                view: ViewNode::NotStarted,
            }),
            Some(FlatViewTreeNode::Branch { view, children }) => {
                Some(FlatViewTreeNode::Branch { view, children })
            }
        }))
    }

    pub fn update(&mut self, update: AppUpdate) {
        match update {
            AppUpdate::ResourceParams { resource_params } => {
                self.resource_params =
                    Some(FlatViewTree::from_view_tree_completed(resource_params));
            }

            AppUpdate::ResourcesStart => {
                self.resources = Some(Self::template_tree(&self.resource_params));
            }
            AppUpdate::ResourcesNode { index, tree } => {
                self.resources
                    .as_mut()
                    .unwrap()
                    .insert_subtree_at_completed(index, tree);
            }
            AppUpdate::ResourcesComplete => {}

            AppUpdate::ResourceStatesStart => {
                self.resource_states = Some(Self::template_tree(&self.resources));
            }
            AppUpdate::ResourceStatesNodeStart { index } => {
                self.resource_states
                    .as_mut()
                    .unwrap()
                    .set_leaf_started(index);
            }
            AppUpdate::ResourceStatesNodeComplete { index, node } => {
                self.resource_states
                    .as_mut()
                    .unwrap()
                    .set_leaf_view(index, ViewNode::Complete(node));
            }
            AppUpdate::ResourceStatesComplete => {}

            AppUpdate::ResourceChangesStart => {
                self.resource_changes = Some(Self::template_tree(&self.resource_states));
            }
            AppUpdate::ResourceChangesNode { index, node } => match node {
                Some(view) => {
                    self.resource_changes
                        .as_mut()
                        .unwrap()
                        .set_leaf_view(index, ViewNode::Complete(view));
                }
                None => {
                    self.resource_changes.as_mut().unwrap().set_node_none(index);
                }
            },
            AppUpdate::ResourceChangesComplete { has_changes } => {
                self.has_changes = Some(has_changes);
            }

            AppUpdate::OperationsStart => {
                self.operations_tree = Some(Self::template_tree(&self.resource_changes));
            }
            AppUpdate::OperationsNode { index, operations } => {
                self.operations_tree
                    .as_mut()
                    .unwrap()
                    .insert_subtree_at_completed(index, operations);
            }
            AppUpdate::OperationsComplete => {}

            AppUpdate::OperationsApplyStart { operations } => {
                self.operations_epochs = Some(
                    operations
                        .into_iter()
                        .map(|epoch| {
                            epoch
                                .into_iter()
                                .map(|view| OperationView {
                                    label: view,
                                    stdout: String::default(),
                                    stderr: String::default(),
                                    is_complete: false,
                                })
                                .collect::<Vec<OperationView>>()
                        })
                        .collect::<Vec<Vec<OperationView>>>(),
                );
            }
            AppUpdate::OperationApplyStart { index } => {
                if let Some(ref mut epochs) = self.operations_epochs {
                    if let Some(epoch) = epochs.get_mut(index.0) {
                        if let Some(operation) = epoch.get_mut(index.1) {
                            operation.stdout.clear();
                            operation.stderr.clear();
                            operation.is_complete = false;
                        }
                    }
                }
            }
            AppUpdate::OperationApplyStdout { index, stdout } => {
                if let Some(ref mut epochs) = self.operations_epochs {
                    if let Some(epoch) = epochs.get_mut(index.0) {
                        if let Some(operation) = epoch.get_mut(index.1) {
                            operation.stdout.push_str(&stdout);
                        }
                    }
                }
            }
            AppUpdate::OperationApplyStderr { index, stderr } => {
                if let Some(ref mut epochs) = self.operations_epochs {
                    if let Some(epoch) = epochs.get_mut(index.0) {
                        if let Some(operation) = epoch.get_mut(index.1) {
                            operation.stderr.push_str(&stderr);
                        }
                    }
                }
            }
            AppUpdate::OperationApplyComplete { index } => {
                if let Some(ref mut epochs) = self.operations_epochs {
                    if let Some(epoch) = epochs.get_mut(index.0) {
                        if let Some(operation) = epoch.get_mut(index.1) {
                            operation.is_complete = true;
                        }
                    }
                }
            }
            AppUpdate::OperationsApplyComplete => {}
        }
    }
}

impl From<FlatViewTree> for ViewTree {
    fn from(value: FlatViewTree) -> Self {
        fn build(tree: &FlatViewTree, index: usize) -> Option<ViewTree> {
            let node = tree.get_node(index)?;
            Some(match node {
                FlatViewTreeNode::Branch { view, children } => ViewTree::Branch {
                    view: view.render(),
                    children: children
                        .iter()
                        .filter_map(|child| build(tree, *child))
                        .collect(),
                },
                FlatViewTreeNode::Leaf { view } => ViewTree::Leaf {
                    view: view.render(),
                },
            })
        }
        build(&value, 0).expect("expected root node to exist")
    }
}

impl Display for FlatViewTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        ViewTree::from(self.clone()).fmt(f)
    }
}
