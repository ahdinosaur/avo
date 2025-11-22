mod config;

use std::{env, path::PathBuf};

use clap::{Parser, Subcommand};
use thiserror::Error;
use tracing::error;

use crate::config::{Config, ConfigError};

#[derive(Parser, Debug)]
#[command(name = "ludis", version, about = "Ludis CLI")]
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
    Config(#[from] ConfigError),

    #[error(transparent)]
    EnvVar(#[from] env::VarError),
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
        Command::Apply { machine } => todo!(),
        Command::Ssh { machine } => todo!(),
    }
}

async fn cmd_machines_list(config: Config) -> Result<(), AppError> {
    config.print_machines();
    Ok(())
}
