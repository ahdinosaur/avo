use clap::Parser;
use ludis_env::{Environment, EnvironmentError};
use ludis_operation::{apply as apply_operations, ApplyError};
use ludis_params::{ParamValues, ParamValuesFromTypeError};
use ludis_plan::{self, plan, PlanError, PlanId};
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
    #[error("JSON parameters parse failed: {0}")]
    Env(#[from] EnvironmentError),

    #[error("JSON parameters parse failed: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Failed to convert parameters for Ludis: {0}")]
    ParamValuesFromType(#[from] ParamValuesFromTypeError),

    #[error(transparent)]
    Plan(#[from] PlanError),

    #[error(transparent)]
    Apply(#[from] ApplyError),
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    install_tracing(&cli.log);

    if let Err(err) = run(cli).await {
        error!("{err}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<(), AppError> {
    info!("starting");
    debug!(cli = ?cli, "parsed cli");

    let env = Environment::create()?;
    let mut store = Store::new(env.cache_dir());

    // Resolve plan id
    let plan_path = cli.plan.canonicalize().unwrap_or(cli.plan.clone());
    let plan_id = PlanId::Path(plan_path.clone());
    info!(plan = %plan_path.display(), "using plan");

    // JSON parameters
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

    // Plan -> operations tree
    let operations = plan(plan_id, param_values, &mut store).await?;
    info!("plan constructed; applying");

    // Apply
    apply_operations(operations).await?;
    info!("apply completed");
    Ok(())
}

fn install_tracing(level: &str) {
    // Accept either a simple level ("debug") or a filter directive.
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
