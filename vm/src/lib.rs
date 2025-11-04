use avo_machine::Machine;
use thiserror::Error;

use crate::{
    context::Context,
    http::{HttpClient, HttpError},
    images::{fetch_image, get_image_for_machine, get_images_list, ImageError},
    paths::Paths,
};

mod context;
mod fs;
mod http;
mod images;
mod paths;
mod qemu;

#[derive(Error, Debug)]
pub enum VmError {
    #[error(transparent)]
    Image(#[from] ImageError),

    #[error(transparent)]
    Http(#[from] HttpError),
}

pub async fn run(machine: Machine) -> Result<(), VmError> {
    let http_client = HttpClient::new()?;
    let paths = Paths::new();
    let ctx = Context::new(http_client, paths);

    let image_list = get_images_list().await?;

    println!("images: {:?}", image_list);

    let image_index = get_image_for_machine(machine).await?;

    let Some(image_index) = image_index else {
        panic!("Unable to find matching image for machine");
    };

    println!("image: {:?}", image_list);

    println!("fetching...");

    fetch_image(ctx, image_index).await?;

    Ok(())
}
