use lusid_params::{validate, ParamValues};
use lusid_resource::{apt::Apt, ResourceParams, ResourceType};
use rimu::Spanned;

use crate::PlanActionToResourceError;

pub fn is_core_module(module: &Spanned<String>) -> Option<&str> {
    module.inner().strip_prefix("@core/")
}

pub fn core_module(
    core_module_id: &str,
    param_values: Option<Spanned<ParamValues>>,
) -> Result<ResourceParams, PlanActionToResourceError> {
    match core_module_id {
        Apt::ID => core_module_for_resource::<Apt>(param_values).map(ResourceParams::Apt),
        other => Err(PlanActionToResourceError::UnsupportedCoreModuleId {
            id: other.to_string(),
        }),
    }
}

fn core_module_for_resource<R: ResourceType>(
    param_values: Option<Spanned<ParamValues>>,
) -> Result<R::Params, PlanActionToResourceError> {
    let param_values = param_values.ok_or(PlanActionToResourceError::MissingParams)?;
    let param_types = R::param_types();
    validate(param_types.as_ref(), Some(&param_values)).map_err(PlanActionToResourceError::from)?;
    let params: R::Params = param_values
        .into_inner()
        .into_type()
        .map_err(PlanActionToResourceError::from)?;
    Ok(params)
}
