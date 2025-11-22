use async_trait::async_trait;
use std::fmt::Debug;
use thiserror::Error;

pub mod ops;

use crate::ops::apt::{AptOperation, AptOperationType};

/// OperationType specifies how to merge and apply a concrete Operation type.
///
/// Operations are the results of ResourceChanges and are executed per epoch.
/// Each type decides how to merge same-type operations and how to apply them.
#[async_trait]
pub trait OperationType {
    type Operation: Debug + Clone + Send + 'static;
    type ApplyError: Debug + Send + 'static;

    /// Merge a set of operations of this type within the same epoch.
    /// Implementations should coalesce operations to a minimal set.
    fn merge(ops: Vec<Self::Operation>) -> Vec<Self::Operation>;

    /// Apply the merged operations of this type for an epoch.
    async fn apply(ops: Vec<Self::Operation>) -> Result<(), Self::ApplyError>;
}

/// An operation produced by the resource layer.
#[derive(Debug, Clone)]
pub enum Operation {
    Apt(AptOperation),
}

#[derive(Error, Debug)]
pub enum OperationApplyError {
    #[error("apt operation failed: {0:?}")]
    Apt(<AptOperationType as OperationType>::ApplyError),
}

/// Merge a single epoch's operations by type.
pub fn merge_operations(ops: &[Operation]) -> Vec<Operation> {
    // Partition by type
    let mut apt_ops: Vec<AptOperation> = Vec::new();

    for op in ops {
        match op {
            Operation::Apt(o) => apt_ops.push(o.clone()),
        }
    }

    // Merge per type and wrap back into the enum
    let mut merged: Vec<Operation> = Vec::new();

    if !apt_ops.is_empty() {
        let m = AptOperationType::merge(apt_ops);
        merged.extend(m.into_iter().map(Operation::Apt));
    }

    merged
}

/// Apply a single epoch's operations (already merged).
pub async fn apply_operations(ops: &[Operation]) -> Result<(), OperationApplyError> {
    let mut apt_ops: Vec<AptOperation> = Vec::new();

    for op in ops {
        match op {
            Operation::Apt(o) => apt_ops.push(o.clone()),
        }
    }

    if !apt_ops.is_empty() {
        AptOperationType::apply(apt_ops)
            .await
            .map_err(OperationApplyError::Apt)?;
    }

    Ok(())
}

/// Merge operations by epoch.
pub fn merge_operations_by_epoch(layers: &[Vec<Operation>]) -> Vec<Vec<Operation>> {
    layers
        .iter()
        .map(|ops| merge_operations(ops))
        .collect::<Vec<_>>()
}

/// Apply operations by epoch, in order.
pub async fn apply_by_epoch(layers: &[Vec<Operation>]) -> Result<(), OperationApplyError> {
    for ops in layers {
        apply_operations(ops).await?;
    }
    Ok(())
}
