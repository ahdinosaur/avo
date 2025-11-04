use thiserror::Error;

use crate::{
    context::{Context, ContextError},
    machines::VmMachine,
    runs::VmRunError,
};

mod context;
mod fs;
mod http;
mod images;
mod machines;
mod paths;
mod qemu;
mod runs;

#[derive(Error, Debug)]
pub enum VmError {
    #[error(transparent)]
    Context(#[from] ContextError),

    #[error(transparent)]
    Run(#[from] VmRunError),
}

pub async fn run(machine: VmMachine) -> Result<(), VmError> {
    let ctx = Context::new()?;
    runs::run(&mut ctx, machine).await?;
    Ok(())
}
