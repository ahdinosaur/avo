mod context;
mod image;
mod instance;
mod paths;
mod qemu;
mod utils;

pub use crate::instance::VmPort;

use lusid_ctx::Context as BaseContext;
use lusid_machine::Machine;
use lusid_ssh::SshVolume;
use std::time::Duration;
use thiserror::Error;
use tokio::time::sleep;

use crate::{
    context::{Context, ContextError},
    instance::{Instance, InstanceError, InstanceSetupOptions},
};

#[derive(Error, Debug)]
pub enum VmError {
    #[error(transparent)]
    Context(#[from] ContextError),

    #[error(transparent)]
    Instance(#[from] InstanceError),

    #[error("instance not found")]
    InstanceNotFound { instance_id: String },

    #[error("instance not running")]
    InstanceNotRunning { instance_id: String },
}

pub struct VmExecOptions<'a> {
    pub instance_id: &'a str,
    pub machine: &'a Machine,
    pub ports: Vec<VmPort>,
    pub volumes: Vec<SshVolume>,
    pub command: &'a str,
    pub timeout: Duration,
}

pub async fn vm_exec(ctx: &mut BaseContext, options: VmExecOptions<'_>) -> Result<(), VmError> {
    let mut ctx = Context::create(ctx)?;

    let VmExecOptions {
        instance_id,
        machine,
        ports,
        volumes,
        command,
        timeout,
    } = options;

    let instance = if Instance::exists(&mut ctx, instance_id).await? {
        Instance::load(&mut ctx, instance_id).await?
    } else {
        let setup_options = InstanceSetupOptions {
            instance_id,
            machine,
            ports,
        };
        let inst = Instance::setup(&mut ctx, setup_options).await?;
        inst.save().await?;
        inst
    };

    if !instance.is_qemu_running().await? {
        instance.start(&mut ctx).await?;

        loop {
            if instance.is_ssh_open() {
                break;
            }

            sleep(Duration::from_millis(100)).await;
        }
    }

    instance.exec(command, volumes, timeout).await?;

    Ok(())
}

pub struct VmTerminalOptions<'a> {
    pub instance_id: &'a str,
    pub timeout: Duration,
}

pub async fn vm_terminal(
    ctx: &mut BaseContext,
    options: VmTerminalOptions<'_>,
) -> Result<(), VmError> {
    let mut ctx = Context::create(ctx)?;

    let VmTerminalOptions {
        instance_id,
        timeout,
    } = options;

    let instance = if Instance::exists(&mut ctx, instance_id).await? {
        Instance::load(&mut ctx, instance_id).await?
    } else {
        return Err(VmError::InstanceNotFound {
            instance_id: instance_id.to_owned(),
        });
    };

    if !instance.is_qemu_running().await? {
        return Err(VmError::InstanceNotRunning {
            instance_id: instance_id.to_owned(),
        });
    }

    instance.terminal(timeout).await?;

    Ok(())
}
