use ludis_operation::{Operation, OperationTrait, PackageOperation};
use ludis_params::{validate, ParamValues};
use rimu::Spanned;

use crate::PlanActionToOperationError;

pub fn core_module(
    core_module_id: &str,
    param_values: Option<Spanned<ParamValues>>,
) -> Result<Operation, PlanActionToOperationError> {
    let param_values = param_values.ok_or(PlanActionToOperationError::MissingParams)?;
    let operation = match core_module_id {
        "pkg" => core_module_for_operation::<PackageOperation>(param_values)?,
        other => {
            return Err(PlanActionToOperationError::UnsupportedCoreModuleId {
                id: other.to_string(),
            });
        }
    };
    Ok(operation)
}

pub fn is_core_module(module: &Spanned<String>) -> Option<&str> {
    module.inner().strip_prefix("@core/")
}

fn core_module_for_operation<Op: OperationTrait>(
    param_values: Spanned<ParamValues>,
) -> Result<Operation, PlanActionToOperationError> {
    let param_types = Op::param_types();
    validate(param_types.as_ref(), Some(&param_values))
        .map_err(PlanActionToOperationError::from)?;
    let package_params: Op::Params = param_values
        .into_inner()
        .into_type()
        .map_err(PlanActionToOperationError::from)?;
    let operation = Op::new(package_params).into();
    Ok(operation)
}
