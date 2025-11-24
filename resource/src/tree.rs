#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeId(pub String);

impl NodeId {
    pub fn new(id: String) -> Self {
        Self(id)
    }
}

#[derive(Debug, Clone)]
pub enum Tree<Node> {
    Branch {
        id: Option<NodeId>,
        before: Vec<NodeId>,
        after: Vec<NodeId>,
        children: Vec<Tree<Node>>,
    },
    Leaf {
        id: Option<NodeId>,
        node: Node,
        before: Vec<NodeId>,
        after: Vec<NodeId>,
    },
}
