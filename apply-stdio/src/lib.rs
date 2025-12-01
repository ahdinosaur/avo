use lusid_view::{Line, Tree};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceParamsTree(pub Tree);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcesTree(pub Tree);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceStatesTree(pub Tree);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceChangesTree(pub Tree);

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
