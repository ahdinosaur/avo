use std::path::PathBuf;
use thiserror::Error;
use tokio::process::Command;

use crate::{
    fs::{self, FsError},
    paths::Paths,
};

#[derive(Error, Debug)]
pub enum ConvertOvmfVarsError {
    #[error(transparent)]
    Fs(#[from] FsError),

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
/// Original source: https://gitlab.archlinux.org/archlinux/vmexec/-/blob/03b649bdbcdc64d30b2943f61b51165f390b920d/src/qemu.rs#L93-124
pub async fn convert_ovmf_uefi_variables(
    paths: &Paths,
    instance_id: &str,
) -> Result<PathBuf, ConvertOvmfVarsError> {
    let ovmf_vars_system_path = paths.ovmf_vars_system_file();
    let ovmf_vars_path = paths.ovmf_vars_file(instance_id);

    if !fs::path_exists(&ovmf_vars_path).await? {
        let output = Command::new("qemu-img")
            .arg("convert")
            .args(["-O", "qcow2"])
            .arg(&ovmf_vars_system_path)
            .arg(&ovmf_vars_path)
            .output()
            .await?;

        if !output.status.success() {
            return Err(ConvertOvmfVarsError::CommandError {
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }
    }

    Ok(ovmf_vars_path)
}
