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

pub use self::spec::OperationSpec;

use displaydoc::Display;
use thiserror::Error;

use crate::{
    epoch::{tree_to_epochs, EpochError},
    ops::package::{PackageOperation, PackageOperationAtom, PackageOperationDelta, PackageSpec},
};

#[derive(Debug, Error, Display)]
pub enum ApplyError {
    /// Failed to compute epochs
    Epoch(#[from] EpochError),
    /// Failed querying operation deltas
    Delta(#[from] OperationDeltaError),
    /// Failed applying operation deltas
    Apply(#[from] OperationApplyError),
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
    let operations_by_epoch = tree_to_epochs(operation)?;
    let atoms_by_epoch = operations_by_epoch.map(|epoch_operations| epoch_operations.atoms());
    let deltas_by_epoch = atoms_by_epoch
        .try_map_async(|epoch_atoms| async move { epoch_atoms.deltas().await })
        .await?;

    deltas_by_epoch
        .try_each_async(|epoch_deltas| async move { epoch_deltas.apply().await })
        .await?;

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

/// Per-epoch operations split by operation type.
#[derive(Debug, Clone)]
pub struct EpochOperations {
    pub package_ops: Vec<PackageOperation>,
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
            PackageSpec::atoms(self.package_ops)
        };
        EpochOperationAtoms { package_atoms }
    }
}

/// Per-epoch deltas by operation type.
#[derive(Debug, Clone)]
pub struct EpochOperationDeltas {
    pub package_deltas: Vec<PackageOperationDelta>,
}

#[derive(Error, Debug, Display)]
pub enum OperationDeltaError {
    /// Package delta query failed
    Package(<PackageSpec as OperationSpec>::DeltaError),
}

impl EpochOperationAtoms {
    pub async fn deltas(self) -> Result<EpochOperationDeltas, OperationDeltaError> {
        let mut package_deltas: Vec<PackageOperationDelta> = Vec::new();

        for atom in self.package_atoms {
            if let Some(d) = PackageSpec::delta(atom)
                .await
                .map_err(OperationDeltaError::Package)?
            {
                package_deltas.push(d);
            }
        }

        Ok(EpochOperationDeltas { package_deltas })
    }
}

#[derive(Error, Debug, Display)]
pub enum OperationApplyError {
    /// Package delta apply failed
    Package(<PackageSpec as OperationSpec>::ApplyError),
}

impl EpochOperationDeltas {
    /// Apply all deltas for this epoch, per operation type.
    pub async fn apply(self) -> Result<(), OperationApplyError> {
        PackageSpec::apply(self.package_deltas)
            .await
            .map_err(OperationApplyError::Package)?;
        Ok(())
    }
}
