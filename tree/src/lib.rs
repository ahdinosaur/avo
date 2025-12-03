use std::{fmt::Display, pin::Pin};

use lusid_view::{Render, Tree as ViewTree, ViewNode};
#[derive(Debug, Clone)]
pub enum Tree<Node, Meta> {
    Branch {
        id: Option<NodeId>,
        children: Vec<Tree<Node, Meta>>,
        meta: Meta,
    },
    Leaf {
        id: Option<NodeId>,
        node: Node,
        meta: Meta,
    },
}

impl<Node, Meta> Tree<Node, Meta>
where
    Meta: Default + Send + 'static,
{
    pub fn branch(children: Vec<Tree<Node, Meta>>) -> Self {
        Self::Branch {
            id: None,
            children,
            meta: Meta::default(),
        }
    }

    pub fn leaf(node: Node) -> Self {
        Self::Leaf {
            id: None,
            node,
            meta: Meta::default(),
        }
    }

    pub fn map<NextNode, MapFn>(self, map: MapFn) -> Tree<NextNode, Meta>
    where
        MapFn: Fn(Node) -> NextNode + Copy,
    {
        match self {
            Tree::Branch { id, children, meta } => Tree::Branch {
                id,
                children: children
                    .into_iter()
                    .map(|tree| Self::map(tree, map))
                    .collect(),
                meta,
            },
            Tree::Leaf { id, node, meta } => Tree::Leaf {
                id,
                node: map(node),
                meta,
            },
        }
    }

    pub fn map_option<NextNode, MapFn>(self, map: MapFn) -> Option<Tree<NextNode, Meta>>
    where
        MapFn: Fn(Node) -> Option<NextNode> + Copy,
    {
        match self {
            Tree::Branch { id, children, meta } => {
                // Recursively map all children and keep only those that remain Some
                let children: Vec<Tree<NextNode, Meta>> = children
                    .into_iter()
                    .filter_map(|child| child.map_option(map))
                    .collect();

                // If no children remain, the branch disappears entirely
                if children.is_empty() {
                    None
                } else {
                    Some(Tree::Branch { id, children, meta })
                }
            }

            Tree::Leaf { id, node, meta } => map(node).map(|node| Tree::Leaf { id, node, meta }),
        }
    }

    pub fn map_result<NextNode, Error, MapFn>(
        self,
        map: MapFn,
    ) -> Result<Tree<NextNode, Meta>, Error>
    where
        MapFn: Fn(Node) -> Result<NextNode, Error> + Copy,
    {
        match self {
            Tree::Branch { id, children, meta } => {
                let children = children
                    .into_iter()
                    .map(|tree| tree.map_result(map))
                    .collect::<Result<Vec<_>, Error>>()?;

                Ok(Tree::Branch { id, children, meta })
            }
            Tree::Leaf { id, node, meta } => Ok(Tree::Leaf {
                id,
                node: map(node)?,
                meta,
            }),
        }
    }

    pub fn map_result_async<NextNode, Error, MapFn, Fut>(
        self,
        map: MapFn,
    ) -> Pin<Box<dyn Future<Output = Result<Tree<NextNode, Meta>, Error>> + Send + 'static>>
    where
        Node: Send + 'static,
        NextNode: Send + 'static,
        Error: Send + 'static,
        MapFn: Fn(Node) -> Fut + Copy + Send + 'static,
        Fut: Future<Output = Result<NextNode, Error>> + Send + 'static,
    {
        match self {
            Tree::Branch { id, children, meta } => {
                // Build futures for each child first...
                #[allow(clippy::type_complexity)]
                let futures: Vec<
                    Pin<
                        Box<
                            dyn Future<Output = Result<Tree<NextNode, Meta>, Error>>
                                + Send
                                + 'static,
                        >,
                    >,
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
                        children: mapped_children,
                        meta,
                    })
                })
            }
            Tree::Leaf { id, node, meta } => Box::pin(async move {
                let node = map(node).await?;
                Ok(Tree::Leaf { id, node, meta })
            }),
        }
    }

    pub fn map_tree<NextNode, MapFn>(self, map: MapFn) -> Tree<NextNode, Meta>
    where
        MapFn: Fn(Node) -> Vec<Tree<NextNode, Meta>> + Copy,
    {
        match self {
            Tree::Branch { id, children, meta } => Tree::Branch {
                id,
                children: children
                    .into_iter()
                    .map(|tree| Self::map_tree(tree, map))
                    .collect(),
                meta,
            },
            Tree::Leaf { id, node, meta } => Tree::Branch {
                id,
                children: map(node),
                meta,
            },
        }
    }

    pub fn map_tree_result<NextNode, Error, MapFn>(
        self,
        map: MapFn,
    ) -> Result<Tree<NextNode, Meta>, Error>
    where
        MapFn: Fn(Node) -> Result<Vec<Tree<NextNode, Meta>>, Error> + Copy,
    {
        match self {
            Tree::Branch { id, children, meta } => {
                let mut mapped = Vec::with_capacity(children.len());
                for child in children {
                    mapped.push(child.map_tree_result(map)?);
                }
                Ok(Tree::Branch {
                    id,
                    children: mapped,
                    meta,
                })
            }
            Tree::Leaf { id, node, meta } => {
                let children = map(node)?;
                Ok(Tree::Branch { id, children, meta })
            }
        }
    }

    pub fn map_tree_result_async<NextNode, Error, MapFn, Fut>(
        self,
        map: MapFn,
    ) -> Pin<Box<dyn Future<Output = Result<Tree<NextNode, Meta>, Error>> + Send + 'static>>
    where
        Node: Send + 'static,
        NextNode: Send + 'static,
        Error: Send + 'static,
        MapFn: Fn(Node) -> Fut + Copy + Send + 'static,
        Fut: Future<Output = Result<Vec<Tree<NextNode, Meta>>, Error>> + Send + 'static,
    {
        match self {
            Tree::Branch { id, children, meta } => Box::pin(async move {
                let mut mapped_children = Vec::with_capacity(children.len());
                for child in children {
                    mapped_children.push(child.map_tree_result_async(map).await?);
                }
                Ok(Tree::Branch {
                    id,
                    children: mapped_children,
                    meta,
                })
            }),
            Tree::Leaf { id, node, meta } => Box::pin(async move {
                let children = map(node).await?;
                Ok(Tree::Branch { id, children, meta })
            }),
        }
    }
}

impl<Node, Meta> Render for Tree<Node, Meta>
where
    Node: Display,
{
    fn render(&self) -> ViewNode {
        fn render_tree<Node, Meta>(tree: &Tree<Node, Meta>) -> ViewTree
        where
            Node: Display,
        {
            match tree {
                Tree::Branch {
                    id,
                    children,
                    meta: _,
                } => {
                    let id = id
                        .clone()
                        .map(|id| id.into_inner())
                        .unwrap_or("?".to_string());
                    ViewTree::Branch {
                        label: id,
                        nodes: children.iter().map(|child| render_tree(child)).collect(),
                    }
                }
                Tree::Leaf { id, node, meta: _ } => {
                    let id = id
                        .clone()
                        .map(|id| id.into_inner())
                        .unwrap_or("?".to_string());
                    ViewTree::Leaf {
                        label: format!("{id}: {node}"),
                    }
                }
            }
        }

        render_tree(self).into()
    }
}
