#![allow(dead_code)]

pub mod operation;
pub mod params;
pub mod parser;
pub mod plan;
mod rimu_interop;
pub mod store;
pub mod system;

use std::{panic, path::PathBuf, str::FromStr};

use directories::ProjectDirs;
use rimu::{call, Spanned, Value};
pub use rimu_interop::FromRimu;

use crate::{
    operation::{Operation, OperationId, OperationTree, PackageOperation},
    params::ParamValues,
    parser::{parse, ParseError, PlanId},
    plan::{IntoPlanActionError, Plan, PlanAction},
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
    Eval(EvalError),
}

pub async fn plan(
    plan_id: PlanId,
    params: Spanned<ParamValues>,
) -> Result<Vec<OperationTree>, PlanError> {
    let store_item_id: StoreItemId = plan_id.clone().into();
    let store = create_store();
    let bytes = store
        .read(&store_item_id)
        .await
        .expect("Failed to read from store");
    let code = String::from_utf8(bytes).expect("Failed to convert bytes to string");
    let plan = parse(&code, plan_id).map_err(PlanError::Parse)?;
    let plan_actions = evaluate(plan, params).map_err(PlanError::Eval)?;
    let mut operations = Vec::with_capacity(plan_actions.len());
    for plan_action in plan_actions {
        operations.push(
            Box::pin(plan_item_to_operation(plan_action))
                .await
                .expect("TODO"),
        )
    }
    Ok(operations)
}

#[derive(Debug)]
pub enum FromPlanItemToOperationError {
    MissingParam { name: String },
}

async fn plan_item_to_operation(
    plan_action: Spanned<PlanAction>,
) -> Result<OperationTree, FromPlanItemToOperationError> {
    let (plan_action, _plan_action_span) = plan_action.take();

    let PlanAction {
        id,
        ref module,
        params,
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
        let (params, _params_span) = params.expect("Failed to get params from PlanAction").take();
        let operation = match core_module_id {
            "pkg" => {
                let packages = params
                    .get("packages")
                    .expect("Failed to get packages from @core/pkg params")
                    .clone()
                    .into_inner();

                let Value::List(packages) = packages else {
                    panic!("Packages is not a list");
                };
                let packages = packages
                    .into_iter()
                    .map(|package| {
                        let Value::String(package) = package.into_inner() else {
                            panic!("Package is not a string");
                        };
                        package
                    })
                    .collect();

                Operation::Package(PackageOperation::new(packages))
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
        let params = params.expect("Failed to get params from PlanAction");
        let module = plan_action.module.into_inner();
        let path = PathBuf::from_str(&module).expect("Failed to convert module to path");
        let plan_id = PlanId::Path(path);
        let children = plan(plan_id, params).await.expect("TODO");
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
    block_definition: Spanned<Plan>,
    params: Spanned<ParamValues>,
) -> Result<Vec<Spanned<PlanAction>>, EvalError> {
    let (block_definition, _block_definition_span) = block_definition.take();
    let (params, params_span) = params.take();
    let args = vec![Spanned::new(params.into_rimu(), params_span)];
    let (setup, setup_span) = block_definition.setup.take();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        assert_eq!(true, true);
    }
}
