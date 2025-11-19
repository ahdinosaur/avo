use std::{path::PathBuf, string::FromUtf8Error};

use ludis_operation::{OperationId, OperationTree};
use ludis_params::{validate, ParamValues, ParamsValidationError};
use ludis_store::{Store, StoreError, StoreItemId};
use displaydoc::Display;
use rimu::SerdeValueError;
use rimu::Spanned;
use thiserror::Error;

mod core;
mod eval;
mod id;
mod load;
mod model;

use crate::core::{core_module, is_core_module};
pub use crate::id::PlanId;

use crate::{
    eval::{evaluate, EvalError},
    load::{load, LoadError},
    model::{Plan, PlanAction},
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
    /// Failed to load plan source
    Load(#[from] LoadError),
    /// Parameter validation failed
    Validate(#[from] ParamsValidationError),
    /// Failed to evaluate plan setup
    Eval(#[from] EvalError),
    /// Failed to convert plan item to operation
    PlanActionToOperation(#[from] PlanActionToOperationError),
}

/// Top-level planning routine: load a plan, validate parameters, and evaluate to `OperationTree`.
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

    let plan = load(&code, &plan_id)?;
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
        let op_tree = Box::pin(plan_action_to_operation(plan_action, &plan_id, store)).await?;
        operations.push(op_tree);
    }

    Ok(operations)
}

#[derive(Debug, Error, Display)]
pub enum PlanActionToOperationError {
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

async fn plan_action_to_operation(
    plan_action: Spanned<PlanAction>,
    current_plan_id: &PlanId,
    store: &mut Store,
) -> Result<OperationTree, PlanActionToOperationError> {
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

    if let Some(core_module_id) = is_core_module(module) {
        let operation = core_module(core_module_id, param_values)?;
        Ok(OperationTree::Leaf {
            id,
            operation,
            before,
            after,
        })
    } else {
        let path = PathBuf::from(module.inner());
        let plan_id = current_plan_id.join(path);
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
