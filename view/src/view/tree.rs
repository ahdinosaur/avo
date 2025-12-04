use serde::{Deserialize, Serialize};
use std::fmt::Display;
use termtree::Tree as DisplayTree;

use crate::ViewNode;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Tree {
    Branch { label: String, nodes: Vec<Tree> },
    Leaf { label: String },
}

impl Display for Tree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        DisplayTree::<String>::from(self.clone()).fmt(f)
    }
}

#[derive(Debug, Clone)]
pub struct TreeBuilder<Label, Nodes> {
    label: Label,
    nodes: Nodes,
}

impl Default for TreeBuilder<(), ()> {
    fn default() -> Self {
        Self {
            label: (),
            nodes: (),
        }
    }
}

impl<Nodes> TreeBuilder<(), Nodes> {
    pub fn label(self, label: String) -> TreeBuilder<String, Nodes> {
        TreeBuilder {
            label,
            nodes: self.nodes,
        }
    }
}

impl<Label> TreeBuilder<Label, ()> {
    pub fn nodes(self, nodes: Vec<Tree>) -> TreeBuilder<Label, Vec<Tree>> {
        TreeBuilder {
            label: self.label,
            nodes,
        }
    }
}

impl TreeBuilder<String, ()> {
    pub fn build(self) -> Tree {
        Tree::Leaf { label: self.label }
    }
}

impl TreeBuilder<String, Vec<Tree>> {
    pub fn build(self) -> Tree {
        Tree::Branch {
            label: self.label,
            nodes: self.nodes,
        }
    }
}

impl From<Tree> for ViewNode {
    fn from(value: Tree) -> Self {
        ViewNode::Tree(value)
    }
}

impl From<Tree> for DisplayTree<String> {
    fn from(value: Tree) -> Self {
        match value {
            Tree::Branch { label, nodes } => DisplayTree::new(label).with_leaves(nodes),
            Tree::Leaf { label } => DisplayTree::new(label),
        }
    }
}
