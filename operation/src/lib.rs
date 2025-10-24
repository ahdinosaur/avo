//! - Start with a tree of operations
//! - Then section operations into temporal epochs
//! - Then group operations of same kind and epoch into single operation.

use std::collections::{HashMap, HashSet, VecDeque};

use async_trait::async_trait;
use avo_params::{ParamField, ParamType, ParamTypes};
use indexmap::indexmap;
use rimu::{SourceId, Span, Spanned};
use serde::{de::DeserializeOwned, Deserialize};

#[derive(Debug)]
pub enum ApplyError {
    Epoch(EpochError),
    OperationGroupApply(OperationGroupApplyError),
}

pub async fn apply(operation: OperationTree) -> Result<OperationEpochsGrouped, ApplyError> {
    println!("Apply ---");
    let epochs = operation.into_epochs().map_err(ApplyError::Epoch)?;
    println!("Epochs: {:?}", epochs);
    let epochs_grouped = epochs.group();
    println!("Epoch grouped: {:?}", epochs_grouped);
    epochs_grouped
        .apply_all()
        .await
        .map_err(ApplyError::OperationGroupApply)?;
    Ok(epochs_grouped)
}

/// A single operation type should define:
/// - what "kind" it is
/// - how to group many single operations into a grouped operation
pub trait OperationTrait: Into<Operation> {
    fn kind() -> OperationKind;

    fn param_types() -> Option<Spanned<ParamTypes>>;

    type Params: DeserializeOwned;
    fn new(params: Self::Params) -> Self;

    type Group: OperationGroupTrait;
    fn group(ops: impl IntoIterator<Item = Self>) -> Self::Group;
}

/// A grouped operation knows how to apply itself.
#[async_trait]
pub trait OperationGroupTrait: Into<OperationGroup> {
    type Error;
    async fn apply(&self) -> Result<(), Self::Error>;
}

/// The kind/class of an operation. Used for grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationKind {
    Package,
}

/// The concrete, ungrouped operation enum.
#[derive(Debug, Clone)]
pub enum Operation {
    Package(PackageOperation),
}

/// The grouped operation enum.
#[derive(Debug, Clone)]
pub enum OperationGroup {
    Package(PackageOperationGroup),
}

/// Group a set of concrete operations into grouped ones by kind.
///
/// Returns a map: Kind -> Grouped Operation.
pub fn group_by_kind(
    ops: impl IntoIterator<Item = Operation>,
) -> HashMap<OperationKind, OperationGroup> {
    let mut package_ops: Vec<PackageOperation> = Vec::new();

    for op in ops {
        match op {
            Operation::Package(p) => package_ops.push(p),
        }
    }

    let mut out = HashMap::new();

    let group = PackageOperation::group(package_ops);
    out.insert(OperationKind::Package, OperationGroup::Package(group));

    out
}

/// Package installation.
#[derive(Debug, Clone)]
pub struct PackageOperation {
    packages: Vec<String>,
}
impl From<PackageOperation> for Operation {
    fn from(value: PackageOperation) -> Self {
        Operation::Package(value)
    }
}

/// Grouped package installation.
#[derive(Debug, Clone)]
pub struct PackageOperationGroup {
    packages: Vec<String>,
}
impl From<PackageOperationGroup> for OperationGroup {
    fn from(value: PackageOperationGroup) -> Self {
        OperationGroup::Package(value)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum PackageParams {
    Package { package: String },
    Packages { packages: Vec<String> },
}

impl OperationTrait for PackageOperation {
    fn kind() -> OperationKind {
        OperationKind::Package
    }

    fn param_types() -> Option<Spanned<ParamTypes>> {
        let span = Span::new(SourceId::empty(), 0, 0);
        Some(Spanned::new(
            ParamTypes::Union(vec![
                indexmap! {
                    "package".to_string() =>
                        Spanned::new(ParamField::new(ParamType::String), span.clone())
                },
                indexmap! {
                    "packages".to_string() =>
                        Spanned::new(
                            ParamField::new(
                                ParamType::List {
                                    item: Box::new(Spanned::new(ParamType::String, span.clone())),
                                },
                            ),
                            span.clone(),
                        ),
                },
            ]),
            span,
        ))
    }

    type Params = PackageParams;
    fn new(params: Self::Params) -> Self {
        match params {
            PackageParams::Package { package } => Self {
                packages: vec![package],
            },
            PackageParams::Packages { packages } => Self { packages },
        }
    }

    type Group = PackageOperationGroup;
    fn group(ops: impl IntoIterator<Item = Self>) -> Self::Group {
        let packages = ops
            .into_iter()
            .flat_map(|op| op.packages.into_iter())
            .collect();
        PackageOperationGroup { packages }
    }
}

#[async_trait]
impl OperationGroupTrait for PackageOperationGroup {
    type Error = ();
    async fn apply(&self) -> Result<(), Self::Error> {
        // MVP: just print what we would do.
        if self.packages.is_empty() {
            println!("[pkg] nothing to do");
        } else {
            println!("[pkg] install: {}", self.packages.join(", "));
        }
        Ok(())
    }
}

/// Unified error for applying grouped operations.
#[derive(Debug)]
pub enum OperationGroupApplyError {
    Package(<PackageOperationGroup as OperationGroupTrait>::Error),
}

impl OperationGroup {
    /// Apply this grouped operation.
    pub async fn apply(&self) -> Result<(), OperationGroupApplyError> {
        match self {
            OperationGroup::Package(g) => {
                g.apply().await.map_err(OperationGroupApplyError::Package)
            }
        }
    }
}

/// Identifier for an operation event.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OperationId(String);

impl OperationId {
    pub fn new(operation_id: String) -> Self {
        Self(operation_id)
    }
}

/// A tree of operation events. Both branches and leaves carry identifiers
/// and dependency constraints. Branch-level constraints apply to the entire
/// subtree of that branch.
#[derive(Debug, Clone)]
pub enum OperationTree {
    Branch {
        id: Option<OperationId>,
        before: Vec<OperationId>,
        after: Vec<OperationId>,
        children: Vec<OperationTree>,
    },
    Leaf {
        id: Option<OperationId>,
        operation: Operation,
        before: Vec<OperationId>,
        after: Vec<OperationId>,
    },
}

/// Errors computing epochs (e.g., dependency issues).
#[derive(Debug, Clone)]
pub enum EpochError {
    DuplicateId(String),
    UnknownBeforeRef(String),
    UnknownAfterRef(String),
    CycleDetected { remaining: usize },
}

/// Operations that happen at the same time (causal "sameness").
/// Each epoch can be processed in parallel, but epochs must respect order.
#[derive(Debug, Clone)]
pub struct OperationEpoch {
    pub operations: Vec<Operation>,
}

/// A sequence of epochs in execution order.
#[derive(Debug, Clone)]
pub struct OperationEpocs(pub Vec<OperationEpoch>);

impl OperationTree {
    /// Build temporal epochs from dependency constraints represented in the
    /// tree. Branch-level constraints are inherited by all descendant leaves.
    ///
    /// Uses Kahn's algorithm to layer nodes (events) by in-degree.
    /// - "before": X.before contains Y => Y must run before X (edge: Y -> X)
    /// - "after": X.after contains Y => X must run before Y (edge: X -> Y)
    /// - Branch ids reference the set of all descendant leaf operations.
    pub fn into_epochs(self) -> Result<OperationEpocs, EpochError> {
        // First, collect leaves with inherited constraints, and build a map
        // from every id (branch or leaf) to the set of leaf indices it refers to.
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
                    // Apply branch-level constraints to descendants.
                    let before_len = ancestor_before.len();
                    ancestor_before.extend(before);

                    let after_len = ancestor_after.len();
                    ancestor_after.extend(after);

                    // Track branch id for expansion and uniqueness.
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

                    // Restore stacks.
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
                    // Effective constraints = ancestor constraints + local.
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

                    // Map this leaf under all active branch ids.
                    for branch_id in active_branch_ids.iter() {
                        if let Some(v) = id_to_leaves.get_mut(branch_id) {
                            v.push(index);
                        }
                    }

                    // Uniqueness for leaf id, and map it to this leaf.
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

        // Build adjacency expanding ids to the set of leaf indices they denote.
        for (i, leaf) in leaves.iter().enumerate() {
            // "before": Y in before => Y -> i
            for id in &leaf.before {
                let Some(targets) = id_to_leaves.get(id) else {
                    return Err(EpochError::UnknownBeforeRef(id.0.clone()));
                };
                for &j in targets {
                    outgoing[j].push(i);
                    indegree[i] += 1;
                }
            }
            // "after": Y in after => i -> Y
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

        // Kahn's layering: collect zero in-degree nodes per wave (epoch)
        let mut queue: VecDeque<usize> = indegree
            .iter()
            .enumerate()
            .filter_map(|(i, &d)| (d == 0).then_some(i))
            .collect();

        let mut seen = 0usize;
        let mut epochs: Vec<OperationEpoch> = Vec::new();
        let mut indegree_mut = indegree;

        while !queue.is_empty() {
            // One wave/epoch = everything zero-indegree at this step.
            let current_wave: Vec<usize> = queue.drain(..).collect();
            seen += current_wave.len();

            // Deterministic order within epoch by original index.
            let mut ops: Vec<Operation> = Vec::with_capacity(current_wave.len());
            for i in current_wave.iter().copied() {
                ops.push(leaves[i].operation.clone());
            }
            epochs.push(OperationEpoch { operations: ops });

            // Remove edges of current wave; track new zeros for next wave.
            let mut next_wave: Vec<usize> = Vec::new();
            for i in current_wave {
                for &j in &outgoing[i] {
                    indegree_mut[j] -= 1;
                    if indegree_mut[j] == 0 {
                        next_wave.push(j);
                    }
                }
            }
            // Prepare next wave
            queue.extend(next_wave);
        }

        if seen != n {
            // Cycle detected: some nodes remain with indegree > 0
            let remaining = n - seen;
            return Err(EpochError::CycleDetected { remaining });
        }

        Ok(OperationEpocs(epochs))
    }
}

impl OperationEpoch {
    /// Step 4 (per-epoch): group operations by kind and reduce to grouped ops.
    pub fn group(self) -> OperationEpochGrouped {
        let grouped = group_by_kind(self.operations);
        OperationEpochGrouped {
            operations: grouped,
        }
    }
}

impl OperationEpocs {
    /// Step 4 (all epochs): group by kind within each epoch.
    pub fn group(self) -> OperationEpochsGrouped {
        let grouped_epochs = self
            .0
            .into_iter()
            .map(OperationEpoch::group)
            .collect::<Vec<_>>();
        OperationEpochsGrouped(grouped_epochs)
    }
}

/// Operations at the same time, minimized as groups by kind.
#[derive(Debug, Clone)]
pub struct OperationEpochGrouped {
    pub operations: HashMap<OperationKind, OperationGroup>,
}

/// Grouped epochs in execution order.
#[derive(Debug, Clone)]
pub struct OperationEpochsGrouped(pub Vec<OperationEpochGrouped>);

impl OperationEpochsGrouped {
    /// Apply all grouped operations epoch-by-epoch (in order).
    /// Stops at first error.
    pub async fn apply_all(&self) -> Result<(), OperationGroupApplyError> {
        for epoch in &self.0 {
            // Order among kinds within an epoch is unspecified by design.
            for group in epoch.operations.values() {
                group.apply().await?;
            }
        }
        Ok(())
    }
}
