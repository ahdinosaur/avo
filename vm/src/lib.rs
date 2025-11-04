use avo_machine::Machine;
use thiserror::Error;

use crate::{
    context::Context,
    http::{HttpClient, HttpError},
    images::{fetch_image, find_image_index_for_machine, get_image, get_images_list, VmImageError},
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
    Image(#[from] VmImageError),

    #[error(transparent)]
    Http(#[from] HttpError),
}

fn create_run_id() -> String {
    cuid2::create_id()
}

pub async fn run(machine: Machine) -> Result<(), VmError> {
    let http_client = HttpClient::new()?;
    let paths = Paths::new();
    let mut ctx = Context::new(http_client, paths);

    let image_list = get_images_list().await?;

    println!("images: {:?}", image_list);

    let image_index = find_image_index_for_machine(machine).await?;

    let Some(image_index) = image_index else {
        panic!("Unable to find matching image for machine");
    };

    println!("image: {:?}", image_list);

    println!("fetching...");

    fetch_image(&mut ctx, &image_index).await?;

    println!("fetched.");

    let image = get_image(&mut ctx, &image_index);

    let run_id = create_run_id();

    Ok(())
}
