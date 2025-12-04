use std::collections::VecDeque;

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
}

#[derive(Debug, Clone)]
pub enum FlatTreeNode<Node, Meta> {
    Branch { meta: Meta, children: Vec<usize> },
    Leaf { meta: Meta, node: Node },
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
        append_tree_to_nodes(&mut self.nodes, tree)
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
        // Collect to determine the original number of indices so we can
        // place mapped items at their original indices and append any
        // additional nodes after that, keeping indices of original nodes
        // stable.
        let items: Vec<Option<FlatTreeMapItem<Node, Meta>>> = iter.collect();
        let original_len = items.len();

        // Pre-size with None so that original indices stay fixed.
        let mut nodes: Vec<Option<FlatTreeNode<Node, Meta>>> = vec![None; original_len];

        for (index, maybe_item) in items.into_iter().enumerate() {
            match maybe_item {
                None => {
                    // Leave as None
                }
                Some(FlatTreeMapItem::Node(node)) => {
                    nodes[index] = Some(node);
                }
                Some(FlatTreeMapItem::SubTree(tree)) => {
                    // Flatten the subtree and splice it into `nodes`, placing
                    // the root at `index`, and appending remaining nodes to
                    // the end. Remap child indices accordingly.
                    let sub_flat: FlatTree<Node, Meta> = tree.into();
                    let sub_root = sub_flat.root_index();

                    // Extract concrete nodes; `From<Tree>` guarantees `Some`.
                    let mut sub_nodes: VecDeque<FlatTreeNode<Node, Meta>> = sub_flat
                        .into_iter()
                        .map(|o| {
                            o.expect(
                                "FlatTree::from(Tree) should not \
                                     produce None nodes",
                            )
                        })
                        .collect();

                    let sub_len = sub_nodes.len();
                    let mut remap: Vec<usize> = vec![usize::MAX; sub_len];

                    // Root of the subtree is placed at the current index.
                    remap[sub_root] = index;

                    // Assign new indices for the remaining nodes by appending.
                    for (index, mapped_index) in remap.iter_mut().enumerate().take(sub_len) {
                        if index == sub_root {
                            continue;
                        }
                        let new_index = nodes.len();
                        nodes.push(None); // reserve slot
                        *mapped_index = new_index;
                    }

                    // Now move nodes over with adjusted child indices.
                    for mapped_index in remap.iter().take(sub_len) {
                        let mapped_node = match sub_nodes
                            .pop_front()
                            .expect("sub_nodes should have len > 0")
                        {
                            FlatTreeNode::Branch { meta, mut children } => {
                                let new_children: Vec<usize> =
                                    children.drain(..).map(|old| remap[old]).collect();
                                FlatTreeNode::Branch {
                                    meta,
                                    children: new_children,
                                }
                            }
                            FlatTreeNode::Leaf { meta, node } => FlatTreeNode::Leaf { meta, node },
                        };
                        nodes[*mapped_index] = Some(mapped_node);
                    }
                }
            }
        }

        FlatTree { nodes, root_index }
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
