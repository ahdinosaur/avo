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

enum PlanError {
    Parse(ParseError),
    Eval(EvalError),
}

pub async fn plan(
    plan_id: PlanId,
    params: Spanned<ParamValues>,
) -> Result<Vec<OperationTree>, PlanError> {
    let cache_dir = PathBuf::new();
    let store_item_id: StoreItemId = plan_id.into();
    let store = create_store();
    let bytes = store
        .read(&store_item_id)
        .await
        .expect("Failed to read from store");
    let code = String::from_utf8(bytes).expect("Failed to convert bytes to string");
    let plan = parse(&code, plan_id).map_err(PlanError::Parse)?;
    let plan_actions = evaluate(plan, params).map_err(PlanError::Eval)?;
    let operations = Vec::with_capacity(plan_actions.len());
    for plan_action in plan_actions {
        operations.push(plan_item_to_operation(plan_action).await?)
    }
    Ok(operations)
}
pub enum FromPlanItemToOperationError {
    MissingParam { name: String },
}

async fn plan_item_to_operation(
    plan_action: Spanned<PlanAction>,
) -> Result<OperationTree, FromPlanItemToOperationError> {
    let (plan_action, plan_action_span) = plan_action.take();
    let id = plan_action
        .id
        .map(|id| OperationId::new(id.into_inner()))

    let before = plan_action
        .before
        .iter()
        .map(|v| v.into_inner())
        .map(OperationId::new)
        .collect();
    let after = plan_action
        .after
        .iter()
        .map(|v| v.into_inner())
        .map(OperationId::new)
        .collect();

    if let Some(core_module_id) = plan_action.core_module_id() {
        let (params, params_span) = plan_action
            .params
            .expect("Failed to get params from PlanAction")
            .take();
        let operation = match core_module_id {
            "pkg" => {
                let packages = params
                    .get("packages")
                    .expect("Failed to get packages from @core/pkg params")
                .into_inner();

                let Value::List(packages) = packages else {
                    panic!("Packages is not a list");
                }
                let packages = packages.into_iter().map(|package| {
                    let Value::String(package) = package else {
                        panic!("Package is not a string");
                    };
                    package
                }).collect();

                Operation::Package(PackageOperation::new(packages))
            }
        };
        Ok(OperationTree::Leaf {
            id,
            operation,
            before,
            after,
        })
    } else {
        let params = plan_action
            .params
            .expect("Failed to get params from PlanAction");
        let module = plan_action.module.into_inner();
        let path = PathBuf::from_str(&module).expect("Failed to convert module to path");
        let plan_id = PlanId::Path(path);
        let children = plan(plan_id, params).await;
        Ok(OperationTree::Branch {
            id,
            children,
            before,
            after,
        })
    }
}

enum EvalError {
    Call(rimu::EvalError),
    ReturnedNotList,
    InvalidPlanAction(Spanned<IntoPlanActionError>),
}

fn evaluate(
    block_definition: Spanned<Plan>,
    params: Spanned<ParamValues>,
) -> Result<Vec<Spanned<PlanAction>>, EvalError> {
    let (block_definition, _block_definition_span) = block_definition.take();
    let (params, params_span) = params.take();
    let args = vec![Spanned::new(params.into_rimu(), params_span)];
    let (setup, setup_span) = block_definition.setup.take();
    let result = call(setup_span, setup.0, &args).map_err(EvalError::Call)?;
    let (result, _result_span) = result.take();
    let Value::List(items) = result else {
        return Err(EvalError::ReturnedNotList);
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let call = PlanAction::from_rimu_spanned(item).map_err(EvalError::InvalidPlanAction)?;
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
