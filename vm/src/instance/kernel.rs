use thiserror::Error;
use tokio::process::Command;

use crate::{
    context::Context,
    fs::{self, FsError},
};
use std::path::{Path, PathBuf};

pub struct VmInstanceKernelDetails {
    pub kernel_path: PathBuf,
    pub initrd_path: Option<PathBuf>,
}

#[derive(Error, Debug)]
pub enum ExtractKernelError {
    #[error("failed to get output from `virt-copy-out ...`")]
    CommandOutput(#[from] tokio::io::Error),
    #[error("virt-copy-out failed")]
    CommandError { stderr: String },
    #[error(transparent)]
    Fs(#[from] FsError),
}

/// Extract the kernel and initrd from a given image
///
/// It will extract it into the same dir of the `image_path`.
///
/// Original source: https://gitlab.archlinux.org/archlinux/vmexec/-/blob/03b649bdbcdc64d30b2943f61b51165f390b920d/src/qemu.rs#L48-91
pub async fn extract_kernel(
    ctx: &mut Context,
    instance_id: &str,
    source_image_path: &Path,
) -> Result<VmInstanceKernelDetails, ExtractKernelError> {
    let instance_dir = ctx.paths().instance_dir(instance_id);

    let output = Command::new(ctx.executables().virt_get_kernel())
        .args(["-a", &source_image_path.to_string_lossy()])
        .args(["-o", &instance_dir.to_string_lossy()])
        .arg("--unversioned-names")
        .output()
        .await?;

    if !output.status.success() {
        return Err(ExtractKernelError::CommandError {
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    let kernel_path = instance_dir.join("vmlinuz");
    let initrd_path = instance_dir.join("initrd.img");

    let initrd_path = if fs::path_exists(&initrd_path).await? {
        Some(initrd_path)
    } else {
        None
    };

    Ok(VmInstanceKernelDetails {
        kernel_path,
        initrd_path,
    })
}
