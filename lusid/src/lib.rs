mod config;

use std::{env, io, path::PathBuf};

use clap::{Parser, Subcommand};
use lusid_apply::{apply, ApplyError, ApplyOptions};
use lusid_system::Hostname;
use thiserror::Error;
use tracing::error;

use crate::config::{Config, ConfigError};

#[derive(Parser, Debug)]
#[command(name = "lusid", version, about = "Lusid CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    #[arg(long = "config", global = true)]
    pub config_path: Option<PathBuf>,

    #[arg(long = "log", global = true, default_value = "info")]
    pub log: String,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Manage machine definitions
    Machines {
        #[command(subcommand)]
        command: MachinesCommand,
    },
    /// Manage local machine
    Local {
        #[command(subcommand)]
        command: LocalCommand,
    },
    /// Manage remote machines
    Remote {
        #[command(subcommand)]
        command: RemoteCommand,
    },
    /// Develop using virtual machines
    Dev {
        #[command(subcommand)]
        command: DevCommand,
    },
}

#[derive(Subcommand, Debug)]
pub enum MachinesCommand {
    /// List machines from machines.toml
    List,
}

#[derive(Subcommand, Debug)]
pub enum LocalCommand {
    // Apply config to local machine.
    Apply {
        /// Parameters as a JSON string.
        #[arg(long = "params")]
        params_json: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum RemoteCommand {
    // Apply config to remote machine.
    Apply {
        /// Machine identifier
        #[arg(long = "machine")]
        machine_id: String,

        /// Parameters as a JSON string.
        #[arg(long = "params")]
        params_json: Option<String>,
    },

    // Ssh into remote machine.
    Ssh {
        #[arg(long = "machine")]
        machine_id: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum DevCommand {
    // Spin up virtual machine and apply config.
    Apply {
        /// Machine identifier
        #[arg(long = "machine")]
        machine_id: String,

        /// Parameters as a JSON string.
        #[arg(long = "params")]
        params_json: Option<String>,
    },

    // Ssh into virtual machine.
    Ssh {
        #[arg(long = "machine")]
        machine_id: String,
    },
}

#[derive(Error, Debug)]
pub enum AppError {
    #[error(transparent)]
    Config(#[from] ConfigError),

    #[error(transparent)]
    EnvVar(#[from] env::VarError),

    #[error("failed to get hostname: {0}")]
    GetHostname(#[source] io::Error),

    #[error("local machine not found: {hostname}")]
    LocalMachineNotFound { hostname: Hostname },

    #[error(transparent)]
    ApplyError(#[from] ApplyError),
}

pub async fn get_config(cli: &Cli) -> Result<Config, AppError> {
    let config_path = cli
        .config_path
        .clone()
        .or_else(|| env::var("LUDIS_CONFIG").ok().map(PathBuf::from))
        .or_else(|| env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    let config = Config::load(&config_path).await?;
    Ok(config)
}

pub async fn run(cli: Cli) -> Result<(), AppError> {
    let config = get_config(&cli).await?;
    match cli.command {
        Command::Machines { command } => match command {
            MachinesCommand::List => cmd_machines_list(config).await,
        },
        Command::Local { command } => match command {
            LocalCommand::Apply { params_json } => cmd_local_apply(config, params_json).await,
        },
        Command::Remote { command } => match command {
            RemoteCommand::Apply {
                machine_id,
                params_json,
            } => cmd_remote_apply(config, machine_id, params_json).await,
            RemoteCommand::Ssh { machine_id } => cmd_remote_ssh(config, machine_id).await,
        },
        Command::Dev { command } => match command {
            DevCommand::Apply {
                machine_id,
                params_json,
            } => cmd_dev_apply(config, machine_id, params_json).await,
            DevCommand::Ssh { machine_id } => cmd_dev_ssh(config, machine_id).await,
        },
    }
}

async fn cmd_machines_list(config: Config) -> Result<(), AppError> {
    config.print_machines();
    Ok(())
}

async fn cmd_local_apply(config: Config, params_json: Option<String>) -> Result<(), AppError> {
    let hostname = Hostname::get().map_err(AppError::GetHostname)?;
    let machine = config
        .machines
        .into_values()
        .find(|config| config.machine.hostname == hostname);
    let Some(machine) = machine else {
        return Err(AppError::LocalMachineNotFound { hostname });
    };

    let plan_id = machine.plan;
    let options = ApplyOptions {
        plan_id,
        params_json,
    };
    apply(options).await?;

    Ok(())
}

async fn cmd_remote_apply(
    config: Config,
    machine_id: String,
    params_json: Option<String>,
) -> Result<(), AppError> {
    todo!()
}

async fn cmd_remote_ssh(config: Config, machine_id: String) -> Result<(), AppError> {
    todo!()
}

async fn cmd_dev_apply(
    config: Config,
    machine_id: String,
    params_json: Option<String>,
) -> Result<(), AppError> {
    todo!()
}

async fn cmd_dev_ssh(config: Config, machine_id: String) -> Result<(), AppError> {
    todo!()
}
