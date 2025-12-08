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

    pub fn leaf(meta: Meta, node: Node) -> Self {
        Self::Leaf { node, meta }
    }

    pub fn is_leaf(&self) -> bool {
        matches!(self, Tree::Leaf { .. })
    }

    pub fn is_branch(&self) -> bool {
        matches!(self, Tree::Branch { .. })
    }

    pub fn map<NextNode, MapFn>(self, map: MapFn) -> Tree<NextNode, Meta>
    where
        MapFn: Fn(Node) -> NextNode + Copy,
    {
        match self {
            Tree::Branch { meta, children } => Tree::Branch {
                meta,
                children: children
                    .into_iter()
                    .map(|tree| Self::map(tree, map))
                    .collect(),
            },
            Tree::Leaf { meta, node } => Tree::Leaf {
                meta,
                node: map(node),
            },
        }
    }

    pub fn map_meta<NextMeta, MapFn>(self, map: MapFn) -> Tree<Node, NextMeta>
    where
        MapFn: Fn(Meta) -> NextMeta + Copy,
    {
        match self {
            Tree::Branch { meta, children } => Tree::Branch {
                meta: map(meta),
                children: children
                    .into_iter()
                    .map(|tree| Self::map_meta(tree, map))
                    .collect(),
            },
            Tree::Leaf { meta, node } => Tree::Leaf {
                meta: map(meta),
                node,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub enum FlatTreeNode<Node, Meta> {
    Branch { meta: Meta, children: Vec<usize> },
    Leaf { meta: Meta, node: Node },
}

#[derive(Debug, Clone)]
pub enum FlatTreeMappedItem<Node, Meta> {
    Node(Node),
    SubTrees(Vec<Tree<Node, Meta>>),
}

impl<Node, Meta> FlatTreeNode<Node, Meta> {
    pub fn map<F, NextNode>(self, map: F) -> FlatTreeMapItem<NextNode, Meta>
    where
        F: Fn(Node) -> FlatTreeMappedItem<NextNode, Meta>,
    {
        match self {
            FlatTreeNode::Branch { meta, children } => {
                FlatTreeMapItem::Node(FlatTreeNode::Branch { meta, children })
            }
            FlatTreeNode::Leaf { meta, node } => match map(node) {
                FlatTreeMappedItem::Node(node) => {
                    FlatTreeMapItem::Node(FlatTreeNode::Leaf { meta, node })
                }
                FlatTreeMappedItem::SubTrees(trees) => FlatTreeMapItem::SubTree(Tree::Branch {
                    meta,
                    children: trees,
                }),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct FlatTree<Node, Meta> {
    nodes: Vec<Option<FlatTreeNode<Node, Meta>>>,
    root_index: usize,
}

#[derive(Debug, Error)]
pub enum FlatTreeError {
    #[error("node at index {0} is None")]
    NodeMissing(usize),

    #[error("index {0} is out of bounds")]
    IndexOutOfBounds(usize),
}

impl<Node, Meta> FlatTree<Node, Meta> {
    pub fn root_index(&self) -> usize {
        self.root_index
    }

    pub fn root(&self) -> Option<&FlatTreeNode<Node, Meta>> {
        self.nodes.get(self.root_index)?.as_ref()
    }

    pub fn get(&self, index: usize) -> Result<&FlatTreeNode<Node, Meta>, FlatTreeError> {
        let node = self
            .nodes
            .get(index)
            .ok_or(FlatTreeError::IndexOutOfBounds(index))?;
        node.as_ref().ok_or(FlatTreeError::NodeMissing(index))
    }

    pub fn get_mut(
        &mut self,
        index: usize,
    ) -> Result<&mut FlatTreeNode<Node, Meta>, FlatTreeError> {
        let node = self
            .nodes
            .get_mut(index)
            .ok_or(FlatTreeError::IndexOutOfBounds(index))?;
        node.as_mut().ok_or(FlatTreeError::NodeMissing(index))
    }

    pub fn append_tree(&mut self, tree: Tree<Node, Meta>) -> usize {
        append_tree_nodes(&mut self.nodes, tree)
    }

    pub fn replace_tree(&mut self, tree: Option<Tree<Node, Meta>>, root_index: usize) {
        replace_tree_nodes(&mut self.nodes, tree, root_index)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Option<FlatTreeNode<Node, Meta>>> {
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

impl<Node, Meta> IntoIterator for FlatTree<Node, Meta> {
    type Item = <Vec<Option<FlatTreeNode<Node, Meta>>> as IntoIterator>::Item;
    type IntoIter = <Vec<Option<FlatTreeNode<Node, Meta>>> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.nodes.into_iter()
    }
}

#[derive(Debug, Clone)]
pub enum FlatTreeMapItem<Node, Meta> {
    Node(FlatTreeNode<Node, Meta>),
    SubTree(Tree<Node, Meta>),
}

impl<Node, Meta> FlatTree<Node, Meta>
where
    Node: Clone,
    Meta: Clone,
{
    pub fn from_map_iter<I>(iter: I, root_index: usize) -> Self
    where
        I: Iterator<Item = Option<FlatTreeMapItem<Node, Meta>>>,
    {
        let mut nodes: Vec<Option<FlatTreeNode<Node, Meta>>> = Vec::new();
        for (index, item) in iter.enumerate() {
            match item {
                None => nodes.push(None),
                Some(FlatTreeMapItem::Node(node)) => nodes.push(Some(node)),
                Some(FlatTreeMapItem::SubTree(tree)) => {
                    replace_tree_nodes(&mut nodes, Some(tree), index);
                }
            }
        }
        FlatTree { nodes, root_index }
    }
}

fn append_tree_nodes<Node, Meta>(
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
                let child_index = append_tree_nodes(nodes, child);
                child_indices.push(child_index);
            }
            if let Some(FlatTreeNode::Branch { children, .. }) = nodes[index].as_mut() {
                *children = child_indices;
            }
            index
        }
    }
}

fn replace_tree_nodes<Node, Meta>(
    nodes: &mut Vec<Option<FlatTreeNode<Node, Meta>>>,
    tree: Option<Tree<Node, Meta>>,
    root_index: usize,
) {
    // NOTE(mw): This removes all children. In the future, maybe we'd want to keep children
    //   that exist in the new tree, however that's not what we need now. Also, how would we
    //   check equality?
    if let Some(Some(FlatTreeNode::Branch { meta: _, children })) = nodes.get(root_index) {
        for child in children.clone() {
            replace_tree_nodes(nodes, None, child);
        }
    }

    match tree {
        None => {
            nodes[root_index] = None;
        }
        Some(Tree::Leaf { node, meta }) => {
            nodes[root_index] = Some(FlatTreeNode::Leaf { node, meta });
        }
        Some(Tree::Branch { children, meta }) => {
            let mut child_indices = Vec::with_capacity(children.len());
            for child in children {
                let child_index = append_tree_nodes(nodes, child);
                child_indices.push(child_index);
            }
            nodes[root_index] = Some(FlatTreeNode::Branch {
                children: child_indices,
                meta,
            });
        }
    }
}

impl<Node, Meta> From<Tree<Node, Meta>> for FlatTree<Node, Meta> {
    fn from(tree: Tree<Node, Meta>) -> Self {
        let mut nodes = Vec::new();
        let root_index = append_tree_nodes(&mut nodes, tree);
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
            let node = nodes[index].take()?;

            match node {
                FlatTreeNode::Leaf { node, meta } => Some(Tree::Leaf { node, meta }),
                FlatTreeNode::Branch { children, meta } => {
                    let mut built_children = Vec::new();
                    for child_index in children {
                        if let Some(child) = build(child_index, nodes) {
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
