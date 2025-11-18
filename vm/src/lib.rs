use std::time::Duration;

use avo_machine::Machine;
use thiserror::Error;
use tokio::time::sleep;

use crate::{
    context::{Context, ContextError},
    instance::{Instance, InstanceError, InstanceSetupOptions, VmPort, VmVolume},
};

mod cmd;
mod context;
mod fs;
mod http;
mod image;
mod instance;
mod paths;
mod qemu;
mod ssh;
mod utils;

#[derive(Error, Debug)]
pub enum VmError {
    #[error(transparent)]
    Context(#[from] ContextError),

    #[error(transparent)]
    Instance(#[from] InstanceError),
}

pub struct RunOptions<'a> {
    instance_id: &'a str,
    machine: &'a Machine,
    ports: Vec<VmPort>,
    volumes: Vec<VmVolume>,
    command: &'a str,
}

pub async fn run(options: RunOptions<'_>) -> Result<(), VmError> {
    install_tracing();
    let mut ctx = Context::new()?;

    let RunOptions {
        instance_id,
        machine,
        ports,
        volumes,
        command,
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
        Instance::setup(&mut ctx, setup_options).await?
    };

    if !instance.is_qemu_running().await? {
        instance.start(&mut ctx).await?;

        loop {
            if instance.is_ssh_open() {
                break;
            }

            sleep(Duration::from_millis(100));
        }
    }

    instance.exec(command).await?;

    Ok(())
}

fn install_tracing() {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        // all spans/events with a level higher than TRACE (e.g, debug, info, warn, etc.)
        // will be written to stdout.
        .with_max_level(tracing::Level::TRACE)
        // builds the subscriber.
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
}
