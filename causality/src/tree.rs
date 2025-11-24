use std::pin::Pin;

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
        before: Vec<NodeId>,
        after: Vec<NodeId>,
        node: Node,
    },
}

impl<Node> Tree<Node> {
    pub fn branch(children: Vec<Tree<Node>>) -> Self {
        Self::Branch {
            id: None,
            before: vec![],
            after: vec![],
            children,
        }
    }

    pub fn leaf(node: Node) -> Self {
        Self::Leaf {
            id: None,
            before: vec![],
            after: vec![],
            node,
        }
    }

    pub fn map<NextNode, MapFn>(self, map: MapFn) -> Tree<NextNode>
    where
        MapFn: Fn(Node) -> NextNode + Copy,
    {
        match self {
            Tree::Branch {
                id,
                before,
                after,
                children,
            } => Tree::Branch {
                id,
                before,
                after,
                children: children
                    .into_iter()
                    .map(|tree| Self::map(tree, map))
                    .collect(),
            },
            Tree::Leaf {
                id,
                before,
                after,
                node,
            } => Tree::Leaf {
                id,
                before,
                after,
                node: map(node),
            },
        }
    }

    pub fn map_option<NextNode, MapFn>(self, map: MapFn) -> Option<Tree<NextNode>>
    where
        MapFn: Fn(Node) -> Option<NextNode> + Copy,
    {
        match self {
            Tree::Branch {
                id,
                before,
                after,
                children,
            } => {
                // Recursively map all children and keep only those that remain Some
                let children: Vec<Tree<NextNode>> = children
                    .into_iter()
                    .filter_map(|child| child.map_option(map))
                    .collect();

                // If no children remain, the branch disappears entirely
                if children.is_empty() {
                    None
                } else {
                    Some(Tree::Branch {
                        id,
                        before,
                        after,
                        children,
                    })
                }
            }

            Tree::Leaf {
                id,
                before,
                after,
                node,
            } => map(node).map(|node| Tree::Leaf {
                id,
                before,
                after,
                node,
            }),
        }
    }

    pub fn map_result<NextNode, Error, MapFn>(self, map: MapFn) -> Result<Tree<NextNode>, Error>
    where
        MapFn: Fn(Node) -> Result<NextNode, Error> + Copy,
    {
        match self {
            Tree::Branch {
                id,
                before,
                after,
                children,
            } => {
                let children = children
                    .into_iter()
                    .map(|tree| tree.map_result(map))
                    .collect::<Result<Vec<_>, Error>>()?;

                Ok(Tree::Branch {
                    id,
                    before,
                    after,
                    children,
                })
            }
            Tree::Leaf {
                id,
                before,
                after,
                node,
            } => Ok(Tree::Leaf {
                id,
                before,
                after,
                node: map(node)?,
            }),
        }
    }

    pub fn map_result_async<NextNode, Error, MapFn, Fut>(
        self,
        map: MapFn,
    ) -> Pin<Box<dyn Future<Output = Result<Tree<NextNode>, Error>> + Send + 'static>>
    where
        Node: Send + 'static,
        NextNode: Send + 'static,
        Error: Send + 'static,
        MapFn: Fn(Node) -> Fut + Copy + Send + 'static,
        Fut: Future<Output = Result<NextNode, Error>> + Send + 'static,
    {
        match self {
            Tree::Branch {
                id,
                before,
                after,
                children,
            } => {
                // Build futures for each child first...
                #[allow(clippy::type_complexity)]
                let futures: Vec<
                    Pin<Box<dyn Future<Output = Result<Tree<NextNode>, Error>> + Send + 'static>>,
                > = children
                    .into_iter()
                    .map(|tree| tree.map_result_async(map))
                    .collect();

                // ...then await them and rebuild the branch.
                Box::pin(async move {
                    let mut mapped_children = Vec::with_capacity(futures.len());
                    for fut in futures {
                        mapped_children.push(fut.await?);
                    }
                    Ok(Tree::Branch {
                        id,
                        before,
                        after,
                        children: mapped_children,
                    })
                })
            }
            Tree::Leaf {
                id,
                before,
                after,
                node,
            } => Box::pin(async move {
                let node = map(node).await?;
                Ok(Tree::Leaf {
                    id,
                    before,
                    after,
                    node,
                })
            }),
        }
    }

    pub fn map_tree<NextNode, MapFn>(self, map: MapFn) -> Tree<NextNode>
    where
        MapFn: Fn(Node) -> Vec<Tree<NextNode>> + Copy,
    {
        match self {
            Tree::Branch {
                id,
                before,
                after,
                children,
            } => Tree::Branch {
                id,
                before,
                after,
                children: children
                    .into_iter()
                    .map(|tree| Self::map_tree(tree, map))
                    .collect(),
            },
            Tree::Leaf {
                id,
                before,
                after,
                node,
            } => Tree::Branch {
                id,
                before,
                after,
                children: map(node),
            },
        }
    }

    pub fn map_tree_result<NextNode, Error, MapFn>(
        self,
        map: MapFn,
    ) -> Result<Tree<NextNode>, Error>
    where
        MapFn: Fn(Node) -> Result<Vec<Tree<NextNode>>, Error> + Copy,
    {
        match self {
            Tree::Branch {
                id,
                before,
                after,
                children,
            } => {
                let mut mapped = Vec::with_capacity(children.len());
                for child in children {
                    mapped.push(child.map_tree_result(map)?);
                }
                Ok(Tree::Branch {
                    id,
                    before,
                    after,
                    children: mapped,
                })
            }
            Tree::Leaf {
                id,
                before,
                after,
                node,
            } => {
                let children = map(node)?;
                Ok(Tree::Branch {
                    id,
                    before,
                    after,
                    children,
                })
            }
        }
    }

    pub fn map_tree_result_async<NextNode, Error, MapFn, Fut>(
        self,
        map: MapFn,
    ) -> Pin<Box<dyn Future<Output = Result<Tree<NextNode>, Error>> + Send + 'static>>
    where
        Node: Send + 'static,
        NextNode: Send + 'static,
        Error: Send + 'static,
        MapFn: Fn(Node) -> Fut + Copy + Send + 'static,
        Fut: Future<Output = Result<Vec<Tree<NextNode>>, Error>> + Send + 'static,
    {
        match self {
            Tree::Branch {
                id,
                before,
                after,
                children,
            } => Box::pin(async move {
                let mut mapped_children = Vec::with_capacity(children.len());
                for child in children {
                    mapped_children.push(child.map_tree_result_async(map).await?);
                }
                Ok(Tree::Branch {
                    id,
                    before,
                    after,
                    children: mapped_children,
                })
            }),
            Tree::Leaf {
                id,
                before,
                after,
                node,
            } => Box::pin(async move {
                let children = map(node).await?;
                Ok(Tree::Branch {
                    id,
                    before,
                    after,
                    children,
                })
            }),
        }
    }
}
