use lusid_fs::{self as fs, FsError};
use thiserror::Error;

use crate::{
    cmd::{Command, CommandError},
    instance::InstancePaths,
    paths::ExecutablePaths,
};

#[derive(Error, Debug)]
pub enum ConvertOvmfVarsError {
    #[error(transparent)]
    Fs(#[from] FsError),

    #[error(transparent)]
    Command(#[from] CommandError),
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
pub(super) async fn setup_ovmf_uefi_variables(
    executables: &ExecutablePaths,
    paths: &InstancePaths<'_>,
) -> Result<(), ConvertOvmfVarsError> {
    let ovmf_vars_system_path = paths.ovmf_vars_system_path();
    let ovmf_vars_path = paths.ovmf_vars_path();

    if !fs::path_exists(&ovmf_vars_path).await? {
        Command::new(executables.qemu_img())
            .arg("convert")
            .args(["-O", "qcow2"])
            .arg(ovmf_vars_system_path)
            .arg(&ovmf_vars_path)
            .run()
            .await?;
    }

    Ok(())
}
