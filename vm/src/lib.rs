use avo_machine::Machine;
use thiserror::Error;

use crate::{
    context::{Context, ContextError},
    runs::VmRunError,
};

mod context;
mod fs;
mod http;
mod images;
mod machines;
mod paths;
mod qemu;
mod run;
mod ssh;
mod utils;

#[derive(Error, Debug)]
pub enum VmError {
    #[error(transparent)]
    Context(#[from] ContextError),

    #[error(transparent)]
    Run(#[from] VmRunError),
}

pub async fn run(machine: Machine) -> Result<(), VmError> {
    let mut ctx = Context::new()?;
    runs::run(&mut ctx, machine).await?;
    Ok(())
}
