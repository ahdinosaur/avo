use std::path::Path;
use thiserror::Error;

use crate::{
    cmd::{Command, CommandError},
    fs::{self, FsError},
    instance::VmInstancePaths,
};

#[derive(Error, Debug)]
pub enum CreateOverlayImageError {
    #[error(transparent)]
    Fs(#[from] FsError),

    #[error(transparent)]
    Command(#[from] CommandError),
}
/// Create an overlay image based on a source image
pub async fn setup_overlay(
    paths: &VmInstancePaths<'_>,
    source_image_path: &Path,
) -> Result<(), CreateOverlayImageError> {
    let overlay_image_path = paths.overlay_image_path();

    if !fs::path_exists(&overlay_image_path).await? {
        let backing_file = format!(
            "backing_file={},backing_fmt=qcow2,nocow=on",
            source_image_path.display()
        );

        Command::new("qemu-img")
            .arg("create")
            .args(["-o", &backing_file])
            .args(["-f", "qcow2"])
            .arg(&overlay_image_path)
            .run()
            .await?;
    }

    Ok(())
}
