use avo_machine::Machine;

use crate::{
    context::Context,
    http::{HttpClient, HttpError},
    paths::Paths,
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

fn ctx() -> Result<Context, HttpError> {
    let http_client = HttpClient::new()?;
    let paths = Paths::new();
    Ok(Context::new(http_client, paths))
}

pub async fn run(machine: Machine) -> Result<(), VmRunError> {
    runs::run(ctx(), machine).await
}
