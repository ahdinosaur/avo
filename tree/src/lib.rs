use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;

use thiserror::Error;

/// A simple tree structure used for building and describing sub-trees.
/// - Leaves carry `node` data and `meta`.
/// - Branches carry `meta` and have `children` which are trees.
#[derive(Debug, Clone)]
pub enum Tree<Node, Meta> {
    Branch {
        children: Vec<Tree<Node, Meta>>,
        meta: Meta,
    },
    Leaf {
        node: Node,
        meta: Meta,
    },
}

impl<Node, Meta> Tree<Node, Meta> {
    pub fn branch_with_meta(meta: Meta, children: Vec<Tree<Node, Meta>>) -> Self {
        Self::Branch { children, meta }
    }

    pub fn leaf_with_meta(node: Node, meta: Meta) -> Self {
        Self::Leaf { node, meta }
    }

    pub fn meta(&self) -> &Meta {
        match self {
            Tree::Branch { meta, .. } => meta,
            Tree::Leaf { meta, .. } => meta,
        }
    }

    pub fn meta_mut(&mut self) -> &mut Meta {
        match self {
            Tree::Branch { meta, .. } => meta,
            Tree::Leaf { meta, .. } => meta,
        }
    }

    pub fn is_leaf(&self) -> bool {
        matches!(self, Tree::Leaf { .. })
    }

    pub fn is_branch(&self) -> bool {
        matches!(self, Tree::Branch { .. })
    }

    pub fn try_push_child(&mut self, child: Tree<Node, Meta>) -> Result<(), TreeBuildError> {
        match self {
            Tree::Branch { children, .. } => {
                children.push(child);
                Ok(())
            }
            Tree::Leaf { .. } => Err(TreeBuildError::CannotPushChildIntoLeaf),
        }
    }
}

/// Friendly default constructors when `Meta: Default`.
impl<Node, Meta> Tree<Node, Meta>
where
    Meta: Default,
{
    pub fn branch(children: Vec<Tree<Node, Meta>>) -> Self {
        Self::Branch {
            children,
            meta: Meta::default(),
        }
    }

    pub fn leaf(node: Node) -> Self {
        Self::Leaf {
            node,
            meta: Meta::default(),
        }
    }
}

#[derive(Debug, Error)]
pub enum TreeBuildError {
    #[error("cannot push a child into a leaf")]
    CannotPushChildIntoLeaf,
}

/// A flat-node for `FlatTree`.
/// - Leaves carry `node` data and `meta`.
/// - Branches carry `meta` and `children` which are indices into the `FlatTree.nodes` vec.
#[derive(Debug, Clone)]
pub enum FlatTreeNode<Node, Meta> {
    Branch { children: Vec<usize>, meta: Meta },
    Leaf { node: Node, meta: Meta },
}

/// A flat tree backed by a single contiguous vector.
/// - `nodes` may contain `None` entries (e.g., after `map_option`).
/// - `root_index` points to the root node of the tree.
/// - Branch `children` are indices into `nodes`.
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
    /// Construct an empty flat tree (no nodes). You will need to set a root later.
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            root_index: 0,
        }
    }

    /// Construct a flat tree with a single leaf as the root.
    pub fn single_leaf(node: Node, meta: Meta) -> Self {
        Self {
            nodes: vec![Some(FlatTreeNode::Leaf { node, meta })],
            root_index: 0,
        }
    }

    /// Construct a flat tree with a single branch as the root.
    pub fn single_branch(meta: Meta) -> Self {
        Self {
            nodes: vec![Some(FlatTreeNode::Branch {
                children: Vec::new(),
                meta,
            })],
            root_index: 0,
        }
    }

    /// Returns the root node if present.
    pub fn root(&self) -> Option<&FlatTreeNode<Node, Meta>> {
        self.nodes.get(self.root_index)?.as_ref()
    }

    /// Returns the mutable root node if present.
    pub fn root_mut(&mut self) -> Option<&mut FlatTreeNode<Node, Meta>> {
        self.nodes.get_mut(self.root_index)?.as_mut()
    }

    pub fn set_root_index(&mut self, root_index: usize) -> Result<(), FlatTreeError> {
        if root_index >= self.nodes.len() {
            return Err(FlatTreeError::InvalidRootIndex(root_index));
        }
        if self.nodes[root_index].is_none() {
            return Err(FlatTreeError::NodeMissing(root_index));
        }
        self.root_index = root_index;
        Ok(())
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

    /// Append a leaf node and return its index.
    pub fn append_leaf(&mut self, node: Node, meta: Meta) -> usize {
        let idx = self.nodes.len();
        self.nodes.push(Some(FlatTreeNode::Leaf { node, meta }));
        idx
    }

    /// Append a branch node (with currently no children) and return its index.
    pub fn append_branch(&mut self, meta: Meta) -> usize {
        let idx = self.nodes.len();
        self.nodes.push(Some(FlatTreeNode::Branch {
            children: vec![],
            meta,
        }));
        idx
    }

    /// Add an existing node as a child of a branch.
    pub fn add_child(
        &mut self,
        parent_index: usize,
        child_index: usize,
    ) -> Result<(), FlatTreeError> {
        // Validate indices
        self.get(child_index).map(|_| ())?;
        // Mutably get the parent branch
        let parent = self.get_mut(parent_index)?;
        match parent {
            FlatTreeNode::Branch { children, .. } => {
                children.push(child_index);
                Ok(())
            }
            FlatTreeNode::Leaf { .. } => Err(FlatTreeError::ExpectedBranch(parent_index)),
        }
    }

    /// Validate the structure:
    /// - root_index must be in-bounds and not None
    /// - every child index of a reachable branch must be in-bounds
    /// - cycles are reported
    /// - None children are permitted (useful after map_option)
    pub fn validate(&self) -> Result<(), FlatTreeError> {
        if self.root_index >= self.nodes.len() {
            return Err(FlatTreeError::InvalidRootIndex(self.root_index));
        }
        if self.nodes[self.root_index].is_none() {
            return Err(FlatTreeError::NodeMissing(self.root_index));
        }

        // DFS from root to check bounds and cycles
        let mut visiting = vec![false; self.nodes.len()];
        let mut visited = vec![false; self.nodes.len()];

        fn dfs<Node, Meta>(
            index: usize,
            nodes: &[Option<FlatTreeNode<Node, Meta>>],
            visiting: &mut [bool],
            visited: &mut [bool],
        ) -> Result<(), FlatTreeError> {
            if visited[index] {
                return Ok(());
            }
            if visiting[index] {
                return Err(FlatTreeError::CycleDetected(index));
            }
            visiting[index] = true;

            let Some(node) = nodes[index].as_ref() else {
                // Root or a reachable node must not be None.
                return Err(FlatTreeError::NodeMissing(index));
            };

            if let FlatTreeNode::Branch { children, .. } = node {
                for &child in children.iter() {
                    if child >= nodes.len() {
                        return Err(FlatTreeError::ChildIndexOutOfBounds {
                            parent_index: index,
                            child_index: child,
                        });
                    }
                    // Allow None children (dangling) for leniency? The validator here
                    // is "strict": reachable None is treated as error. If you want
                    // lenient behavior, you can skip this error and return Ok(()).
                    if nodes[child].is_none() {
                        return Err(FlatTreeError::NodeMissing(child));
                    }
                    dfs(child, nodes, visiting, visited)?;
                }
            }

            visiting[index] = false;
            visited[index] = true;
            Ok(())
        }

        dfs(self.root_index, &self.nodes, &mut visiting, &mut visited)
    }

    /// Efficient leaf-only map. Indices do not change.
    pub fn map<NextNode, MapFn>(self, map: MapFn) -> FlatTree<NextNode, Meta>
    where
        MapFn: Fn(Node) -> NextNode + Copy,
    {
        let root_index = self.root_index;
        let nodes = self
            .nodes
            .into_iter()
            .map(|maybe| {
                maybe.map(|node| match node {
                    FlatTreeNode::Leaf { node, meta } => FlatTreeNode::Leaf {
                        node: map(node),
                        meta,
                    },
                    FlatTreeNode::Branch { children, meta } => {
                        FlatTreeNode::Branch { children, meta }
                    }
                })
            })
            .collect();

        FlatTree { nodes, root_index }
    }

    /// Map leaves with an Option. Do not compact; leave nodes as None.
    /// Indices do not change.
    pub fn map_option<NextNode, MapFn>(self, map: MapFn) -> FlatTree<NextNode, Meta>
    where
        MapFn: Fn(Node) -> Option<NextNode> + Copy,
    {
        let root_index = self.root_index;
        let nodes = self
            .nodes
            .into_iter()
            .map(|maybe| {
                maybe.and_then(|node| match node {
                    FlatTreeNode::Leaf { node, meta } => {
                        map(node).map(|node| FlatTreeNode::Leaf { node, meta })
                    }
                    FlatTreeNode::Branch { children, meta } => {
                        Some(FlatTreeNode::Branch { children, meta })
                    }
                })
            })
            .collect();

        FlatTree { nodes, root_index }
    }

    /// Map leaves with a Result. Indices do not change.
    pub fn map_result<NextNode, Error, MapFn>(
        self,
        map: MapFn,
    ) -> Result<FlatTree<NextNode, Meta>, Error>
    where
        MapFn: Fn(Node) -> Result<NextNode, Error> + Copy,
    {
        let root_index = self.root_index;
        let mut nodes_out = Vec::with_capacity(self.nodes.len());

        for maybe in self.nodes {
            let mapped = match maybe {
                None => None,
                Some(FlatTreeNode::Leaf { node, meta }) => Some(FlatTreeNode::Leaf {
                    node: map(node)?,
                    meta,
                }),
                Some(FlatTreeNode::Branch { children, meta }) => {
                    Some(FlatTreeNode::Branch { children, meta })
                }
            };
            nodes_out.push(mapped);
        }

        Ok(FlatTree {
            nodes: nodes_out,
            root_index,
        })
    }

    /// Async map on leaves with a Result. Indices do not change.
    pub fn map_result_async<NextNode, Error, MapFn, Fut>(
        self,
        map: MapFn,
    ) -> Pin<Box<dyn Future<Output = Result<FlatTree<NextNode, Meta>, Error>> + Send + 'static>>
    where
        Node: Send + 'static,
        NextNode: Send + 'static,
        Error: Send + 'static,
        MapFn: Fn(Node) -> Fut + Copy + Send + 'static,
        Fut: Future<Output = Result<NextNode, Error>> + Send + 'static,
    {
        Box::pin(async move {
            let root_index = self.root_index;
            let mut nodes_out = Vec::with_capacity(self.nodes.len());

            // Prepare futures for leaves
            let mut jobs: Vec<(
                usize,
                Meta,
                Pin<Box<dyn Future<Output = Result<NextNode, Error>> + Send>>,
            )> = Vec::new();

            // First pass: keep branches, placeholder for leaves
            for (i, maybe) in self.nodes.into_iter().enumerate() {
                match maybe {
                    None => nodes_out.push(None),
                    Some(FlatTreeNode::Branch { children, meta }) => {
                        nodes_out.push(Some(FlatTreeNode::Branch { children, meta }));
                    }
                    Some(FlatTreeNode::Leaf { node, meta }) => {
                        nodes_out.push(None); // will fill later
                        let fut = map(node);
                        jobs.push((i, meta, Box::pin(fut)));
                    }
                }
            }

            // Resolve leaves
            for (i, meta, fut) in jobs {
                let mapped_node = fut.await?;
                nodes_out[i] = Some(FlatTreeNode::Leaf {
                    node: mapped_node,
                    meta,
                });
            }

            Ok(FlatTree {
                nodes: nodes_out,
                root_index,
            })
        })
    }

    /// Map leaves into sub-trees. Existing indices do not change; new nodes
    /// are appended. Each leaf becomes a branch whose children are the roots
    /// of the appended sub-trees.
    pub fn map_tree<NextNode, MapFn>(self, map: MapFn) -> FlatTree<NextNode, Meta>
    where
        MapFn: Fn(Node) -> Vec<Tree<NextNode, Meta>> + Copy,
    {
        let orig_len = self.nodes.len();
        let mut nodes_out: Vec<Option<FlatTreeNode<NextNode, Meta>>> = Vec::with_capacity(orig_len);
        nodes_out.resize_with(orig_len, || None);

        // Helper to append a Tree<NextNode, Meta> into nodes_out and
        // return its index.
        fn append_tree<Node, Meta>(
            tree: Tree<Node, Meta>,
            nodes: &mut Vec<Option<FlatTreeNode<Node, Meta>>>,
        ) -> usize {
            match tree {
                Tree::Leaf { node, meta } => {
                    let idx = nodes.len();
                    nodes.push(Some(FlatTreeNode::Leaf { node, meta }));
                    idx
                }
                Tree::Branch { mut children, meta } => {
                    let idx = nodes.len();
                    nodes.push(Some(FlatTreeNode::Branch {
                        children: Vec::new(),
                        meta,
                    }));
                    // Collect all children indices recursively
                    let mut child_indices = Vec::with_capacity(children.len());
                    for child in children.drain(..) {
                        let cidx = append_tree(child, nodes);
                        child_indices.push(cidx);
                    }
                    if let Some(FlatTreeNode::Branch { children, .. }) = nodes[idx].as_mut() {
                        *children = child_indices;
                    }
                    idx
                }
            }
        }

        // First pass: rewrite old nodes; leaves become branches with
        // children appended at the end.
        for i in 0..orig_len {
            match self.nodes[i].clone() {
                None => {
                    nodes_out[i] = None;
                }
                Some(FlatTreeNode::Branch { children, meta }) => {
                    nodes_out[i] = Some(FlatTreeNode::Branch { children, meta });
                }
                Some(FlatTreeNode::Leaf { .. }) => {
                    // Move out of self.nodes by taking ownership via match below
                    // (we borrowed above only to decide the variant).
                    let maybe_owned = self.nodes[i].clone();
                    if let Some(FlatTreeNode::Leaf { node, meta }) = maybe_owned {
                        // Map this leaf to new sub-trees and append them.
                        let trees = map(node);
                        let mut child_indices = Vec::with_capacity(trees.len());
                        for t in trees {
                            let idx = append_tree(t, &mut nodes_out);
                            child_indices.push(idx);
                        }
                        nodes_out[i] = Some(FlatTreeNode::Branch {
                            children: child_indices,
                            meta,
                        });
                    } else {
                        nodes_out[i] = None;
                    }
                }
            }
        }

        FlatTree {
            nodes: nodes_out,
            root_index: self.root_index,
        }
    }

    /// Map leaves into sub-trees with Result.
    pub fn map_tree_result<NextNode, Error, MapFn>(
        self,
        map: MapFn,
    ) -> Result<FlatTree<NextNode, Meta>, Error>
    where
        MapFn: Fn(Node) -> Result<Vec<Tree<NextNode, Meta>>, Error> + Copy,
    {
        let orig_len = self.nodes.len();
        let mut nodes_out: Vec<Option<FlatTreeNode<NextNode, Meta>>> = Vec::with_capacity(orig_len);
        nodes_out.resize_with(orig_len, || None);

        fn append_tree<Node, Meta>(
            tree: Tree<Node, Meta>,
            nodes: &mut Vec<Option<FlatTreeNode<Node, Meta>>>,
        ) -> usize {
            match tree {
                Tree::Leaf { node, meta } => {
                    let idx = nodes.len();
                    nodes.push(Some(FlatTreeNode::Leaf { node, meta }));
                    idx
                }
                Tree::Branch { mut children, meta } => {
                    let idx = nodes.len();
                    nodes.push(Some(FlatTreeNode::Branch {
                        children: Vec::new(),
                        meta,
                    }));
                    let mut child_indices = Vec::with_capacity(children.len());
                    for child in children.drain(..) {
                        let cidx = append_tree(child, nodes);
                        child_indices.push(cidx);
                    }
                    if let Some(FlatTreeNode::Branch { children, .. }) = nodes[idx].as_mut() {
                        *children = child_indices;
                    }
                    idx
                }
            }
        }

        for i in 0..orig_len {
            match self.nodes[i].clone() {
                None => nodes_out[i] = None,
                Some(FlatTreeNode::Branch { children, meta }) => {
                    nodes_out[i] = Some(FlatTreeNode::Branch { children, meta });
                }
                Some(FlatTreeNode::Leaf { node, meta }) => {
                    let trees = map(node)?;
                    let mut child_indices = Vec::with_capacity(trees.len());
                    for t in trees {
                        let idx = append_tree(t, &mut nodes_out);
                        child_indices.push(idx);
                    }
                    nodes_out[i] = Some(FlatTreeNode::Branch {
                        children: child_indices,
                        meta,
                    });
                }
            }
        }

        Ok(FlatTree {
            nodes: nodes_out,
            root_index: self.root_index,
        })
    }

    /// Async map of leaves into sub-trees with Result.
    pub fn map_tree_result_async<NextNode, Error, MapFn, Fut>(
        self,
        map: MapFn,
    ) -> Pin<Box<dyn Future<Output = Result<FlatTree<NextNode, Meta>, Error>> + Send + 'static>>
    where
        Node: Send + 'static,
        NextNode: Send + 'static,
        Error: Send + 'static,
        MapFn: Fn(Node) -> Fut + Copy + Send + 'static,
        Fut: Future<Output = Result<Vec<Tree<NextNode, Meta>>, Error>> + Send + 'static,
    {
        Box::pin(async move {
            let orig_len = self.nodes.len();
            let mut nodes_out: Vec<Option<FlatTreeNode<NextNode, Meta>>> =
                Vec::with_capacity(orig_len);
            nodes_out.resize_with(orig_len, || None);

            fn append_tree<Node, Meta>(
                tree: Tree<Node, Meta>,
                nodes: &mut Vec<Option<FlatTreeNode<Node, Meta>>>,
            ) -> usize {
                match tree {
                    Tree::Leaf { node, meta } => {
                        let idx = nodes.len();
                        nodes.push(Some(FlatTreeNode::Leaf { node, meta }));
                        idx
                    }
                    Tree::Branch { mut children, meta } => {
                        let idx = nodes.len();
                        nodes.push(Some(FlatTreeNode::Branch {
                            children: Vec::new(),
                            meta,
                        }));
                        let mut child_indices = Vec::with_capacity(children.len());
                        for child in children.drain(..) {
                            let cidx = append_tree(child, nodes);
                            child_indices.push(cidx);
                        }
                        if let Some(FlatTreeNode::Branch { children, .. }) = nodes[idx].as_mut() {
                            *children = child_indices;
                        }
                        idx
                    }
                }
            }

            // Prepare jobs for leaves; copy through branches.
            let mut jobs: Vec<(
                usize,
                Meta,
                Pin<Box<dyn Future<Output = Result<Vec<Tree<NextNode, Meta>>, Error>> + Send>>,
            )> = Vec::new();

            for (i, maybe) in self.nodes.into_iter().enumerate() {
                match maybe {
                    None => nodes_out[i] = None,
                    Some(FlatTreeNode::Branch { children, meta }) => {
                        nodes_out[i] = Some(FlatTreeNode::Branch { children, meta });
                    }
                    Some(FlatTreeNode::Leaf { node, meta }) => {
                        nodes_out[i] = None; // to be filled
                        let fut = map(node);
                        jobs.push((i, meta, Box::pin(fut)));
                    }
                }
            }

            for (i, meta, fut) in jobs {
                let trees = fut.await?;
                let mut child_indices = Vec::with_capacity(trees.len());
                for t in trees {
                    let idx = append_tree(t, &mut nodes_out);
                    child_indices.push(idx);
                }
                nodes_out[i] = Some(FlatTreeNode::Branch {
                    children: child_indices,
                    meta,
                });
            }

            Ok(FlatTree {
                nodes: nodes_out,
                root_index: self.root_index,
            })
        })
    }
}

/// From<Tree> -> FlatTree: flatten a tree into a single vector.
impl<Node, Meta> From<Tree<Node, Meta>> for FlatTree<Node, Meta> {
    fn from(tree: Tree<Node, Meta>) -> Self {
        fn append<Node, Meta>(
            tree: Tree<Node, Meta>,
            nodes: &mut Vec<Option<FlatTreeNode<Node, Meta>>>,
        ) -> usize {
            match tree {
                Tree::Leaf { node, meta } => {
                    let idx = nodes.len();
                    nodes.push(Some(FlatTreeNode::Leaf { node, meta }));
                    idx
                }
                Tree::Branch { mut children, meta } => {
                    let idx = nodes.len();
                    nodes.push(Some(FlatTreeNode::Branch {
                        children: Vec::new(),
                        meta,
                    }));
                    let mut child_indices = Vec::with_capacity(children.len());
                    for child in children.drain(..) {
                        let cidx = append(child, nodes);
                        child_indices.push(cidx);
                    }
                    if let Some(FlatTreeNode::Branch { children, .. }) = nodes[idx].as_mut() {
                        *children = child_indices;
                    }
                    idx
                }
            }
        }

        let mut nodes = Vec::new();
        let root_index = append(tree, &mut nodes);
        FlatTree { nodes, root_index }
    }
}

/// From<FlatTree> -> Tree: reconstruct a nested tree. This is lenient:
/// - Missing or invalid children are skipped.
/// - If the root is missing, returns an empty Branch with default meta.
///
/// For strict validation and error reporting, use `TryFrom` via
/// `flat.validate()?; let tree: Tree<_, _> = flat.into();`
/// or `Tree::<_, _>::try_from(flat)`.
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

/// TryFrom<FlatTree> -> Tree with strict validation using `thiserror`.
impl<Node, Meta> TryFrom<FlatTree<Node, Meta>> for Tree<Node, Meta>
where
    Meta: Default,
{
    type Error = FlatTreeError;

    fn try_from(flat: FlatTree<Node, Meta>) -> Result<Self, Self::Error> {
        flat.validate()?;
        Ok(Tree::from(flat))
    }
}
