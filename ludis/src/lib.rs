mod machines;

use clap::{Parser, Subcommand};
use ludis_operation::ApplyError;
use ludis_plan::PlanError;
use thiserror::Error;
use tracing::error;
use tracing_subscriber::{fmt, EnvFilter};

use crate::machines::{MachinesConfig, MachinesError};

#[derive(Parser, Debug)]
#[command(name = "ludis", version, about = "Ludis CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    #[arg(long = "log", global = true, default_value = "info")]
    pub log: String,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Manage and inspect machines
    Machines {
        #[command(subcommand)]
        command: MachinesCommand,
    },
    /// Apply Ludis config (local or remote via --machine)
    Apply {
        #[arg(long = "machine", value_name = "ID")]
        machine: Option<String>,
    },
    /// SSH into a remote machine
    Ssh {
        #[arg(long = "machine", value_name = "ID")]
        machine: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum MachinesCommand {
    /// List machines from machines.toml
    List,
}

#[derive(Error, Debug)]
pub enum AppError {
    #[error(transparent)]
    Machines(#[from] MachinesError),

    #[error("failed to initialize environment: {0}")]
    Env(#[from] ludis_env::EnvironmentError),

    #[error("failed to plan operations: {0}")]
    Plan(#[from] PlanError),

    #[error("failed to apply operations: {0}")]
    Apply(#[from] ApplyError),

    #[error("{0}")]
    Message(String),
}

pub async fn run(cli: Cli) -> Result<(), AppError> {
    match cli.command {
        Command::Machines { command } => match command {
            MachinesCommand::List => cmd_machines_list().await,
        },
        Command::Apply { machine } => todo!(),
        Command::Ssh { machine } => todo!(),
    }
}

async fn cmd_machines_list() -> Result<(), AppError> {
    let machines = MachinesConfig::load().await?;
    machines.print();
    Ok(())
}

pub fn install_tracing(level: &str) {
    let filter = EnvFilter::try_new(level).unwrap_or_else(|_| EnvFilter::new("info"));
    fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_level(true)
        .with_ansi(false)
        .init();
}
