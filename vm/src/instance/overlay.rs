use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::process::Command;

use crate::{
    fs::{self, FsError},
    paths::Paths,
};

#[derive(Error, Debug)]
pub enum CreateOverlayImageError {
    #[error(transparent)]
    Fs(#[from] FsError),

    #[error("failed to get output from `qemu-img create ...`")]
    CommandOutput(#[from] tokio::io::Error),

    #[error("qemu-img create failed: {stderr}")]
    CommandError { stderr: String },
}
/// Create an overlay image based on a source image
pub async fn create_overlay_image(
    paths: &Paths,
    instance_id: &str,
    source_image_path: &Path,
) -> Result<PathBuf, CreateOverlayImageError> {
    let overlay_image_path = paths.overlay_image_file(instance_id);

    let source_image_str = source_image_path.to_string_lossy();
    let backing_file = format!("backing_file={source_image_str},backing_fmt=qcow2,nocow=on");

    if !fs::path_exists(&overlay_image_path).await? {
        let output = Command::new("qemu-img")
            .arg("create")
            .args(["-o", &backing_file])
            .args(["-f", "qcow2"])
            .arg(&overlay_image_path)
            .output()
            .await?;

        if !output.status.success() {
            return Err(CreateOverlayImageError::CommandError {
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }
    }

    Ok(overlay_image_path)
}
