//! - Start with a tree of operations
//! - Then reduce into a flat list of operations
//! - Then section operations into temporal epochs
//! - Then group operations of same kind and epoch into single operation.

use std::{
    collections::{HashMap, VecDeque},
    iter::once,
};

/// A single operation type should define:
/// - what "kind" it is
/// - how to group many single operations into a grouped operation
pub trait OperationTrait: Into<Operation> {
    fn kind() -> OperationKind;

    type Group: OperationGroupTrait;
    fn group(ops: impl IntoIterator<Item = Self>) -> Self::Group;
}

/// A grouped operation knows how to apply itself.
pub trait OperationGroupTrait: Into<OperationGroup> {
    type Error;
    fn apply(&self) -> Result<(), Self::Error>;
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

impl OperationTrait for PackageOperation {
    fn kind() -> OperationKind {
        OperationKind::Package
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

impl OperationGroupTrait for PackageOperationGroup {
    type Error = ();
    fn apply(&self) -> Result<(), Self::Error> {
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
    pub fn apply(&self) -> Result<(), OperationGroupApplyError> {
        match self {
            OperationGroup::Package(g) => g.apply().map_err(OperationGroupApplyError::Package),
        }
    }
}

/// Identifier for an operation event.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OperationId(String);

/// A single scheduled operation event with optional ID and
/// explicit "before" and "after" constraints.
#[derive(Debug, Clone)]
pub struct OperationEvent {
    pub id: Option<OperationId>,
    pub operation: Operation,
    pub before: Vec<OperationId>,
    pub after: Vec<OperationId>,
}

/// A lightweight "tree" root. This is where you start:
/// - put many events in a branch, or
/// - a single event as a leaf.
#[derive(Debug, Clone)]
pub enum OperationEventTree {
    Branch(Vec<OperationEventTree>),
    Leaf(OperationEvent),
}

impl OperationEventTree {
    /// Step 1: Flatten the tree into an iterator.
    pub fn into_list(self) -> OperationEventList {
        fn tree_to_iter(tree: OperationEventTree) -> Box<dyn Iterator<Item = OperationEvent>> {
            match tree {
                OperationEventTree::Branch(branch) => {
                    Box::new(branch.into_iter().flat_map(tree_to_iter))
                }
                OperationEventTree::Leaf(op) => Box::new(once(op)),
            }
        }

        let ops = tree_to_iter(self);
        OperationEventList(ops.collect())
    }
}

/// A flat list of operation events (with dependencies).
#[derive(Debug, Clone)]
pub struct OperationEventList(pub Vec<OperationEvent>);

/// Errors computing epochs (e.g., dependency issues).
#[derive(Debug, Clone)]
pub enum EpochError {
    DuplicateId(String),
    UnknownBeforeRef(String),
    UnknownAfterRef(String),
    CycleDetected { remaining: usize },
}

impl OperationEventList {
    /// Step 2 -> 3: Build temporal epochs from dependency constraints.
    ///
    /// Uses Kahn's algorithm to layer nodes (events) by in-degree.
    /// - "before": X.before contains Y => Y must run before X (edge: Y -> X)
    /// - "after": X.after contains Y => X must run before Y (edge: X -> Y)
    pub fn into_epochs(self) -> Result<OperationEpocs, EpochError> {
        let events = self.0;
        let n = events.len();

        // Map id -> index, ensure uniqueness
        let mut id_to_index: HashMap<OperationId, usize> = HashMap::new();
        for (i, ev) in events.iter().enumerate() {
            if let Some(id) = &ev.id
                && id_to_index.insert(id.clone(), i).is_some()
            {
                return Err(EpochError::DuplicateId(id.0.clone()));
            }
        }

        // Build adjacency and in-degrees
        let mut outgoing: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut indeg: Vec<usize> = vec![0; n];

        for (i, ev) in events.iter().enumerate() {
            // "before": listed ids must happen before 'i'. Edge: before_id -> i
            for b in &ev.before {
                let Some(&src) = id_to_index.get(b) else {
                    return Err(EpochError::UnknownBeforeRef(b.0.clone()));
                };
                outgoing[src].push(i);
                indeg[i] += 1;
            }
            // "after": listed ids must happen after 'i'. Edge: i -> after_id
            for a in &ev.after {
                let Some(&dst) = id_to_index.get(a) else {
                    return Err(EpochError::UnknownAfterRef(a.0.clone()));
                };
                outgoing[i].push(dst);
                indeg[dst] += 1;
            }
        }

        // Kahn's layering: collect zero in-degree nodes per wave (epoch)
        let mut queue: VecDeque<usize> = indeg
            .iter()
            .enumerate()
            .filter_map(|(i, &d)| (d == 0).then_some(i))
            .collect();

        let mut seen = 0usize;
        let mut epochs: Vec<OperationEpoch> = Vec::new();
        let mut indeg_mut = indeg;

        while !queue.is_empty() {
            // One wave/epoch = everything zero-indegree at this step.
            let current_wave: Vec<usize> = queue.drain(..).collect();
            seen += current_wave.len();

            // Deterministic order within epoch by original index.
            let mut ops: Vec<Operation> = Vec::with_capacity(current_wave.len());
            for i in current_wave.iter().copied() {
                ops.push(events[i].operation.clone());
            }
            epochs.push(OperationEpoch { operations: ops });

            // Remove edges of current wave; track new zeros for next wave.
            let mut next_wave: Vec<usize> = Vec::new();
            for i in current_wave {
                for &j in &outgoing[i] {
                    indeg_mut[j] -= 1;
                    if indeg_mut[j] == 0 {
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

/// Operations that happen at the same time (causal "sameness").
/// Each epoch can be processed in parallel, but epochs must respect order.
#[derive(Debug, Clone)]
pub struct OperationEpoch {
    pub operations: Vec<Operation>,
}

/// A sequence of epochs in execution order.
#[derive(Debug, Clone)]
pub struct OperationEpocs(pub Vec<OperationEpoch>);

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
    pub fn apply_all(&self) -> Result<(), OperationGroupApplyError> {
        for epoch in &self.0 {
            // Order among kinds within an epoch is unspecified by design.
            for group in epoch.operations.values() {
                group.apply()?;
            }
        }
        Ok(())
    }
}
