use cuid2::create_id;
use lusid_causality::CausalityMeta;
use lusid_tree::{FlatTree, FlatTreeMapItem, FlatTreeMappedItem, FlatTreeNode, Tree};

use crate::PlanNodeId;

pub type PlanTree<Node> = Tree<Node, PlanMeta>;
pub type PlanMeta = CausalityMeta<PlanNodeId>;
pub type PlanFlatTree<Node> = FlatTree<Node, PlanMeta>;
pub type PlanFlatTreeNode<Node> = FlatTreeNode<Node, PlanMeta>;
pub type PlanFlatTreeMappedItem<Node> = FlatTreeMappedItem<Node, PlanMeta>;
pub type PlanFlatTreeMapItem<Node> = FlatTreeMapItem<Node, PlanMeta>;

pub fn map_plan_subitems<Node, NextNode, F>(
    node: Option<PlanFlatTreeNode<Node>>,
    map: F,
) -> Option<PlanFlatTreeMapItem<NextNode>>
where
    F: Fn(Node) -> Vec<Tree<NextNode, CausalityMeta<String>>>,
{
    let node = node?;
    Some(node.map(|node| {
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
