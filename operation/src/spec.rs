use async_trait::async_trait;
use ludis_params::ParamTypes;
use rimu::Spanned;
use serde::de::DeserializeOwned;

use crate::Operation;

/// Defines how a specific operation type behaves within the system.
///
/// An operation spec must describe:
/// - Its unique identifier (`ID`)
/// - The schema of its parameters
/// - How to build the concrete operation
/// - How to break operations into atomic units
/// - How to compute, apply, and handle changes
#[async_trait]
pub trait OperationSpec {
    /// Unique identifier for this operation type.
    const ID: &str;

    /// Parameter schema for this operation type, or `None` if not applicable.
    fn param_types() -> Option<Spanned<ParamTypes>>;

    /// Input parameters for constructing an operation.
    type Params: DeserializeOwned;

    /// The concrete operation produced by this spec.
    type Operation: Into<Operation>;

    /// Build a new operation from the given parameters.
    fn operation(params: Self::Params) -> Self::Operation;

    /// A minimal, indivisible unit of work for this operation type.
    type Atom;

    /// Split a collection of operations into atomic units.
    fn atoms(ops: impl IntoIterator<Item = Self::Operation>) -> Vec<Self::Atom>;

    /// Represents a concrete change to be applied.
    type Delta;

    /// Get the delta for a given atom, to change from the current state to the desired state.
    ///
    /// Returns:
    /// - `Some(delta)` if a change is needed
    /// - `None` if the state is already correct
    async fn delta(atom: Self::Atom) -> Option<Self::Delta>;

    /// Error type returned when applying deltas fails.
    type ApplyError;

    /// Apply all given deltas for this operation type.
    async fn apply(deltas: Vec<Self::Delta>) -> Result<(), Self::ApplyError>;
}
