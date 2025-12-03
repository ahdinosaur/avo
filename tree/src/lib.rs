use thiserror::Error;

#[derive(Debug, Clone)]
pub enum Tree<Node, Meta> {
    Branch {
        meta: Meta,
        children: Vec<Tree<Node, Meta>>,
    },
    Leaf {
        meta: Meta,
        node: Node,
    },
}

impl<Node, Meta> Tree<Node, Meta> {
    pub fn branch(meta: Meta, children: Vec<Tree<Node, Meta>>) -> Self {
        Self::Branch { children, meta }
    }

    pub fn leaf(node: Node, meta: Meta) -> Self {
        Self::Leaf { node, meta }
    }

    pub fn is_leaf(&self) -> bool {
        matches!(self, Tree::Leaf { .. })
    }

    pub fn is_branch(&self) -> bool {
        matches!(self, Tree::Branch { .. })
    }
}

#[derive(Debug, Clone)]
pub enum FlatTreeNode<Node, Meta> {
    Branch { meta: Meta, children: Vec<usize> },
    Leaf { meta: Meta, node: Node },
}

#[derive(Debug, Clone)]
pub struct FlatTree<Node, Meta> {
    pub nodes: Vec<Option<FlatTreeNode<Node, Meta>>>,
    pub root_index: usize,
}

#[derive(Debug, Error)]
pub enum FlatTreeError {
    #[error("root index {0} is out of bounds")]
    InvalidRootIndex(usize),

    #[error(
        "child index {child_index} referenced by parent {parent_index} is out \
         of bounds"
    )]
    ChildIndexOutOfBounds {
        parent_index: usize,
        child_index: usize,
    },

    #[error("node at index {0} is None")]
    NodeMissing(usize),

    #[error("cycle detected at index {0}")]
    CycleDetected(usize),

    #[error("index {0} is out of bounds")]
    IndexOutOfBounds(usize),

    #[error("expected a branch at index {0}, found a leaf")]
    ExpectedBranch(usize),

    #[error("expected a leaf at index {0}, found a branch")]
    ExpectedLeaf(usize),
}

impl<Node, Meta> FlatTree<Node, Meta> {
    pub fn root(&self) -> Option<&FlatTreeNode<Node, Meta>> {
        self.nodes.get(self.root_index)?.as_ref()
    }

    pub fn get(&self, index: usize) -> Result<&FlatTreeNode<Node, Meta>, FlatTreeError> {
        let node = self
            .nodes
            .get(index)
            .ok_or(FlatTreeError::IndexOutOfBounds(index))?;
        node.as_ref()
            .ok_or_else(|| FlatTreeError::NodeMissing(index))
    }

    pub fn get_mut(
        &mut self,
        index: usize,
    ) -> Result<&mut FlatTreeNode<Node, Meta>, FlatTreeError> {
        let node = self
            .nodes
            .get_mut(index)
            .ok_or(FlatTreeError::IndexOutOfBounds(index))?;
        node.as_mut()
            .ok_or_else(|| FlatTreeError::NodeMissing(index))
    }

    pub fn append_tree(&mut self, tree: Tree<Node, Meta>) -> usize {
        append_tree_to_nodes(&mut self.nodes, tree)
    }

    pub fn into_iter<I>(&self) -> impl Iterator<Item = &Option<FlatTreeNode<Node, Meta>>> {
        self.nodes.iter()
    }

    pub fn from_iter<I>(iter: I, root_index: usize) -> Self
    where
        I: Iterator<Item = Option<FlatTreeNode<Node, Meta>>>,
    {
        Self {
            nodes: iter.collect(),
            root_index,
        }
    }
}

#[derive(Debug, Clone)]
pub enum FlatTreeMapItem<Node, Meta> {
    Node(FlatTreeNode<Node, Meta>),
    SubTree(Tree<Node, Meta>),
}

impl<Node, Meta> FlatTree<Node, Meta> {
    pub fn from_map_iter<I>(iter: I, root_index: usize) -> Self
    where
        I: Iterator<Item = Option<FlatTreeMapItem<Node, Meta>>>,
    {
    }
}

fn append_tree_to_nodes<Node, Meta>(
    nodes: &mut Vec<Option<FlatTreeNode<Node, Meta>>>,
    tree: Tree<Node, Meta>,
) -> usize {
    match tree {
        Tree::Leaf { node, meta } => {
            let index = nodes.len();
            nodes.push(Some(FlatTreeNode::Leaf { node, meta }));
            index
        }
        Tree::Branch { mut children, meta } => {
            let index = nodes.len();
            nodes.push(Some(FlatTreeNode::Branch {
                children: Vec::new(),
                meta,
            }));
            let mut child_indices = Vec::with_capacity(children.len());
            for child in children.drain(..) {
                let child_index = append_tree_to_nodes(nodes, child);
                child_indices.push(child_index);
            }
            if let Some(FlatTreeNode::Branch { children, .. }) = nodes[index].as_mut() {
                *children = child_indices;
            }
            index
        }
    }
}

impl<Node, Meta> From<Tree<Node, Meta>> for FlatTree<Node, Meta> {
    fn from(tree: Tree<Node, Meta>) -> Self {
        let mut nodes = Vec::new();
        let root_index = append_tree_to_nodes(&mut nodes, tree);
        FlatTree { nodes, root_index }
    }
}

/// From<FlatTree> -> Tree: reconstruct a nested tree. This is lenient:
/// - Missing or invalid children are skipped.
/// - If the root is missing, returns an empty Branch with default meta.
impl<Node, Meta> From<FlatTree<Node, Meta>> for Tree<Node, Meta>
where
    Meta: Default,
{
    fn from(mut flat: FlatTree<Node, Meta>) -> Self {
        fn build<Node, Meta>(
            index: usize,
            nodes: &mut [Option<FlatTreeNode<Node, Meta>>],
        ) -> Option<Tree<Node, Meta>>
        where
            Meta: Default,
        {
            if index >= nodes.len() {
                return None;
            }
            let Some(node) = nodes[index].take() else {
                return None;
            };

            match node {
                FlatTreeNode::Leaf { node, meta } => Some(Tree::Leaf { node, meta }),
                FlatTreeNode::Branch { children, meta } => {
                    let mut built_children = Vec::new();
                    for child_idx in children {
                        if let Some(child) = build(child_idx, nodes) {
                            built_children.push(child);
                        }
                    }
                    Some(Tree::Branch {
                        children: built_children,
                        meta,
                    })
                }
            }
        }

        build(flat.root_index, &mut flat.nodes).unwrap_or_else(|| Tree::Branch {
            children: Vec::new(),
            meta: Meta::default(),
        })
    }
}
