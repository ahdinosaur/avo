use async_trait::async_trait;
use lusid_view::Render;
use std::fmt::{Debug, Display};
use thiserror::Error;

pub mod operations;

use crate::operations::apt::{Apt, AptOperation};

/// OperationType specifies how to merge and apply a concrete Operation type.
///
/// Operations are the results of ResourceChanges and are executed per epoch.
/// Each type decides how to merge same-type operations and how to apply them.
#[async_trait]
pub trait OperationType {
    type Operation: Render;

    /// Merge a set of operations of this type within the same epoch.
    /// Implementations should coalesce operations to a minimal set.
    fn merge(operations: Vec<Self::Operation>) -> Vec<Self::Operation>;

    type ApplyError;

    /// Apply the merged operations of this type for an epoch.
    async fn apply(operations: Vec<Self::Operation>) -> Result<(), Self::ApplyError>;
}

#[derive(Debug, Clone)]
pub enum Operation {
    Apt(AptOperation),
}

impl Display for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use Operation::*;
        match self {
            Apt(apt) => Display::fmt(apt, f),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OperationsByType {
    apt: Vec<AptOperation>,
}

/// Merge a set of operations by type.
pub fn partition_by_type(operations: Vec<Operation>) -> OperationsByType {
    let mut apt: Vec<AptOperation> = Vec::new();
    for operation in operations {
        match operation {
            Operation::Apt(op) => apt.push(op),
        }
    }
    OperationsByType { apt }
}

/// Merge a set of operations by type.
pub fn merge_operations(operations: OperationsByType) -> OperationsByType {
    let OperationsByType { apt } = operations;

    let apt = Apt::merge(apt);

    OperationsByType { apt }
}

#[derive(Error, Debug)]
pub enum OperationApplyError {
    #[error("apt operation failed: {0:?}")]
    Apt(<Apt as OperationType>::ApplyError),
}

/// Apply a set of operations by type
pub async fn apply_operations(operations: OperationsByType) -> Result<(), OperationApplyError> {
    let OperationsByType { apt } = operations;

    Apt::apply(apt).await.map_err(OperationApplyError::Apt)?;

    Ok(())
}
