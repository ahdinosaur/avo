use std::path::PathBuf;

use avo_machine::Machine;
use avo_system::{Arch, Linux, Os};
use serde::{Deserialize, Serialize};
use thiserror::Error;

mod hash;
mod list;

use crate::{
    context::Context,
    fs::{self, FsError},
    http::HttpError,
    images::{
        hash::{VmImageHash, VmImageHashError},
        list::{VmImageIndex, VmImagesList},
    },
    paths::Paths,
};

#[derive(Error, Debug)]
pub enum VmImageError {
    #[error("Failed to load image cache: {0}")]
    CacheLoad(#[from] toml::de::Error),

    #[error(transparent)]
    Hash(#[from] VmImageHashError),

    #[error(transparent)]
    Http(#[from] HttpError),

    #[error(transparent)]
    Fs(#[from] FsError),
}

pub async fn get_images_list() -> Result<VmImagesList, VmImageError> {
    let images_str = include_str!("../../images.toml");
    let images_list: VmImagesList = toml::from_str(images_str)?;
    Ok(images_list)
}

pub async fn find_image_index_for_machine(
    machine: Machine,
) -> Result<Option<VmImageIndex>, VmImageError> {
    let images_list = get_images_list().await?;
    let image_index = images_list
        .into_values()
        .find(|image_index| image_index.os == machine.os && image_index.arch == machine.arch);
    Ok(image_index)
}

pub async fn fetch_image(
    ctx: &mut Context,
    image_index: &VmImageIndex,
) -> Result<(), VmImageError> {
    let image_path = ctx.paths().image_file(&image_index.to_image_file_name());

    fs::setup_directory_access(ctx.paths().images_dir()).await?;

    ctx.http_client()
        .download_file(image_index.image.to_url(), &image_path)
        .await?;

    let hash_path = ctx.paths().image_file(&image_index.to_hash_file_name());

    ctx.http_client()
        .download_file(image_index.hash.to_url(), &hash_path)
        .await?;

    let hash = VmImageHash::new(&image_index.hash, &hash_path);
    hash.validate(&image_index, &image_path).await?;

    Ok(())
}

pub fn get_image(ctx: &mut Context, image_index: &VmImageIndex) -> VmImage {
    VmImage::new(ctx.paths(), image_index)
}

// An VM image and auxiliary files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VmImage {
    Linux {
        arch: Arch,
        linux: Linux,
        image_path: PathBuf,
        kernel_path: PathBuf,
        // `initrd` is optional since some distributions (such as Arch Linux) bake all required modules
        // directly into the kernel. This shaves a few hundred milliseconds off the boot so we won't use an
        // initrd if we can avoid it.
        initrd_path: Option<PathBuf>,
    },
}

impl VmImage {
    pub fn new(paths: &Paths, image_index: &VmImageIndex) -> Self {
        let image_path = paths.image_file(&image_index.to_image_file_name());
        let arch = image_index.arch;
        match &image_index.os {
            Os::Linux(linux) => {
                let kernel_path = image_path.join("vmlinuz-linux");
                let initrd_path = if matches!(linux, Linux::Arch) {
                    None
                } else {
                    Some(image_path.join("initramfs-linux,img"))
                };
                VmImage::Linux {
                    arch,
                    linux: linux.clone(),
                    image_path,
                    kernel_path,
                    initrd_path,
                }
            }
            _ => {
                unimplemented!()
            }
        }
    }

    pub fn overlay_image(&self) -> PathBuf {
        match self {
            VmImage::Linux { image_path, .. } => image_path.with_extension("overlay.qcow2"),
        }
    }
}
