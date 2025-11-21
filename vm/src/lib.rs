mod cmd;
mod context;
mod image;
mod instance;
mod paths;
mod qemu;
mod utils;

pub use crate::instance::{VmPort, VmVolume};

use ludis_machine::Machine;
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
}

pub struct RunOptions<'a> {
    pub instance_id: &'a str,
    pub machine: &'a Machine,
    pub ports: Vec<VmPort>,
    pub volumes: Vec<VmVolume>,
    pub command: &'a str,
    pub timeout: Duration,
}

pub async fn run(options: RunOptions<'_>) -> Result<(), VmError> {
    let mut ctx = Context::new()?;

    let RunOptions {
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
            volumes,
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

    instance.exec(command, timeout).await?;

    Ok(())
}
