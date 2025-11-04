use avo_machine::Machine;
use thiserror::Error;

use crate::{
    context::Context,
    http::HttpError,
    images::{get_image, VmImageError},
};

#[derive(Error, Debug)]
pub enum VmRunError {
    #[error(transparent)]
    Image(#[from] VmImageError),

    #[error(transparent)]
    Http(#[from] HttpError),
}

fn create_run_id() -> String {
    cuid2::create_id()
}

pub async fn run(ctx: &mut Context, machine: Machine) -> Result<(), VmRunError> {
    let image = get_image(ctx, machine).await?;

    let run_id = create_run_id();

    Ok(())
}
