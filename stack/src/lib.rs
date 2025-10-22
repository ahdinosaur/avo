#![allow(dead_code)]

pub mod block;
pub mod operation;
pub mod params;
pub mod parser;
mod rimu_interop;
pub mod store;
pub mod system;

use std::path::PathBuf;

use directories::ProjectDirs;
use rimu::{call, Spanned, Value};
pub use rimu_interop::FromRimu;

use crate::{
    block::{BlockCallRef, BlockDefinition, IntoBlockCallRefError},
    operation::OperationEventTree,
    params::ParamValues,
    parser::{parse, BlockId, ParseError},
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
    let block_definition = parse(&code, block_id).map_err(PlanError::Parse)?;
    let block_call_refs = evaluate(block_definition, params).map_err(PlanError::Eval)?;
    Ok(OperationEventTree::Leaf(()))
}

enum EvalError {
    SetupReturnedNotList,
    SetupCall(rimu::EvalError),
    SetupListItemNotBlockCallRef(Spanned<IntoBlockCallRefError>),
}

fn evaluate(
    block_definition: Spanned<BlockDefinition>,
    params: Spanned<ParamValues>,
) -> Result<Vec<Spanned<BlockCallRef>>, EvalError> {
    let (block_definition, _block_definition_span) = block_definition.take();
    let (params, params_span) = params.take();
    let args = vec![Spanned::new(params.into_rimu(), params_span)];
    let (setup, setup_span) = block_definition.setup.take();
    let result = call(setup_span, setup.0, &args).map_err(EvalError::SetupCall)?;
    let (result, _result_span) = result.take();
    let Value::List(items) = result else {
        return Err(EvalError::SetupReturnedNotList);
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let call = BlockCallRef::from_rimu_spanned(item)
            .map_err(EvalError::SetupListItemNotBlockCallRef)?;
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
