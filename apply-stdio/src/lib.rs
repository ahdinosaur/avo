use lusid_view::Tree;
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
pub struct OperationsEpochs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperationApply {
    Start,
    Progress,
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Error {
    display: String,
}
