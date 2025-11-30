pub struct CausalityMeta {
    before: Vec<NodeId>
    after: Vec<NodeId>
}

type CausalityTree<Node> = Tree<Node, CausalityMeta>
