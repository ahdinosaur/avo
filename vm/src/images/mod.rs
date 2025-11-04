use avo_machine::Machine;
use thiserror::Error;

mod hash;
mod list;

use crate::{
    context::Context,
    fs::{self, FsError},
    http::HttpError,
    images::{
        hash::{ImageHash, ImageHashError},
        list::{ImageIndex, ImagesList},
    },
};

#[derive(Error, Debug)]
pub enum ImageError {
    #[error("Failed to load image cache: {0}")]
    CacheLoad(#[from] toml::de::Error),

    #[error(transparent)]
    Hash(#[from] ImageHashError),

    #[error(transparent)]
    Http(#[from] HttpError),

    #[error(transparent)]
    Fs(#[from] FsError),
}

pub async fn get_images_list() -> Result<ImagesList, ImageError> {
    let images_str = include_str!("../../images.toml");
    let images_list: ImagesList = toml::from_str(images_str)?;
    Ok(images_list)
}

pub async fn get_image_for_machine(machine: Machine) -> Result<Option<ImageIndex>, ImageError> {
    let images_list = get_images_list().await?;
    let image_index = images_list
        .into_values()
        .find(|image_index| image_index.os == machine.os && image_index.arch == machine.arch);
    Ok(image_index)
}

pub async fn fetch_image(mut ctx: Context, image_index: ImageIndex) -> Result<(), ImageError> {
    let image_path = ctx.paths().image_file(&image_index.to_image_file_name());

    fs::setup_directory_access(ctx.paths().images_dir()).await?;

    ctx.http_client()
        .download_file(image_index.image.to_url(), &image_path)
        .await?;

    let hash_path = ctx.paths().image_file(&image_index.to_hash_file_name());

    ctx.http_client()
        .download_file(image_index.hash.to_url(), &hash_path)
        .await?;

    let hash = ImageHash::new(&image_index.hash, &hash_path);
    hash.validate(&image_index, &image_path).await?;

    Ok(())
}
