use lusid_view::{Line, Tree, ViewNode};
use serde::{Deserialize, Serialize};
use std::fmt::Display;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ResourcesTree(pub ViewNode);

impl Display for ResourcesTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ResourceStatesTree(pub ViewNode);

impl Display for ResourceStatesTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ResourceChangesTree(pub ViewNode);

impl Display for ResourceChangesTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationsTree(pub Tree);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationsEpochs(pub Vec<Vec<Line>>);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperationsApply {
    Start,
    OperationStart {
        operation_id: String,
    },
    OperationStdout {
        operation_id: String,
        stdout: String,
    },
    OperationStderr {
        operation_id: String,
        stderr: String,
    },
    OperationComplete {
        operation_id: String,
    },
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Error {
    debug: String,
    display: String,
}
