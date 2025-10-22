use std::collections::HashMap;

// - Start with a tree of operations
// - Then reduce into a flat list of operations
// - Then section operations into temporal epochs.
// - Then group operations of same kind and epoch into single operation.

pub trait OperationTrait: Into<Operation> {
    fn kind() -> OperationKind;

    type Group: OperationGroupTrait;
    fn group(ops: impl IntoIterator<Item = Self>) -> Self::Group;
}

pub trait OperationGroupTrait: Into<OperationGroup> {
    type Error;
    fn apply(&self) -> Result<(), Self::Error>;
}

pub enum OperationKind {
    Package,
}

pub enum Operation {
    Package(PackageOperation),
}

pub enum OperationGroup {
    Package(PackageOperationGroup),
}

fn group_by_kind<Ops>(ops: Ops) -> HashMap<OperationKind, OperationGroup> {}

pub struct PackageOperation {
    packages: Vec<String>,
}
impl From<PackageOperation> for Operation {
    fn from(value: PackageOperation) -> Self {
        Operation::Package(value)
    }
}

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
        todo!()
    }
}

pub struct OperationId(String);

pub struct OperationEvent {
    id: Option<OperationId>,
    operation: Operation,
    before: Vec<OperationId>,
    after: Vec<OperationId>,
}

// Tree of operations
pub enum OperationEventTree {
    Branch(Vec<OperationEvent>),
    Leaf(OperationEvent),
}

// Flat list of operations.
pub struct OperationEventList(Vec<OperationEvent>);

// Operations that happen at the same time (causal "sameness").
pub struct OperationEpoch {
    operations: Vec<Operation>,
}
pub struct OperationEpocs(Vec<OperationEpoch>);

// Operations at the same time, minimized as a group.
pub struct OperationEpochGrouped {
    operations: HashMap<OperationKind, OperationGroup>,
}
pub struct OperationEpocsGrouped(Vec<OperationEpochGrouped>);
