mod epoch;
mod id;
mod tree;

use std::fmt::Display;

pub use crate::epoch::*;
pub use crate::id::*;
pub use crate::tree::*;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NodeId {
    Plan,
}

impl Display for NodeId {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

pub type Flow<Node> = Tree<Node, FlowMeta>;

#[derive(Debug, Clone, Default)]
pub struct FlowMeta {
    pub id: Option<NodeId>,
    pub before: Vec<NodeId>,
    pub after: Vec<NodeId>,
}
