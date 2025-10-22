#![allow(dead_code)]

pub mod operation;
pub mod params;
pub mod parser;
pub mod plan;
mod rimu_interop;
pub mod store;
pub mod system;

use std::path::PathBuf;

use directories::ProjectDirs;
use rimu::{call, Spanned, Value};
pub use rimu_interop::FromRimu;

use crate::{
    operation::{Operation, OperationEvent, OperationEventTree, PackageOperation},
    params::ParamValues,
    parser::{parse, BlockId, ParseError},
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
    block_id: BlockId,
    params: Spanned<ParamValues>,
) -> Result<OperationEventTree, PlanError> {
    let cache_dir = PathBuf::new();
    let store_item_id: StoreItemId = block_id.into();
    let store = create_store();
    let bytes = store
        .read(&store_item_id)
        .await
        .expect("Failed to read from store");
    let code = String::from_utf8(bytes).expect("Failed to convert bytes to string");
    let plan = parse(&code, block_id).map_err(PlanError::Parse)?;
    let plan_actions = evaluate(plan, params).map_err(PlanError::Eval)?;
    let operations = Vec::with_capacity(plan_actions.len());
    for plan_action in plan_actions {
        operations.push(plan_item_to_operation_event_tree(plan_action).await?)
    }
    Ok(OperationEventTree::Branch(operations))
}
pub enum FromPlanItemToOperationError {
    MissingParam { name: String },
}

async fn plan_item_to_operation_event_tree(
    plan_action: Spanned<PlanAction>,
) -> Result<OperationEventTree, FromPlanItemToOperationError> {
    let (plan_action, plan_action_span) = plan_action.take();
    let (id, id_span) = plan_action
        .id
        .expect("Failed to get id from PlanAction")
        .take();
    let (params, params_span) = plan_action
        .params
        .expect("Failed to get params from PlanAction")
        .take();
    let before = plan_action.before.iter().map(|v| v.into_inner()).collect();
    let after = plan_action.after.iter().map(|v| v.into_inner()).collect();
    if let Some(core_module_id) = plan_action.core_module_id() {
        let op = match core_module_id {
            "pkg" => {
                let packages = params
                    .get("packages")
                    .expect("Failed to get packages from @core/pkg params");
                let (packages, packages_span) = packages.take();
                Operation::Package(PackageOperation::new(packages))
            }
        };
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
