#![allow(dead_code)]

use avo_operation::{
    Operation, OperationId, OperationTrait, OperationTree, PackageOperation, PackageParams,
};
use avo_params::{validate, ParamValidationErrors, ParamValues};
use avo_store::{Store, StoreItemId};
use rimu::{SerdeValueError, Spanned};
use rimu_interop::FromRimu;
use std::{panic, path::PathBuf, str::FromStr};

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

#[derive(Debug)]
pub enum PlanError {
    Parse(ParseError),
    Validate(ParamValidationErrors),
    Eval(EvalError),
}

pub async fn plan(
    plan_id: PlanId,
    params: Spanned<ParamValues>,
    store: &mut Store,
) -> Result<OperationTree, PlanError> {
    println!("Plan ---");
    let operations = plan_recursive(plan_id, params, store).await?;
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
    param_values: Spanned<ParamValues>,
    store: &mut Store,
) -> Result<Vec<OperationTree>, PlanError> {
    let store_item_id: StoreItemId = plan_id.clone().into();
    let bytes = store
        .read(&store_item_id)
        .await
        .expect("Failed to read from store");
    let code = String::from_utf8(bytes).expect("Failed to convert bytes to string");
    let plan = parse(&code, plan_id).map_err(PlanError::Parse)?;
    let Plan {
        name: _,
        version: _,
        params: param_types,
        setup,
    } = plan.into_inner();
    validate(&param_types, &param_values).map_err(PlanError::Validate)?;
    let plan_actions = evaluate(setup, param_values).map_err(PlanError::Eval)?;
    let mut operations = Vec::with_capacity(plan_actions.len());
    for plan_action in plan_actions {
        operations.push(
            Box::pin(plan_item_to_operation(plan_action, store))
                .await
                .expect("TODO"),
        )
    }
    Ok(operations)
}

#[derive(Debug)]
pub enum FromPlanItemToOperationError {
    MissingParam { name: String },
    ParamValidation(Box<ParamValidationErrors>),
    SerdeValue(Box<SerdeValueError>),
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

    let param_values = param_values.expect("Failed to get params from PlanAction");

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
        let operation = match core_module_id {
            "pkg" => {
                let param_types = PackageOperation::param_types();
                validate(&param_types, &param_values).map_err(|error| {
                    FromPlanItemToOperationError::ParamValidation(Box::new(error))
                })?;
                let package_params: PackageParams = param_values
                    .into_inner()
                    .into_type()
                    .map_err(|error| FromPlanItemToOperationError::SerdeValue(Box::new(error)))?;
                Operation::Package(PackageOperation::new(package_params))
            }
            _ => {
                panic!("Unexpected core module");
            }
        };
        Ok(OperationTree::Leaf {
            id,
            operation,
            before,
            after,
        })
    } else {
        let path = PathBuf::from_str(module.inner()).expect("Failed to convert module to path");
        let plan_id = PlanId::Path(path);
        let children = plan_recursive(plan_id, param_values, store)
            .await
            .expect("TODO");
        Ok(OperationTree::Branch {
            id,
            children,
            before,
            after,
        })
    }
}
