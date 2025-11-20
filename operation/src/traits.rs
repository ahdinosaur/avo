use async_trait::async_trait;
use ludis_params::ParamTypes;
use rimu::Spanned;
use serde::de::DeserializeOwned;

use crate::{Operation, OperationKind};

/// An operation type must describe:
/// - its kind
/// - the schema for its parameters
/// - how to construct a concrete operation from parameters
/// - how to split a set of operations of this type into atoms
pub trait OperationTrait: Into<Operation> {
    fn kind() -> OperationKind;
    fn param_types() -> Option<Spanned<ParamTypes>>;
    type Params: DeserializeOwned;
    fn new(params: Self::Params) -> Self;

    type Atom: OperationAtomTrait;

    /// Split a set of operations of this type into atoms.
    fn atoms(ops: impl IntoIterator<Item = Self>) -> Vec<Self::Atom>;
}

/// A single, minimal unit of change/query for an operation type.
///
/// It can derive a Delta (if anything needs to change).
pub trait OperationAtomTrait {
    type Delta: OperationDeltaTrait;

    /// Compute the delta for this atom.
    ///
    /// Returns:
    /// - Some(delta) if something needs to change
    /// - None if the current state already matches desired state
    fn delta(&self) -> Option<Self::Delta>;
}

/// A delta describes concrete changes to apply for an operation type.
/// The `apply` function receives a batch of deltas of the same type.
#[async_trait]
pub trait OperationDeltaTrait: Send + Sync {
    type Error: Send + Sync;

    async fn apply(deltas: Vec<Self>) -> Result<(), Self::Error>
    where
        Self: Sized;
}
