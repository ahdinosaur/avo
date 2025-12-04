use lusid_tree::Tree;

pub type CausalityTree<Node, NodeId = String> = Tree<Node, CausalityMeta<NodeId>>;

#[derive(Debug, Clone)]
pub struct CausalityMeta<NodeId> {
    pub id: Option<NodeId>,
    pub before: Vec<NodeId>,
    pub after: Vec<NodeId>,
}

impl<NodeId> Default for CausalityMeta<NodeId> {
    fn default() -> Self {
        Self {
            id: None,
            before: Vec::new(),
            after: Vec::new(),
        }
    }
}
