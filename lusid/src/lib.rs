mod config;

use std::{env, net::Ipv4Addr, path::PathBuf, sync::Arc, time::Duration};

use clap::{Parser, Subcommand};
use lusid_ctx::Context;
use lusid_ssh::{Ssh, SshConnectOptions, SshError, SshVolume};
use lusid_vm::{Vm, VmError, VmOptions};
use thiserror::Error;
use tokio::io::copy;
use tracing::error;
use which::which;

use crate::config::{Config, ConfigError, MachineConfig};

#[derive(Parser, Debug)]
#[command(name = "lusid", version, about = "Lusid CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    #[arg(long = "config", env = "LUSID_CONFIG", global = true)]
    pub config_path: Option<PathBuf>,

    #[arg(long = "log", env = "LUSID_LOG", global = true)]
    pub log: Option<String>,

    #[arg(env = "LUSID_APPLY_LINUX_X86_64", global = true)]
    pub lusid_apply_linux_x86_64_path: Option<String>,

    #[arg(env = "LUSID_APPLY_LINUX_AARCH64", global = true)]
    pub lusid_apply_linux_aarch64_path: Option<String>,
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
    Apply,
}

#[derive(Subcommand, Debug)]
pub enum RemoteCommand {
    // Apply config to remote machine.
    Apply {
        /// Machine identifier
        #[arg(long = "machine")]
        machine_id: String,
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

    #[error(transparent)]
    Vm(#[from] VmError),

    #[error(transparent)]
    Ssh(#[from] SshError),

    #[error("failed to convert params toml to json: {0}")]
    ParamsTomlToJson(#[from] serde_json::Error),

    #[error("failed to join stdio streams")]
    JoinStdio(#[source] tokio::io::Error),

    #[error(transparent)]
    Which(#[from] which::Error),
}

pub async fn get_config(cli: &Cli) -> Result<Config, AppError> {
    let config_path = cli
        .config_path
        .clone()
        .or_else(|| env::var("LUSID_CONFIG").ok().map(PathBuf::from))
        .or_else(|| env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    let config = Config::load(&config_path, cli).await?;
    Ok(config)
}

pub async fn run(cli: Cli, config: Config) -> Result<(), AppError> {
    match cli.command {
        Command::Machines { command } => match command {
            MachinesCommand::List => cmd_machines_list(config).await,
        },
        Command::Local { command } => match command {
            LocalCommand::Apply => cmd_local_apply(config).await,
        },
        Command::Remote { command } => match command {
            RemoteCommand::Apply { machine_id } => cmd_remote_apply(config, machine_id).await,
            RemoteCommand::Ssh { machine_id } => cmd_remote_ssh(config, machine_id).await,
        },
        Command::Dev { command } => match command {
            DevCommand::Apply { machine_id } => cmd_dev_apply(config, machine_id).await,
            DevCommand::Ssh { machine_id } => cmd_dev_ssh(config, machine_id).await,
        },
    }
}

async fn cmd_machines_list(config: Config) -> Result<(), AppError> {
    config.print_machines();
    Ok(())
}

async fn cmd_local_apply(config: Config) -> Result<(), AppError> {
    let Config {
        ref lusid_apply_linux_x86_64_path,
        ..
    } = config;
    let MachineConfig { plan, params, .. } = config.local_machine()?;

    let plan = plan.display();
    let mut command = format!("{lusid_apply_linux_x86_64_path} --plan {plan} --log trace");
    if let Some(params) = params {
        let params_json = serde_json::to_string(&params)?;
        command.push_str(&format!(" --params '{params_json}'"));
    }

    Ok(())
}

async fn cmd_remote_apply(_config: Config, _machine_id: String) -> Result<(), AppError> {
    todo!()
}

async fn cmd_remote_ssh(_config: Config, _machine_id: String) -> Result<(), AppError> {
    todo!()
}

async fn cmd_dev_apply(config: Config, machine_id: String) -> Result<(), AppError> {
    let MachineConfig {
        plan,
        machine,
        params,
    } = config.get_machine(&machine_id)?;

    let instance_id = &machine_id;
    let ports = vec![];

    let mut ctx = Context::create().unwrap();
    let options = VmOptions {
        instance_id,
        machine: &machine,
        ports,
    };
    let vm = Vm::run(&mut ctx, options).await?;

    let mut ssh = Ssh::connect(SshConnectOptions {
        private_key: vm.ssh_keypair().await?.private_key,
        addrs: (Ipv4Addr::LOCALHOST, vm.ssh_port),
        username: vm.user.clone(),
        config: Arc::new(Default::default()),
        timeout: Duration::from_secs(10),
    })
    .await?;

    let dev_dir = format!("/home/{}", vm.user);
    let plan_dir = plan.parent().unwrap();
    let plan_filename = plan.file_name().unwrap().to_string_lossy();

    let apply_bin = which(config.lusid_apply_linux_x86_64_path)?;

    let volumes = vec![
        SshVolume::FilePath {
            local: apply_bin,
            remote: format!("{dev_dir}/lusid-apply"),
        },
        SshVolume::DirPath {
            local: plan_dir.to_path_buf(),
            remote: format!("{dev_dir}/plan"),
        },
    ];

    let mut command =
        format!("{dev_dir}/lusid-apply --plan {dev_dir}/plan/{plan_filename} --log trace");
    if let Some(params) = params {
        let params_json = serde_json::to_string(&params)?;
        command.push_str(&format!(" --params '{params_json}'"));
    }

    for volume in volumes {
        ssh.sync(volume).await?;
    }

    let mut handle = ssh.command(&command).await?;

    {
        let mut stdout = tokio::io::stdout();
        let mut stderr = tokio::io::stderr();
        tokio::try_join!(
            copy(&mut handle.stdout, &mut stdout),
            copy(&mut handle.stderr, &mut stderr),
        )
        .map_err(AppError::JoinStdio)?;
    }

    let _exit_code = handle.wait().await?;

    ssh.disconnect().await?;

    Ok(())
}

async fn cmd_dev_ssh(config: Config, machine_id: String) -> Result<(), AppError> {
    let MachineConfig {
        plan: _,
        machine,
        params: _,
    } = config.get_machine(&machine_id)?;

    let instance_id = &machine_id;
    let ports = vec![];

    let mut ctx = Context::create().unwrap();
    let options = VmOptions {
        instance_id,
        machine: &machine,
        ports,
    };
    let vm = Vm::run(&mut ctx, options).await?;

    let mut ssh = Ssh::connect(SshConnectOptions {
        private_key: vm.ssh_keypair().await?.private_key,
        addrs: (Ipv4Addr::LOCALHOST, vm.ssh_port),
        username: vm.user,
        config: Arc::new(Default::default()),
        timeout: Duration::from_secs(10),
    })
    .await?;

    let _exit_code = ssh.terminal().await?;

    ssh.disconnect().await?;

    Ok(())
}
