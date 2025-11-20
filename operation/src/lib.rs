//!
//! Operation planning and application pipeline (atoms → deltas).
//!
//! Flow:
//! - Evaluate Rimu to an OperationTree
//! - Compute epochs from dependency constraints
//! - For each epoch:
//!   - Split operations into atoms
//!   - Compute per-atom deltas
//!   - Apply deltas per operation type (batched)
//!
//! Each concrete operation defines:
//! - how to build atoms from operations (OperationTrait::atoms)
//! - how an atom derives a delta (OperationAtomTrait::delta)
//! - how to apply a batch of deltas (OperationDeltaTrait::apply)

mod epoch;
pub mod ops;
mod spec;

use displaydoc::Display;
use thiserror::Error;

use crate::{
    epoch::{EpochError, OperationDeltaApplyError},
    ops::package::PackageOperation,
};

#[derive(Debug, Error, Display)]
pub enum ApplyError {
    /// Failed to compute epochs
    Epoch(#[from] EpochError),
    /// Failed applying operation deltas
    DeltaApply(#[from] OperationDeltaApplyError),
}

/// Apply a fully planned operation tree using the atoms → deltas pipeline.
///
/// - Build epochs from the operation tree
/// - For each epoch:
///   - Split operations into atoms
///   - Compute per-atom deltas
///   - Apply batched deltas per operation type
#[tracing::instrument(skip_all)]
pub async fn apply(operation: OperationTree) -> Result<(), ApplyError> {
    tracing::info!("Applying operation tree (atoms → deltas)");
    let epochs = operation.into_epochs()?;

    for epoch_ops in epochs.0 {
        let atoms = epoch_ops.atoms();
        let deltas = atoms.deltas().await;
        deltas.apply().await?;
    }

    Ok(())
}

/// Enum of all operation types.
#[derive(Debug, Clone)]
pub enum Operation {
    Package(PackageOperation),
}

/// Identifier for an operation event.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OperationId(String);

impl OperationId {
    pub fn new(operation_id: String) -> Self {
        Self(operation_id)
    }
}

/// A tree of operation events.
/// Branch-level constraints are inherited by all descendant leaves.
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
