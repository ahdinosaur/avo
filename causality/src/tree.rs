use lusid_tree::Tree;

pub type CausalityTree<Node, NodeId> = Tree<Node, CausalityMeta<NodeId>>;

#[derive(Debug, Clone, Default)]
pub struct CausalityMeta<NodeId> {
    pub id: Option<NodeId>,
    pub before: Vec<NodeId>,
    pub after: Vec<NodeId>,
}
