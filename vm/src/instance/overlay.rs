use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::process::Command;

use crate::paths::Paths;

#[derive(Error, Debug)]
pub enum CreateOverlayImageError {
    #[error("failed to get output from `qemu-img create ...`")]
    CommandOutput(#[from] tokio::io::Error),
    #[error("qemu-img create failed: {stderr}")]
    CommandError { stderr: String },
}
/// Create an overlay image based on a source image
pub async fn create_overlay_image(
    paths: &Paths,
    machine_id: &str,
    source_image_path: &Path,
) -> Result<PathBuf, CreateOverlayImageError> {
    let overlay_image_path = paths.overlay_image_file(machine_id);

    let source_image_str = source_image_path.to_string_lossy();
    let backing_file = format!("backing_file={source_image_str},backing_fmt=qcow2,nocow=on");
    let mut qemu_img_cmd = Command::new("qemu-img");
    qemu_img_cmd
        .arg("create")
        .args(["-o", &backing_file])
        .args(["-f", "qcow2"])
        .arg(&overlay_image_path);

    let qemu_img_output = qemu_img_cmd.output().await?;
    if !qemu_img_output.status.success() {
        return Err(CreateOverlayImageError::CommandError {
            stderr: String::from_utf8_lossy(&qemu_img_output.stderr).to_string(),
        });
    }

    Ok(overlay_image_path)
}
