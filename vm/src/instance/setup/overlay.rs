use lusid_cmd::{Command, CommandError};
use lusid_fs::{self as fs, FsError};
use std::path::Path;
use thiserror::Error;

use crate::instance::VmPaths;

#[derive(Error, Debug)]
pub enum CreateOverlayImageError {
    #[error(transparent)]
    Fs(#[from] FsError),

    #[error(transparent)]
    Command(#[from] CommandError),
}
/// Create an overlay image based on a source image
pub(super) async fn setup_overlay(
    paths: &VmPaths<'_>,
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
