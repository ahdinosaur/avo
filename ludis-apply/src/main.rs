use clap::Parser;
use ludis_causality::{compute_epochs, EpochError};
use ludis_ctx::{Context, ContextError};
use ludis_operation::{apply_operations, merge_operations, OperationApplyError};
use ludis_params::{ParamValues, ParamValuesFromTypeError};
use ludis_plan::{self, plan, PlanError, PlanId};
use ludis_resource::{Resource, ResourceState, ResourceStateError};
use ludis_store::Store;
use rimu::SourceId;
use std::path::PathBuf;
use thiserror::Error;
use tracing::{debug, error, info};
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Parser, Debug)]
#[command(name = "ludis-apply", about = "Apply a Ludis plan.", version)]
struct Cli {
    /// Absolute or relative path to the .ludis plan file.
    #[arg(long = "plan", value_name = "PATH")]
    plan: PathBuf,

    /// Parameters as a JSON string (top-level object).
    #[arg(long = "params", value_name = "PARAMS")]
    params: Option<String>,

    /// Log level (e.g., trace, debug, info, warn, error). Default: info.
    #[arg(long = "log", value_name = "LEVEL", default_value = "info")]
    log: String,
}

#[derive(Error, Debug)]
enum AppError {
    #[error(transparent)]
    Context(#[from] ContextError),

    #[error("JSON parameters parse failed: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Failed to convert parameters for Ludis: {0}")]
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

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    install_tracing(&cli.log);
    debug!(cli = ?cli, "parsed cli");

    if let Err(err) = run(cli).await {
        error!("{err}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<(), AppError> {
    info!("starting");

    let env = Context::create()?;
    let mut store = Store::new(env.paths().cache_dir());

    let plan_path = cli.plan.canonicalize().unwrap_or(cli.plan.clone());
    let plan_id = PlanId::Path(plan_path.clone());
    info!(plan = %plan_path.display(), "using plan");

    let param_values = match cli.params {
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
    info!("resource params constructed");

    // Map to Tree<Resource>
    let resources = resource_params.map_tree(|params| params.resources());

    // Get Tree<(Resource, ResourceState)>
    let resource_states = resources
        .map_result_async(|resource| async move {
            let state = resource.state().await?;
            Ok::<(Resource, ResourceState), ResourceStateError>((resource, state))
        })
        .await?;

    // Get Tree<ResourceChange>
    let changes = resource_states
        .map_option(|(resource, state)| resource.change(&state))
        .unwrap();

    // Get Tree<Operations>
    let operations = changes.map_tree(|change| change.operations());

    let operation_epochs = compute_epochs(operations)?;
    let epochs_count = operation_epochs.len();

    for (epoch_index, operations) in operation_epochs.into_iter().enumerate() {
        info!(
            epoch = epoch_index,
            count = epochs_count,
            "processing epoch"
        );

        let merged = merge_operations(&operations);
        apply_operations(&merged).await?;
    }

    info!("apply completed");
    Ok(())
}

fn install_tracing(level: &str) {
    let filter = EnvFilter::try_new(level).unwrap_or_else(|_| EnvFilter::new("info"));
    fmt()
        .with_env_filter(filter)
        .json()
        .with_current_span(true)
        .with_target(true)
        .with_level(true)
        .with_ansi(false)
        .init();
}
