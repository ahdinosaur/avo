use std::collections::{HashMap, HashSet, VecDeque};

use thiserror::Error;

use crate::tree::{ResourceId, ResourceSpec, ResourceTree};

#[derive(Debug, Error)]
pub enum EpochError {
    #[error("Duplicate id: {0}")]
    DuplicateId(String),

    #[error("Unknown id referenced in 'before': {0}")]
    UnknownBeforeRef(String),

    #[error("Unknown id referenced in 'after': {0}")]
    UnknownAfterRef(String),

    #[error("Cycle detected in dependency graph (remaining nodes: {remaining})")]
    CycleDetected { remaining: usize },
}

/// Compute dependency layers of resource specs (Kahn's algorithm).
/// Returns a list of epochs (layers), each epoch is a Vec<ResourceSpec>.
pub fn compute_epochs(tree: ResourceTree) -> Result<Vec<Vec<ResourceSpec>>, EpochError> {
    #[derive(Debug)]
    struct CollectedLeaf {
        spec: ResourceSpec,
        before: Vec<ResourceId>,
        after: Vec<ResourceId>,
    }

    let mut leaves: Vec<CollectedLeaf> = Vec::new();
    let mut id_to_leaves: HashMap<ResourceId, Vec<usize>> = HashMap::new();
    let mut seen_ids: HashSet<ResourceId> = HashSet::new();

    fn collect_recursive(
        node: ResourceTree,
        ancestor_before: &mut Vec<ResourceId>,
        ancestor_after: &mut Vec<ResourceId>,
        active_branch_ids: &mut Vec<ResourceId>,
        seen_ids: &mut HashSet<ResourceId>,
        id_to_leaves: &mut HashMap<ResourceId, Vec<usize>>,
        leaves: &mut Vec<CollectedLeaf>,
    ) -> Result<(), EpochError> {
        match node {
            ResourceTree::Branch {
                id,
                before,
                after,
                children,
            } => {
                let before_len = ancestor_before.len();
                ancestor_before.extend(before);

                let after_len = ancestor_after.len();
                ancestor_after.extend(after);

                let pushed_branch_id = if let Some(branch_id) = id {
                    if !seen_ids.insert(branch_id.clone()) {
                        return Err(EpochError::DuplicateId(branch_id.0));
                    }
                    id_to_leaves.entry(branch_id.clone()).or_default();
                    active_branch_ids.push(branch_id);
                    true
                } else {
                    false
                };

                for child in children {
                    collect_recursive(
                        child,
                        ancestor_before,
                        ancestor_after,
                        active_branch_ids,
                        seen_ids,
                        id_to_leaves,
                        leaves,
                    )?;
                }

                ancestor_before.truncate(before_len);
                ancestor_after.truncate(after_len);
                if pushed_branch_id {
                    active_branch_ids.pop();
                }
                Ok(())
            }
            ResourceTree::Leaf {
                id,
                resource,
                before,
                after,
            } => {
                let mut effective_before: Vec<ResourceId> = Vec::new();
                effective_before.extend(ancestor_before.iter().cloned());
                effective_before.extend(before);

                let mut effective_after: Vec<ResourceId> = Vec::new();
                effective_after.extend(ancestor_after.iter().cloned());
                effective_after.extend(after);

                let index = leaves.len();
                leaves.push(CollectedLeaf {
                    spec: resource,
                    before: effective_before,
                    after: effective_after,
                });

                for branch_id in active_branch_ids.iter() {
                    if let Some(v) = id_to_leaves.get_mut(branch_id) {
                        v.push(index);
                    }
                }

                if let Some(leaf_id) = id {
                    if !seen_ids.insert(leaf_id.clone()) {
                        return Err(EpochError::DuplicateId(leaf_id.0));
                    }
                    id_to_leaves.insert(leaf_id, vec![index]);
                }
                Ok(())
            }
        }
    }

    let mut ancestor_before: Vec<ResourceId> = Vec::new();
    let mut ancestor_after: Vec<ResourceId> = Vec::new();
    let mut active_branch_ids: Vec<ResourceId> = Vec::new();

    collect_recursive(
        tree,
        &mut ancestor_before,
        &mut ancestor_after,
        &mut active_branch_ids,
        &mut seen_ids,
        &mut id_to_leaves,
        &mut leaves,
    )?;

    // Build adjacency and indegrees (Kahn's algorithm)
    let n = leaves.len();
    let mut outgoing: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut indegree: Vec<usize> = vec![0; n];

    for (i, leaf) in leaves.iter().enumerate() {
        for id in &leaf.before {
            let Some(targets) = id_to_leaves.get(id) else {
                return Err(EpochError::UnknownBeforeRef(id.0.clone()));
            };
            for &j in targets {
                outgoing[j].push(i);
                indegree[i] += 1;
            }
        }
        for id in &leaf.after {
            let Some(targets) = id_to_leaves.get(id) else {
                return Err(EpochError::UnknownAfterRef(id.0.clone()));
            };
            for &j in targets {
                outgoing[i].push(j);
                indegree[j] += 1;
            }
        }
    }

    let mut queue: VecDeque<usize> = indegree
        .iter()
        .enumerate()
        .filter_map(|(i, &d)| (d == 0).then_some(i))
        .collect();

    let mut seen = 0usize;
    let mut epochs: Vec<Vec<ResourceSpec>> = Vec::new();
    let mut indegree_mut = indegree;

    while !queue.is_empty() {
        let current_wave: Vec<usize> = queue.drain(..).collect();
        seen += current_wave.len();

        let mut specs: Vec<ResourceSpec> = Vec::new();
        for i in current_wave.iter().copied() {
            specs.push(leaves[i].spec.clone());
        }
        epochs.push(specs);

        let mut next_wave: Vec<usize> = Vec::new();
        for i in current_wave {
            for &j in &outgoing[i] {
                indegree_mut[j] -= 1;
                if indegree_mut[j] == 0 {
                    next_wave.push(j);
                }
            }
        }
        queue.extend(next_wave);
    }

    if seen != n {
        let remaining = n - seen;
        return Err(EpochError::CycleDetected { remaining });
    }

    Ok(epochs)
}
