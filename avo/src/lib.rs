#![allow(dead_code)]

pub mod operation;
pub mod parser;
pub mod plan;
pub mod store;
pub mod system;

use avo_params::{validate, ParamValidationErrors, ParamValues};
use directories::ProjectDirs;
use rimu::{call, SerdeValueError, Spanned, Value};
use rimu_interop::FromRimu;
use std::{panic, path::PathBuf, str::FromStr};

use crate::{
    operation::{
        EpochError, Operation, OperationEpochsGrouped, OperationGroupApplyError, OperationId,
        OperationTrait, OperationTree, PackageOperation, PackageParams,
    },
    parser::{parse, ParseError, PlanId},
    plan::{IntoPlanActionError, Plan, PlanAction, SetupFunction},
    store::{Store, StoreItemId},
};

pub fn create_store() -> Store {
    let project_dirs =
        ProjectDirs::from("dev", "Avo Org", "Avo").expect("Failed to get project directory");
    let cache_dir = project_dirs.cache_dir();
    Store::new(cache_dir.to_path_buf())
}

#[derive(Debug)]
pub enum PlanError {
    Parse(ParseError),
    Validate(ParamValidationErrors),
    Eval(EvalError),
}

pub async fn plan(
    plan_id: PlanId,
    params: Spanned<ParamValues>,
) -> Result<OperationTree, PlanError> {
    println!("Plan ---");
    let mut store = create_store();
    let operations = plan_recursive(plan_id, params, &mut store).await?;
    let operation = OperationTree::Branch {
        id: None,
        before: vec![],
        after: vec![],
        children: operations,
    };
    println!("Operation: {:?}", operation);
    Ok(operation)
}

pub async fn plan_recursive(
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

#[derive(Debug)]
pub enum EvalError {
    Call(Box<rimu::EvalError>),
    ReturnedNotList,
    InvalidPlanAction(Box<Spanned<IntoPlanActionError>>),
}

fn evaluate(
    setup: Spanned<SetupFunction>,
    params: Spanned<ParamValues>,
) -> Result<Vec<Spanned<PlanAction>>, EvalError> {
    let (setup, setup_span) = setup.take();
    let (params, params_span) = params.take();
    let args = vec![Spanned::new(params.into_rimu(), params_span)];
    let result =
        call(setup_span, setup.0, &args).map_err(|error| EvalError::Call(Box::new(error)))?;
    let (result, _result_span) = result.take();
    let Value::List(items) = result else {
        return Err(EvalError::ReturnedNotList);
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let call = PlanAction::from_rimu_spanned(item)
            .map_err(|error| EvalError::InvalidPlanAction(Box::new(error)))?;
        out.push(call)
    }
    Ok(out)
}

#[derive(Debug)]
pub enum ApplyError {
    Epoch(EpochError),
    OperationGroupApply(OperationGroupApplyError),
}

pub async fn apply(operation: OperationTree) -> Result<OperationEpochsGrouped, ApplyError> {
    println!("Apply ---");
    let epochs = operation.into_epochs().map_err(ApplyError::Epoch)?;
    println!("Epochs: {:?}", epochs);
    let epochs_grouped = epochs.group();
    println!("Epoch grouped: {:?}", epochs_grouped);
    epochs_grouped
        .apply_all()
        .await
        .map_err(ApplyError::OperationGroupApply)?;
    Ok(epochs_grouped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        assert_eq!(true, true);
    }
}
