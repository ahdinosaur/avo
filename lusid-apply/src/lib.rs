use lusid_apply_stdio::{AppUpdate, FlatViewTreeNode};
use lusid_causality::{compute_epochs, CausalityMeta, CausalityTree, EpochError};
use lusid_ctx::{Context, ContextError};
use lusid_operation::{apply_operations, merge_operations, partition_by_type, OperationApplyError};
use lusid_params::{ParamValues, ParamValuesFromTypeError};
use lusid_plan::{self, map_plan_subitems, plan, plan_view_tree, PlanError, PlanId, PlanNodeId};
use lusid_resource::{Resource, ResourceState, ResourceStateError};
use lusid_store::Store;
use lusid_tree::{FlatTree, FlatTreeMapItem, FlatTreeMappedItem};
use lusid_view::{Render, ViewTree};
use rimu::SourceId;
use thiserror::Error;
use tokio::io::{AsyncWriteExt, Stdout};
use tracing::{debug, error, info};

pub struct ApplyOptions {
    pub plan_id: PlanId,
    pub params_json: Option<String>,
}

#[derive(Error, Debug)]
pub enum ApplyError {
    #[error(transparent)]
    Context(#[from] ContextError),

    #[error("failed to parse JSON parameters: {0}")]
    JsonParameters(#[source] serde_json::Error),

    #[error("failed to output JSON: {0}")]
    JsonOutput(#[source] serde_json::Error),

    #[error("failed to write to stdout: {0}")]
    WriteStdout(#[source] tokio::io::Error),

    #[error("failed to flush stdout: {0}")]
    FlushStdout(#[source] tokio::io::Error),

    #[error("failed to convert parameters for Lusid: {0}")]
    ParamValuesFromType(#[from] ParamValuesFromTypeError),

    #[error(transparent)]
    Plan(#[from] PlanError),

    #[error(transparent)]
    Epoch(#[from] EpochError<PlanNodeId>),

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

    let mut stdout = tokio::io::stdout();
    let ctx = Context::create()?;
    let mut store = Store::new(ctx.paths().cache_dir());

    info!(plan = %plan_id, "using plan");

    let param_values = match params_json {
        None => {
            info!("no parameters provided");
            None
        }
        Some(json) => {
            let value: serde_json::Value =
                serde_json::from_str(&json).map_err(ApplyError::JsonParameters)?;
            let source_id = SourceId::from("<cli:params>".to_string());
            let params = ParamValues::from_type(value, source_id)?;
            Some(params)
        }
    };

    // Parse/evaluate to tree of resource params.
    let resource_params = plan(plan_id, param_values, &mut store).await?;
    debug!("Resource params: {resource_params:?}");
    writeln_update(
        &mut stdout,
        AppUpdate::ResourceParams {
            resource_params: plan_view_tree(resource_params),
        },
    );
    let resource_params = FlatTree::from(resource_params);

    // Get tree of atomic resources.
    writeln_update(&mut stdout, AppUpdate::ResourcesStart);
    let resources = resource_params.map_tree(
        |node| map_plan_subitems(node, |node| node.resources()),
        |update| {
            writeln_update(
                &mut stdout,
                AppUpdate::ResourcesNode {
                    index: update.index,
                    update: plan_view_tree(update.tree),
                },
            )
        },
    );
    debug!("Resources: {:?}", CausalityTree::from(resources));
    writeln_update(&mut stdout, AppUpdate::ResourcesComplete);

    // Get tree of (resource, resource state)
    let resource_states = resources
        .map_result_async(|resource| async move {
            let state = resource.state().await?;
            Ok::<(Resource, ResourceState), ResourceStateError>((resource, state))
        })
        .await?;
    debug!("Resource states: {resource_states:?}");
    let states = resource_states.clone().map(|(_resource, state)| state);
    writeln_output(&states, &mut stdout).await?;

    // Get CausalityTree<ResourceChange>
    let changes = resource_states.map_option(|(resource, state)| resource.change(&state));
    debug!("Changes: {changes:?}");

    let Some(changes) = changes else {
        info!("No changes to apply!");
        return Ok(());
    };
    writeln_output(&changes, &mut stdout).await?;

    // Get CausalityTree<Operations>
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
        debug!("Operations: {operations:?}");

        let operations = partition_by_type(operations);
        debug!("Operations by type: {operations:?}");

        let merged = merge_operations(operations);
        debug!("Merged operations: {merged:?}");

        apply_operations(merged).await?;
    }

    info!("Apply completed");
    Ok(())
}

async fn writeln_update(stdout: &mut Stdout, update: &AppUpdate) -> Result<(), ApplyError> {
    stdout
        .write_all(&serde_json::to_vec(&update).map_err(ApplyError::JsonOutput)?)
        .await
        .map_err(ApplyError::WriteStdout)?;

    stdout
        .write_all(b"\n")
        .await
        .map_err(ApplyError::WriteStdout)?;

    stdout.flush().await.map_err(ApplyError::FlushStdout)?;

    Ok(())
}
