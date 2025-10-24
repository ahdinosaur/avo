use std::{path::PathBuf, string::FromUtf8Error};

use avo_operation::{
    Operation, OperationId, OperationTrait, OperationTree, PackageOperation, PackageParams,
};
use avo_params::{validate, ParamValues, ParamsValidationError};
use avo_store::{Store, StoreError, StoreItemId};
use displaydoc::Display;
use rimu::SerdeValueError;
use rimu::Spanned;
use rimu_interop::FromRimu;
use thiserror::Error;

mod eval;
mod id;
mod parse;
mod plan;

pub use crate::id::PlanId;

use crate::{
    eval::{evaluate, EvalError},
    parse::{parse, ParseError},
    plan::{Plan, PlanAction},
};

#[derive(Debug, Error, Display)]
pub enum PlanError {
    /// Failed to read plan source from store for id {id:?}
    StoreRead {
        id: StoreItemId,
        #[source]
        source: StoreError,
    },
    /// Failed to decode plan source as UTF-8
    InvalidUtf8(#[from] FromUtf8Error),
    /// Failed to parse plan source
    Parse(#[from] ParseError),
    /// Parameter validation failed
    Validate(#[from] ParamsValidationError),
    /// Failed to evaluate plan setup
    Eval(#[from] EvalError),
    /// Failed to convert plan item to operation
    PlanItemToOperation(#[from] FromPlanItemToOperationError),
}

/// Top-level planning routine: read & parse a plan, validate parameters and
/// assemble `OperationTree`.
pub async fn plan(
    plan_id: PlanId,
    param_values: Option<Spanned<ParamValues>>,
    store: &mut Store,
) -> Result<OperationTree, PlanError> {
    println!("Plan ---");

    let operations = plan_recursive(plan_id, param_values.as_ref(), store).await?;
    let operation = OperationTree::Branch {
        id: None,
        before: vec![],
        after: vec![],
        children: operations,
    };

    println!("Operation: {:?}", operation);
    Ok(operation)
}

async fn plan_recursive(
    plan_id: PlanId,
    param_values: Option<&Spanned<ParamValues>>,
    store: &mut Store,
) -> Result<Vec<OperationTree>, PlanError> {
    let store_item_id: StoreItemId = plan_id.clone().into();

    let bytes = store
        .read(&store_item_id)
        .await
        .map_err(|source| PlanError::StoreRead {
            id: store_item_id.clone(),
            source,
        })?;

    let code = String::from_utf8(bytes)?;

    let plan = parse(&code, plan_id)?;
    let Plan {
        name: _,
        version: _,
        params: param_types,
        setup,
    } = plan.into_inner();

    validate(param_types.as_ref(), param_values)?;
    let plan_actions = evaluate(setup, param_values.cloned())?;

    let mut operations = Vec::with_capacity(plan_actions.len());
    for plan_action in plan_actions {
        let op_tree = Box::pin(plan_item_to_operation(plan_action, store)).await?;
        operations.push(op_tree);
    }

    Ok(operations)
}

#[derive(Debug, Error, Display)]
pub enum FromPlanItemToOperationError {
    /// Missing required parameters in plan action
    MissingParams,
    /// Parameters validation for operation failed
    ParamsValidation(#[from] ParamsValidationError),
    /// Failed to convert parameter values to operation params
    SerdeValue(#[from] SerdeValueError),
    /// Unsupported core module id "{id}"
    UnsupportedCoreModuleId { id: String },
    /// Failed to compute subtree for nested plan
    PlanSubtree(#[from] Box<PlanError>),
}

async fn plan_item_to_operation(
    plan_action: Spanned<PlanAction>,
    store: &mut Store,
) -> Result<OperationTree, FromPlanItemToOperationError> {
    let (plan_action, _plan_action_span) = plan_action.take();
    let PlanAction {
        id,
        ref module,
        params: param_values,
        before,
        after,
    } = plan_action;

    let id = id.map(|id| OperationId::new(id.into_inner()));
    let before = before
        .into_iter()
        .map(|v| v.into_inner())
        .map(OperationId::new)
        .collect();
    let after = after
        .into_iter()
        .map(|v| v.into_inner())
        .map(OperationId::new)
        .collect();

    if let Some(core_module_id) = PlanAction::is_core_module(module) {
        let param_values = param_values.ok_or(FromPlanItemToOperationError::MissingParams)?;
        let operation = match core_module_id {
            "pkg" => {
                let param_types = PackageOperation::param_types();
                validate(param_types.as_ref(), Some(&param_values))
                    .map_err(FromPlanItemToOperationError::from)?;
                let package_params: PackageParams = param_values
                    .into_inner()
                    .into_type()
                    .map_err(FromPlanItemToOperationError::from)?;
                Operation::Package(PackageOperation::new(package_params))
            }
            other => {
                return Err(FromPlanItemToOperationError::UnsupportedCoreModuleId {
                    id: other.to_string(),
                });
            }
        };

        Ok(OperationTree::Leaf {
            id,
            operation,
            before,
            after,
        })
    } else {
        let path = PathBuf::from(module.inner());
        let plan_id = PlanId::Path(path);
        let children = plan_recursive(plan_id, param_values.as_ref(), store)
            .await
            .map_err(Box::new)?;
        Ok(OperationTree::Branch {
            id,
            children,
            before,
            after,
        })
    }
}
