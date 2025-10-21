use std::{collections::HashMap, convert::Infallible};

use crate::block::BlockDefinition;

pub trait OperationTrait {
    fn kind() -> OperationKind;

    type Group: Into<OperationGroup>;
    fn group(ops: impl IntoIterator<Item = Self>) -> Self::Group;

    type Error;
    fn apply(&self) -> Result<(), Self::Error>;
}

pub enum OperationKind {
    Block,
    Package,
    Command,
}

pub enum Operation {
    Block(BlockOperation),
    Package(PackageOperation),
    Command(CommandOperation),
}

pub enum OperationGroup {
    Block(BlockOperationGroup),
    Package(PackageOperationGroup),
    Command(CommandOperationGroup),
}

fn group_by_kind<Ops>(ops: Ops) -> HashMap<OperationKind, OperationGroup> {}

pub struct BlockOperation {
    operations: Vec<Operation>,
}

impl OperationTrait for BlockOperation {
    type Error = Infallible;
    fn union(a: Self, b: Self) -> Self {
        todo!()
    }
    fn apply(&self) -> Self {
        todo!()
    }
}

pub struct PackageOperation {}

impl OperationTrait for PackageOperation {
    type Error = Infallible;
    fn union(a: Self, b: Self) -> Self {
        todo!()
    }
    fn apply(&self) -> Self {
        todo!()
    }
}

pub struct CommandOperation {}

impl OperationTrait for CommandOperation {
    type Error = Infallible;
    fn union(a: Self, b: Self) -> Self {
        todo!()
    }
    fn apply(&self) -> Self {
        todo!()
    }
}

pub struct OperationId(String);

// Flat list of operations.
pub struct OperationEvent {
    id: Option<OperationId>,
    operation: Box<Operation>,
    before: Vec<OperationId>,
    after: Vec<OperationId>,
}

pub struct OperationEvents(Vec<OperationEvents>);

// Operations that happen at the same time (causal "sameness").
pub struct OperationEpoch {
    operations: Vec<Operation>,
}
pub struct OperationEpocs(Vec<OperationEpoch>);

pub struct OperationEpochGrouped {
    operations: HashMap<OperationKind, OperationGroup>,
}
pub struct OperationEpocsGrouped(Vec<OperationEpochGrouped>);
