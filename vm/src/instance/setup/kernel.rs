use lusid_cmd::{Command, CommandError};
use lusid_fs::{self as fs, FsError};
use std::path::Path;
use thiserror::Error;

use crate::{instance::VmPaths, paths::ExecutablePaths};

pub(super) struct VmKernelDetails {
    pub has_initrd: bool,
}

#[derive(Error, Debug)]
pub enum ExtractKernelError {
    #[error(transparent)]
    Fs(#[from] FsError),

    #[error(transparent)]
    Command(#[from] CommandError),
}

/// Extract the kernel and initrd from a given image
///
/// It will extract it into the same dir of the `image_path`.
///
/// Original source: https://gitlab.archlinux.org/archlinux/vmexec/-/blob/03b649bdbcdc64d30b2943f61b51165f390b920d/src/qemu.rs#L48-91
pub(super) async fn setup_kernel(
    executables: &ExecutablePaths,
    paths: &VmPaths<'_>,
    source_image_path: &Path,
) -> Result<VmKernelDetails, ExtractKernelError> {
    let kernel_path = paths.kernel_path();

    if !fs::path_exists(&kernel_path).await? {
        Command::new(executables.virt_get_kernel())
            .args(["-a", &source_image_path.to_string_lossy()])
            .args(["-o", &paths.instance_dir().to_string_lossy()])
            .arg("--unversioned-names")
            .run()
            .await?;
    }

    let has_initrd = fs::path_exists(&paths.initrd_path()).await?;

    Ok(VmKernelDetails { has_initrd })
}
