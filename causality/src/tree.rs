use lusid_tree::Tree;

pub use lusid_tree::NodeId;

pub type CausalityTree<Node> = Tree<Node, CausalityMeta>;

#[derive(Debug, Clone, Default)]
pub struct CausalityMeta {
    pub before: Vec<NodeId>,
    pub after: Vec<NodeId>,
}
