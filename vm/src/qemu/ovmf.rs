use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::process::Command;

use crate::context::Context;

#[derive(Debug, Error)]
pub enum ConvertOvmfVarsError {
    #[error("failed to get output from `convert -O qcow $source_image $output_file`")]
    CommandOutput(#[from] tokio::io::Error),
    #[error("qemu-img convert failed")]
    CommandError { stderr: String },
}

/// Convert OVMF UEFI variables raw image to qcow2
///
/// We need it to be qcow2 so that snapshotting will work. We don't particularly want to snaphot
/// the UEFI variables, however, snapshotting the VM only works if all its writeable disks support
/// it so here we are.
///
/// Also, if we don't provide a read-write OVMF_VARS file on boot, we'll get an `NvVars` file in
/// our writeable mounts which is what QEMU uses to emulate writeable UEFI vars.
///
/// Source: https://gitlab.archlinux.org/archlinux/vmexec/-/blob/main/src/qemu.rs#L93-124
pub async fn convert_ovmf_uefi_variables(
    ctx: &Context,
    run_id: &str,
    source_image: &Path,
) -> Result<PathBuf, ConvertOvmfVarsError> {
    let output_file = ctx.paths().ovmf_vars_file(run_id);

    let mut qemu_img_cmd = Command::new("qemu-img");
    qemu_img_cmd
        .arg("convert")
        .args(["-O", "qcow2"])
        .arg(source_image)
        .arg(&output_file);

    let qemu_img_output = qemu_img_cmd.output().await?;
    if !qemu_img_output.status.success() {
        return Err(ConvertOvmfVarsError::CommandError {
            stderr: String::from_utf8_lossy(&qemu_img_output.stderr).to_string(),
        });
    }

    Ok(output_file)
}
