use avo_machine::Machine;
use thiserror::Error;

use crate::context::{Context, ContextError};

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
    Run(#[from] RunError),
}

pub async fn run(
    instance_id: &str,
    machine: &Machine,
    command: &str,
    options: VmRunOptions,
) -> Result<(), VmError> {
    install_tracing();
    let mut ctx = Context::new()?;
    run::run(&mut ctx, instance_id, machine, command, options).await?;
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
