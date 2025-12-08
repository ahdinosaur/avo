use cuid2::create_id;
use lusid_causality::CausalityMeta;
use lusid_tree::{FlatTree, FlatTreeMappedItem, FlatTreeNode, FlatTreeUpdate, Tree};
use lusid_view::{Render, ViewTree};

use crate::PlanNodeId;

pub type PlanTree<Node> = Tree<Node, PlanMeta>;
pub type PlanMeta = CausalityMeta<PlanNodeId>;
pub type PlanFlatTree<Node> = FlatTree<Node, PlanMeta>;
pub type PlanFlatTreeNode<Node> = FlatTreeNode<Node, PlanMeta>;
pub type PlanFlatTreeMappedItem<Node> = FlatTreeMappedItem<Node, PlanMeta>;
pub type PlanFlatTreeUpdate<Node> = FlatTreeUpdate<Node, PlanMeta>;

pub fn map_plan_subitems<Node, NextNode, F>(
    node: Option<PlanFlatTreeNode<Node>>,
    map: F,
) -> Option<PlanFlatTreeUpdate<NextNode>>
where
    F: Fn(Node) -> Vec<Tree<NextNode, CausalityMeta<String>>>,
{
    let node = node?;
    Some(node.update(|node| {
        let subtrees = map(node);
        let scope_id = create_id();
        let subtrees = subtrees
            .into_iter()
            .map(|tree| {
                tree.map_meta(|meta| CausalityMeta {
                    id: meta.id.map(|item_id| PlanNodeId::SubItem {
                        scope_id: scope_id.clone(),
                        item_id,
                    }),
                    before: meta
                        .before
                        .into_iter()
                        .map(|item_id| PlanNodeId::SubItem {
                            scope_id: scope_id.clone(),
                            item_id,
                        })
                        .collect(),
                    after: meta
                        .after
                        .into_iter()
                        .map(|item_id| PlanNodeId::SubItem {
                            scope_id: scope_id.clone(),
                            item_id,
                        })
                        .collect(),
                })
            })
            .collect();
        FlatTreeMappedItem::SubTrees(subtrees)
    }))
}

pub fn plan_view_tree<Node>(tree: PlanTree<Node>) -> ViewTree
where
    Node: Render,
{
    match tree {
        Tree::Branch { meta, children } => ViewTree::Branch {
            view: meta.id.map(|id| id.render()).unwrap_or("?".render()),
            children: children.into_iter().map(plan_view_tree).collect(),
        },
        Tree::Leaf { meta: _, node } => ViewTree::Leaf {
            view: node.render(),
        },
    }
}
