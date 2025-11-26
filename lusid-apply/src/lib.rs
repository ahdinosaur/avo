use lusid_causality::{compute_epochs, EpochError};
use lusid_ctx::{Context, ContextError};
use lusid_operation::{apply_operations, merge_operations, partition_by_type, OperationApplyError};
use lusid_params::{ParamValues, ParamValuesFromTypeError};
use lusid_plan::{self, plan, PlanError, PlanId};
use lusid_resource::{Resource, ResourceState, ResourceStateError};
use lusid_store::Store;
use rimu::SourceId;
use thiserror::Error;
use tracing::{debug, error, info};

pub struct ApplyOptions {
    pub plan_id: PlanId,
    pub params_json: Option<String>,
}

#[derive(Error, Debug)]
pub enum ApplyError {
    #[error(transparent)]
    Context(#[from] ContextError),

    #[error("JSON parameters parse failed: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Failed to convert parameters for Lusid: {0}")]
    ParamValuesFromType(#[from] ParamValuesFromTypeError),

    #[error(transparent)]
    Plan(#[from] PlanError),

    #[error(transparent)]
    Epoch(#[from] EpochError),

    #[error(transparent)]
    ResourceState(#[from] ResourceStateError),

    #[error(transparent)]
    OperationApply(#[from] OperationApplyError),
}

pub async fn apply(options: ApplyOptions) -> Result<(), ApplyError> {
    info!("starting");
    let ApplyOptions {
        plan_id,
        params_json,
    } = options;

    let ctx = Context::create()?;
    let mut store = Store::new(ctx.paths().cache_dir());

    info!(plan = %plan_id, "using plan");

    let param_values = match params_json {
        None => {
            info!("no parameters provided");
            None
        }
        Some(json) => {
            let value: serde_json::Value = serde_json::from_str(&json)?;
            let source_id = SourceId::from("<cli:params>".to_string());
            let params = ParamValues::from_type(value, source_id)?;
            Some(params)
        }
    };

    // Parse/evaluate to Tree<ResourceParams>
    let resource_params = plan(plan_id, param_values, &mut store).await?;
    debug!("Resource params: {resource_params:?}");

    // Map to Tree<Resource>
    let resources = resource_params.map_tree(|params| params.resources());
    debug!("Resources: {resources:?}");

    // Get Tree<(Resource, ResourceState)>
    let resource_states = resources
        .map_result_async(|resource| async move {
            let state = resource.state().await?;
            Ok::<(Resource, ResourceState), ResourceStateError>((resource, state))
        })
        .await?;
    debug!("Resource states: {resource_states:?}");

    // Get Tree<ResourceChange>
    let changes = resource_states
        .map_option(|(resource, state)| resource.change(&state))
        .unwrap();
    debug!("Changes: {changes:?}");

    // Get Tree<Operations>
    let operations = changes.map_tree(|change| change.operations());

    debug!("Operations tree: {operations:?}");

    let operation_epochs = compute_epochs(operations)?;
    let epochs_count = operation_epochs.len();
    debug!("Operation epochs: {operation_epochs:?}");

    for (epoch_index, operations) in operation_epochs.into_iter().enumerate() {
        info!(
            epoch = epoch_index,
            count = epochs_count,
            "processing epoch"
        );

        let operations = partition_by_type(operations);
        let merged = merge_operations(operations);
        apply_operations(merged).await?;
    }

    info!("Apply completed");
    Ok(())
}
