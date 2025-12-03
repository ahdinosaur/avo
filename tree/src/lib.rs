use std::collections::HashMap;
use std::fmt::Display;
use std::future::Future;
use std::hash::Hash;
use std::pin::Pin;

use lusid_view::{Render, Tree as ViewTree, ViewNode};
use thiserror::Error;

#[derive(Debug, Clone)]
pub enum TreeElement<NodeId, Node, Meta> {
    Branch {
        id: NodeId,
        children: Vec<TreeElement<NodeId, Node, Meta>>,
        meta: Meta,
    },
    Leaf {
        id: NodeId,
        node: Node,
        meta: Meta,
    },
}

impl<NodeId, Node, Meta> TreeElement<NodeId, Node, Meta> {
    pub fn branch(id: NodeId, children: Vec<TreeElement<NodeId, Node, Meta>>, meta: Meta) -> Self {
        Self::Branch { id, children, meta }
    }

    pub fn leaf(id: NodeId, node: Node, meta: Meta) -> Self {
        Self::Leaf { id, node, meta }
    }

    fn id(&self) -> &NodeId {
        match self {
            TreeElement::Branch { id, .. } | TreeElement::Leaf { id, .. } => id,
        }
    }
}

#[derive(Debug, Clone)]
pub enum TreeNode<NodeId, Node, Meta> {
    Branch { children: Vec<NodeId>, meta: Meta },
    Leaf { node: Option<Node>, meta: Meta },
}

#[derive(Debug, Clone)]
pub struct Tree<NodeId, Node, Meta> {
    nodes: Vec<TreeNode<NodeId, Node, Meta>>,
    index_by_id: HashMap<NodeId, usize>,
    root_id: NodeId,
}

#[derive(Error, Debug)]
pub enum TreeError<NodeId> {
    #[error("duplicate node id: {0:?}")]
    Duplicateid(NodeId),
    #[error("missing node id: {0:?}")]
    Missingid(NodeId),
    #[error("node is not a branch: {0:?}")]
    NotABranch(NodeId),
}

#[derive(Error, Debug)]
pub enum TreeMapError<NodeId, UserError> {
    #[error(transparent)]
    Structural(#[from] TreeError<NodeId>),
    #[error("user mapping error: {0}")]
    User(UserError),
}

impl<NodeId, Node, Meta> Tree<NodeId, Node, Meta>
where
    NodeId: Eq + Hash + Clone,
{
    pub fn from_tree_elements(
        root: TreeElement<NodeId, Node, Meta>,
    ) -> Result<Self, TreeError<NodeId>> {
        let mut nodes = Vec::new();
        let mut index_by_id = HashMap::new();
        let root_id = root.id().clone();

        append_element_flat(&mut nodes, &mut index_by_id, 0, root)?;

        Ok(Self {
            nodes,
            index_by_id,
            root_id,
        })
    }

    pub fn new(root: TreeElement<NodeId, Node, Meta>) -> Result<Self, TreeError<NodeId>> {
        Self::from_tree_elements(root)
    }

    pub fn root_id(&self) -> &NodeId {
        &self.root_id
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn contains(&self, id: &NodeId) -> bool {
        self.index_by_id.contains_key(id)
    }

    pub fn get(&self, id: &NodeId) -> Option<&TreeNode<NodeId, Node, Meta>> {
        self.index_by_id.get(id).map(|&index| &self.nodes[index])
    }

    pub fn get_mut(&mut self, id: &NodeId) -> Option<&mut TreeNode<NodeId, Node, Meta>> {
        self.index_by_id
            .get(id)
            .map(|&index| &mut self.nodes[index])
    }

    pub fn add_children(
        &mut self,
        parent_id: &NodeId,
        new_children: Vec<TreeElement<NodeId, Node, Meta>>,
    ) -> Result<(), TreeError<NodeId>> {
        let parent_index = *self
            .index_by_id
            .get(parent_id)
            .ok_or_else(|| TreeError::Missingid(parent_id.clone()))?;

        let is_branch = matches!(self.nodes[parent_index], TreeNode::Branch { .. });
        if !is_branch {
            return Err(TreeError::NotABranch(parent_id.clone()));
        }

        let mut child_ids = Vec::new();
        for element in new_children {
            let child_root_id = element.id().clone();
            append_element_flat(&mut self.nodes, &mut self.index_by_id, 0, element)?;
            child_ids.push(child_root_id);
        }

        if let TreeNode::Branch { children, .. } = &mut self.nodes[parent_index] {
            children.extend(child_ids);
        }

        Ok(())
    }

    pub fn map<NextNode, MapFunction>(
        self,
        mut map_function: MapFunction,
    ) -> Tree<NodeId, NextNode, Meta>
    where
        MapFunction: FnMut(Node) -> NextNode,
    {
        let Self {
            nodes,
            index_by_id,
            root_id,
        } = self;

        let nodes = nodes
            .into_iter()
            .map(|node| match node {
                TreeNode::Branch { children, meta } => TreeNode::Branch { children, meta },
                TreeNode::Leaf { node, meta } => TreeNode::Leaf {
                    node: node.map(|n| map_function(n)),
                    meta,
                },
            })
            .collect();

        Tree {
            nodes,
            index_by_id,
            root_id,
        }
    }

    pub fn map_result<NextNode, Error, MapFunction>(
        self,
        mut map_function: MapFunction,
    ) -> Result<Tree<NodeId, NextNode, Meta>, Error>
    where
        MapFunction: FnMut(Node) -> Result<NextNode, Error>,
    {
        let Self {
            nodes,
            index_by_id,
            root_id,
        } = self;

        let mut out = Vec::with_capacity(nodes.len());
        for node in nodes {
            match node {
                TreeNode::Branch { children, meta } => {
                    out.push(TreeNode::Branch { children, meta })
                }
                TreeNode::Leaf { node, meta } => {
                    let mapped = match node {
                        Some(n) => Some(map_function(n)?),
                        None => None,
                    };
                    out.push(TreeNode::Leaf { node: mapped, meta });
                }
            }
        }

        Ok(Tree {
            nodes: out,
            index_by_id,
            root_id,
        })
    }

    pub fn map_result_async<NextNode, Error, MapFunction, Fut>(
        self,
        mut map_function: MapFunction,
    ) -> Pin<Box<dyn Future<Output = Result<Tree<NodeId, NextNode, Meta>, Error>> + Send + 'static>>
    where
        Node: Send + 'static,
        NextNode: Send + 'static,
        Error: Send + 'static,
        MapFunction: FnMut(Node) -> Fut + Send + 'static,
        Fut: Future<Output = Result<NextNode, Error>> + Send + 'static,
        NodeId: Send + 'static,
        Meta: Send + 'static,
    {
        let Tree {
            nodes,
            index_by_id,
            root_id,
        } = self;

        Box::pin(async move {
            let mut out = Vec::with_capacity(nodes.len());
            for node in nodes {
                match node {
                    TreeNode::Branch { children, meta } => {
                        out.push(TreeNode::Branch { children, meta })
                    }
                    TreeNode::Leaf { node, meta } => {
                        let mapped = if let Some(n) = node {
                            Some(map_function(n).await?)
                        } else {
                            None
                        };
                        out.push(TreeNode::Leaf { node: mapped, meta });
                    }
                }
            }

            Ok(Tree {
                nodes: out,
                index_by_id,
                root_id,
            })
        })
    }

    pub fn map_option<NextNode, MapFunction>(
        self,
        mut map_function: MapFunction,
    ) -> Tree<NodeId, NextNode, Meta>
    where
        MapFunction: FnMut(Node) -> Option<NextNode>,
    {
        let Self {
            nodes,
            index_by_id,
            root_id,
        } = self;

        let nodes = nodes
            .into_iter()
            .map(|node| match node {
                TreeNode::Branch { children, meta } => TreeNode::Branch { children, meta },
                TreeNode::Leaf { node, meta } => TreeNode::Leaf {
                    node: node.and_then(|n| map_function(n)),
                    meta,
                },
            })
            .collect();

        Tree {
            nodes,
            index_by_id,
            root_id,
        }
    }

    pub fn map_tree<NextNode, MapFunction>(
        self,
        mut map_function: MapFunction,
    ) -> Result<Tree<NodeId, NextNode, Meta>, TreeError<NodeId>>
    where
        NextNode: Clone,
        Meta: Clone,
        MapFunction: FnMut(Node) -> Vec<TreeElement<NodeId, NextNode, Meta>>,
    {
        let Self {
            nodes,
            mut index_by_id,
            root_id,
        } = self;

        let original_len = nodes.len();
        let mut prefix: Vec<Option<TreeNode<NodeId, NextNode, Meta>>> = vec![None; original_len];
        let mut tail_nodes: Vec<TreeNode<NodeId, NextNode, Meta>> = Vec::new();

        for (index, tree_node) in nodes.into_iter().enumerate() {
            match tree_node {
                TreeNode::Branch { children, meta } => {
                    prefix[index] = Some(TreeNode::Branch { children, meta });
                }
                TreeNode::Leaf {
                    node: Some(node_data),
                    meta,
                } => {
                    let new_elements = map_function(node_data);
                    let mut child_ids = Vec::with_capacity(new_elements.len());

                    for element in new_elements {
                        let child_id = element.id().clone();
                        append_element_flat(
                            &mut tail_nodes,
                            &mut index_by_id,
                            original_len,
                            element,
                        )?;
                        child_ids.push(child_id);
                    }

                    prefix[index] = Some(TreeNode::Branch {
                        children: child_ids,
                        meta,
                    });
                }
                TreeNode::Leaf { node: None, meta } => {
                    prefix[index] = Some(TreeNode::Leaf { node: None, meta });
                }
            }
        }

        let mut nodes_out = prefix.into_iter().map(|n| n.unwrap()).collect::<Vec<_>>();
        nodes_out.extend(tail_nodes);

        Ok(Tree {
            nodes: nodes_out,
            index_by_id,
            root_id,
        })
    }

    pub fn map_tree_result<NextNode, UserError, MapFunction>(
        self,
        mut map_function: MapFunction,
    ) -> Result<Tree<NodeId, NextNode, Meta>, TreeMapError<NodeId, UserError>>
    where
        NextNode: Clone,
        Meta: Clone,
        MapFunction: FnMut(Node) -> Result<Vec<TreeElement<NodeId, NextNode, Meta>>, UserError>,
    {
        let Self {
            nodes,
            mut index_by_id,
            root_id,
        } = self;

        let original_len = nodes.len();
        let mut prefix: Vec<Option<TreeNode<NodeId, NextNode, Meta>>> = vec![None; original_len];
        let mut tail_nodes: Vec<TreeNode<NodeId, NextNode, Meta>> = Vec::new();

        for (index, tree_node) in nodes.into_iter().enumerate() {
            match tree_node {
                TreeNode::Branch { children, meta } => {
                    prefix[index] = Some(TreeNode::Branch { children, meta });
                }
                TreeNode::Leaf {
                    node: Some(node_data),
                    meta,
                } => {
                    let new_elements = map_function(node_data).map_err(TreeMapError::User)?;
                    let mut child_ids = Vec::with_capacity(new_elements.len());

                    for element in new_elements {
                        let child_id = element.id().clone();
                        append_element_flat(
                            &mut tail_nodes,
                            &mut index_by_id,
                            original_len,
                            element,
                        )?;
                        child_ids.push(child_id);
                    }

                    prefix[index] = Some(TreeNode::Branch {
                        children: child_ids,
                        meta,
                    });
                }
                TreeNode::Leaf { node: None, meta } => {
                    prefix[index] = Some(TreeNode::Leaf { node: None, meta });
                }
            }
        }

        let mut nodes_out = prefix.into_iter().map(|n| n.unwrap()).collect::<Vec<_>>();
        nodes_out.extend(tail_nodes);

        Ok(Tree {
            nodes: nodes_out,
            index_by_id,
            root_id,
        })
    }

    pub fn map_tree_result_async<NextNode, UserError, MapFunction, Fut>(
        self,
        mut map_function: MapFunction,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<Tree<NodeId, NextNode, Meta>, TreeMapError<NodeId, UserError>>,
                > + Send
                + 'static,
        >,
    >
    where
        Node: Send + 'static,
        NextNode: Send + Clone + 'static,
        UserError: Send + 'static,
        MapFunction: FnMut(Node) -> Fut + Send + 'static,
        Fut: Future<Output = Result<Vec<TreeElement<NodeId, NextNode, Meta>>, UserError>>
            + Send
            + 'static,
        NodeId: Send + 'static,
        Meta: Send + Clone + 'static,
    {
        let Tree {
            nodes,
            mut index_by_id,
            root_id,
        } = self;

        Box::pin(async move {
            let original_len = nodes.len();
            let mut prefix: Vec<Option<TreeNode<NodeId, NextNode, Meta>>> =
                vec![None; original_len];
            let mut tail_nodes: Vec<TreeNode<NodeId, NextNode, Meta>> = Vec::new();

            for (index, tree_node) in nodes.into_iter().enumerate() {
                match tree_node {
                    TreeNode::Branch { children, meta } => {
                        prefix[index] = Some(TreeNode::Branch { children, meta });
                    }
                    TreeNode::Leaf {
                        node: Some(node_data),
                        meta,
                    } => {
                        let new_elements =
                            map_function(node_data).await.map_err(TreeMapError::User)?;
                        let mut child_ids = Vec::with_capacity(new_elements.len());

                        for element in new_elements {
                            let child_id = element.id().clone();
                            append_element_flat(
                                &mut tail_nodes,
                                &mut index_by_id,
                                original_len,
                                element,
                            )?;
                            child_ids.push(child_id);
                        }

                        prefix[index] = Some(TreeNode::Branch {
                            children: child_ids,
                            meta,
                        });
                    }
                    TreeNode::Leaf { node: None, meta } => {
                        prefix[index] = Some(TreeNode::Leaf { node: None, meta });
                    }
                }
            }

            let mut nodes_out = prefix.into_iter().map(|n| n.unwrap()).collect::<Vec<_>>();
            nodes_out.extend(tail_nodes);

            Ok(Tree {
                nodes: nodes_out,
                index_by_id,
                root_id,
            })
        })
    }

    pub fn to_tree_elements(&self) -> Option<TreeElement<NodeId, Node, Meta>>
    where
        Node: Clone,
        Meta: Clone,
    {
        fn build<NodeId, Node, Meta>(
            tree: &Tree<NodeId, Node, Meta>,
            id: &NodeId,
        ) -> Option<TreeElement<NodeId, Node, Meta>>
        where
            NodeId: Eq + Hash + Clone,
            Node: Clone,
            Meta: Clone,
        {
            let index = *tree.index_by_id.get(id)?;
            match &tree.nodes[index] {
                TreeNode::Leaf {
                    node: Some(node),
                    meta,
                } => Some(TreeElement::Leaf {
                    id: id.clone(),
                    node: node.clone(),
                    meta: meta.clone(),
                }),
                TreeNode::Leaf { node: None, .. } => None,
                TreeNode::Branch { children, meta } => {
                    let mut kept_children = Vec::new();
                    for child_id in children {
                        if let Some(child_el) = build(tree, child_id) {
                            kept_children.push(child_el);
                        }
                    }
                    if kept_children.is_empty() {
                        None
                    } else {
                        Some(TreeElement::Branch {
                            id: id.clone(),
                            children: kept_children,
                            meta: meta.clone(),
                        })
                    }
                }
            }
        }

        build(self, &self.root_id)
    }

    pub fn compact(&self) -> Option<Self>
    where
        Node: Clone,
        Meta: Clone,
    {
        let elements = self.to_tree_elements()?;
        Self::from_tree_elements(elements).ok()
    }
}

fn append_element_flat<NodeId, Node, Meta>(
    nodes_out: &mut Vec<TreeNode<NodeId, Node, Meta>>,
    index_by_id: &mut HashMap<NodeId, usize>,
    base_index: usize,
    element: TreeElement<NodeId, Node, Meta>,
) -> Result<(), TreeError<NodeId>>
where
    NodeId: Eq + Hash + Clone,
{
    match element {
        TreeElement::Leaf { id, node, meta } => {
            if index_by_id.contains_key(&id) {
                return Err(TreeError::Duplicateid(id));
            }
            let current_index = base_index + nodes_out.len();
            nodes_out.push(TreeNode::Leaf {
                node: Some(node),
                meta,
            });
            index_by_id.insert(id, current_index);
        }
        TreeElement::Branch { id, children, meta } => {
            if index_by_id.contains_key(&id) {
                return Err(TreeError::Duplicateid(id));
            }
            let current_index = base_index + nodes_out.len();

            let child_ids = children.iter().map(|c| c.id().clone()).collect::<Vec<_>>();

            nodes_out.push(TreeNode::Branch {
                children: child_ids,
                meta,
            });
            index_by_id.insert(id, current_index);

            for child in children {
                append_element_flat(nodes_out, index_by_id, base_index, child)?;
            }
        }
    }
    Ok(())
}

impl<NodeId, Node, Meta> Render for Tree<NodeId, Node, Meta>
where
    NodeId: Display + Clone + Eq + Hash,
    Node: Display,
{
    fn render(&self) -> ViewNode {
        fn render_from<NodeId, Node, Meta>(tree: &Tree<NodeId, Node, Meta>, id: &NodeId) -> ViewTree
        where
            NodeId: Display + Clone + Eq + Hash,
            Node: Display,
        {
            let index = *tree.index_by_id.get(id).expect("invalid id in tree");

            match &tree.nodes[index] {
                TreeNode::Branch { children, .. } => ViewTree::Branch {
                    label: id.to_string(),
                    nodes: children
                        .iter()
                        .map(|child_id| render_from(tree, child_id))
                        .collect(),
                },
                TreeNode::Leaf { node, .. } => {
                    let label = match node {
                        Some(n) => format!("{id}: {n}"),
                        None => format!("{id}"),
                    };
                    ViewTree::Leaf { label }
                }
            }
        }

        render_from(self, &self.root_id).into()
    }
}
