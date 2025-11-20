use std::collections::{HashMap, HashSet, VecDeque};

use displaydoc::Display;
use thiserror::Error;

use crate::{
    ops::package::{PackageOperationAtom, PackageOperationDelta},
    traits::{OperationAtomTrait, OperationDeltaTrait, OperationTrait},
    Operation, OperationId, OperationTree, PackageOperation,
};

#[derive(Debug, Clone, Error, Display)]
pub enum EpochError {
    /// Duplicate id: {0}
    DuplicateId(String),
    /// Unknown id referenced in 'before': {0}
    UnknownBeforeRef(String),
    /// Unknown id referenced in 'after': {0}
    UnknownAfterRef(String),
    /// Cycle detected in dependency graph (remaining nodes: {remaining})
    CycleDetected { remaining: usize },
}

/// Per-epoch operations split by operation type.
#[derive(Debug, Clone)]
pub struct EpochOperations {
    pub package_ops: Vec<PackageOperation>,
}

/// A sequence of epochs in execution order.
#[derive(Debug, Clone)]
pub struct OperationEpocs(pub Vec<EpochOperations>);

impl OperationTree {
    /// Build temporal epochs from dependency constraints represented in the tree.
    ///
    /// Uses Kahn's algorithm to layer nodes (events) by in-degree.
    /// - "before": X.before contains Y => Y must run before X (edge: Y -> X)
    /// - "after":  X.after contains Y => X must run before Y (edge: X -> Y)
    pub fn into_epochs(self) -> Result<OperationEpocs, EpochError> {
        #[derive(Debug)]
        struct CollectedLeaf {
            operation: Operation,
            before: Vec<OperationId>,
            after: Vec<OperationId>,
        }

        let mut leaves: Vec<CollectedLeaf> = Vec::new();
        let mut id_to_leaves: HashMap<OperationId, Vec<usize>> = HashMap::new();
        let mut seen_ids: HashSet<OperationId> = HashSet::new();

        fn collect_recursive(
            node: OperationTree,
            ancestor_before: &mut Vec<OperationId>,
            ancestor_after: &mut Vec<OperationId>,
            active_branch_ids: &mut Vec<OperationId>,
            seen_ids: &mut HashSet<OperationId>,
            id_to_leaves: &mut HashMap<OperationId, Vec<usize>>,
            leaves: &mut Vec<CollectedLeaf>,
        ) -> Result<(), EpochError> {
            match node {
                OperationTree::Branch {
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
                OperationTree::Leaf {
                    id,
                    operation,
                    before,
                    after,
                } => {
                    let mut effective_before: Vec<OperationId> = Vec::new();
                    effective_before.extend(ancestor_before.iter().cloned());
                    effective_before.extend(before);

                    let mut effective_after: Vec<OperationId> = Vec::new();
                    effective_after.extend(ancestor_after.iter().cloned());
                    effective_after.extend(after);

                    let index = leaves.len();
                    leaves.push(CollectedLeaf {
                        operation,
                        before: effective_before,
                        after: effective_after,
                    });

                    // Map branch ids to descendant leaf indices
                    for branch_id in active_branch_ids.iter() {
                        if let Some(v) = id_to_leaves.get_mut(branch_id) {
                            v.push(index);
                        }
                    }

                    // Map leaf ids to this leaf index
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

        let mut ancestor_before: Vec<OperationId> = Vec::new();
        let mut ancestor_after: Vec<OperationId> = Vec::new();
        let mut active_branch_ids: Vec<OperationId> = Vec::new();

        collect_recursive(
            self,
            &mut ancestor_before,
            &mut ancestor_after,
            &mut active_branch_ids,
            &mut seen_ids,
            &mut id_to_leaves,
            &mut leaves,
        )?;

        let n = leaves.len();
        let mut outgoing: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut indegree: Vec<usize> = vec![0; n];

        for (i, leaf) in leaves.iter().enumerate() {
            // "before" creates edges: before_id -> this leaf
            for id in &leaf.before {
                let Some(targets) = id_to_leaves.get(id) else {
                    return Err(EpochError::UnknownBeforeRef(id.0.clone()));
                };
                for &j in targets {
                    outgoing[j].push(i);
                    indegree[i] += 1;
                }
            }
            // "after" creates edges: this leaf -> after_id
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
        let mut epochs: Vec<EpochOperations> = Vec::new();
        let mut indegree_mut = indegree;

        while !queue.is_empty() {
            let current_wave: Vec<usize> = queue.drain(..).collect();
            seen += current_wave.len();

            let mut package_ops: Vec<PackageOperation> = Vec::new();

            for i in current_wave.iter().copied() {
                match &leaves[i].operation {
                    Operation::Package(p) => package_ops.push(p.clone()),
                }
            }

            epochs.push(EpochOperations { package_ops });

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

        Ok(OperationEpocs(epochs))
    }
}

/// Per-epoch atoms by operation type.
#[derive(Debug, Clone)]
pub struct EpochOperationAtoms {
    pub package_atoms: Vec<PackageOperationAtom>,
}

impl EpochOperations {
    pub fn atoms(self) -> EpochOperationAtoms {
        let package_atoms = if self.package_ops.is_empty() {
            Vec::new()
        } else {
            PackageOperation::atoms(self.package_ops)
        };
        EpochOperationAtoms { package_atoms }
    }
}

/// Per-epoch deltas by operation type.
#[derive(Debug, Clone)]
pub struct EpochOperationDeltas {
    pub package_delta: Vec<PackageOperationDelta>,
}

impl EpochOperationAtoms {
    pub fn deltas(self) -> EpochOperationDeltas {
        let mut package_delta: Vec<PackageOperationDelta> = Vec::new();

        for atom in self.package_atoms {
            if let Some(d) = atom.delta() {
                package_delta.push(d);
            }
        }

        EpochOperationDeltas { package_delta }
    }
}

#[derive(Error, Debug, Display)]
pub enum OperationDeltaApplyError {
    /// Package delta apply failed
    Package(<PackageOperationDelta as OperationDeltaTrait>::Error),
}

impl EpochOperationDeltas {
    /// Apply all deltas for this epoch, per operation type.
    pub async fn apply(self) -> Result<(), OperationDeltaApplyError> {
        PackageOperationDelta::apply(self.package_delta)
            .await
            .map_err(OperationDeltaApplyError::Package)?;
        Ok(())
    }
}
