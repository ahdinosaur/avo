// See https://github.com/cubic-vm/cubic/blob/main/src/image/image_factory.rs

use std::collections::HashMap;

use avo_machine::{Arch, Os};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{context::Context, http::HttpError};

#[derive(Error, Debug)]
pub enum ImageError {
    #[error("Failed to load image cache: {0}")]
    CacheLoad(#[from] toml::de::Error),

    #[error(transparent)]
    Http(#[from] HttpError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageIndex {
    pub os: Os,
    pub arch: Arch,
    pub image: ImageRef,
    pub hash: HashRef,
}

impl ImageIndex {
    pub fn to_name(&self) -> String {
        format!("{}:{}", self.os, self.arch)
    }

    pub fn to_file_name(&self) -> String {
        format!("{}_{}", self.os, self.arch)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ImageRef {
    Qcow2 { url: String },
}

impl ImageRef {
    fn to_url(&self) -> &str {
        match self {
            ImageRef::Qcow2 { url } => url,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HashRef {
    Sha512Sums { url: String },
}

type ImageList = HashMap<String, ImageIndex>;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImageListContent {
    images: ImageList,
}

pub async fn load_list() -> Result<ImageList, ImageError> {
    let images_str = include_str!("../images.toml");
    let images_cache: ImageListContent = toml::from_str(images_str)?;
    Ok(images_cache.images)
}

pub async fn fetch_image(mut ctx: Context, image_index: ImageIndex) -> Result<(), ImageError> {
    let image_path = ctx.paths().images_dir().join(image_index.to_file_name());

    ctx.http_client()
        .download_file(image_index.image.to_url(), image_path)
        .await?;

    Ok(())
}
