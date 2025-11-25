mod config;

use std::{env, io, path::PathBuf, time::Duration};

use clap::{Parser, Subcommand};
use lusid_apply::{apply, ApplyError, ApplyOptions};
use lusid_ctx::Context;
use lusid_ssh::SshVolume;
use lusid_system::Hostname;
use lusid_vm::{vm, VmError, VmOptions};
use thiserror::Error;
use tracing::error;

use crate::config::{Config, ConfigError, MachineConfig};

const LUDIS_APPLY_X86_64: &[u8] =
    include_bytes!("../../target/x86_64-unknown-linux-gnu/release/lusid-apply");

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

    #[error("machine id not found: {machine_id}")]
    MachineIdNotFound { machine_id: String },

    #[error(transparent)]
    ApplyError(#[from] ApplyError),

    #[error(transparent)]
    VmError(#[from] VmError),
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
    let MachineConfig { plan, machine } =
        config
            .machines
            .get(&machine_id)
            .cloned()
            .ok_or_else(|| AppError::MachineIdNotFound {
                machine_id: machine_id.clone(),
            })?;
    let instance_id = &machine_id;
    let ports = vec![];

    let plan_path = plan.as_path().unwrap();
    let plan_dir = plan_path.parent().unwrap();
    let plan_filename = plan_path.file_name().unwrap().to_string_lossy();
    let volumes = vec![
        SshVolume::FileBytes {
            local: LUDIS_APPLY_X86_64.to_vec(),
            permissions: Some(0o755),
            remote: "/home/debian/lusid-apply".to_owned(),
        },
        SshVolume::DirPath {
            local: plan_dir.to_path_buf(),
            remote: "/home/debian/plan".to_owned(),
        },
    ];

    let mut command = format!("/home/debian/lusid-apply --plan /home/debian/plan/{plan_filename}");
    if let Some(params_json) = params_json {
        command.push_str(&format!(" --params '{params_json}'"));
    }

    let timeout = Duration::from_secs(10);
    let options = VmOptions {
        instance_id,
        machine: &machine,
        ports,
        volumes,
        command: &command,
        timeout,
    };

    let mut ctx = Context::create().unwrap();
    vm(&mut ctx, options).await?;

    Ok(())
}

async fn cmd_dev_ssh(config: Config, machine_id: String) -> Result<(), AppError> {
    todo!()
}
