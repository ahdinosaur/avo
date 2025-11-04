use avo_system::Linux;
use thiserror::Error;
use tokio::process::Command;

use crate::context::Context;
use std::path::{Path, PathBuf};

pub struct VmImageKernel {
    kernel_path: PathBuf,
    initrd_path: Option<PathBuf>,
}

#[derive(Error, Debug)]
pub enum ExtractKernelError {
    #[error("failed to get output from `virt-copy-out ...`")]
    CommandOutput(#[from] tokio::io::Error),
    #[error("virt-copy-out failed")]
    CommandError { stderr: String },
}

/// Extract the kernel and initrd from a given image
///
/// It will extract it into the same dir of the `image_path`.
///
/// Original source: https://gitlab.archlinux.org/archlinux/vmexec/-/blob/03b649bdbcdc64d30b2943f61b51165f390b920d/src/qemu.rs#L48-91
pub async fn extract_kernel(
    ctx: &mut Context,
    machine_id: &str,
    linux: Linux,
    source_image_path: &Path,
) -> Result<VmImageKernel, ExtractKernelError> {
    let kernel = "vmlinuz-linux";
    let initrd = if matches!(linux, Linux::Arch) {
        None
    } else {
        Some("initramfs-linux,img")
    };

    let dest_dir = ctx.paths().machine_dir(machine_id);
    let mut virt_copy_out_cmd = Command::new(ctx.executables().virt_copy_out());

    let mut files_to_extract = Vec::with_capacity(2);
    if let Some(initrd) = initrd {
        files_to_extract.push(format!("/boot/{}", initrd));
    }
    files_to_extract.push(format!("/boot/{}", kernel));

    virt_copy_out_cmd
        .args(["-a", &source_image_path.to_string_lossy()])
        .args(files_to_extract)
        .arg(&dest_dir);

    let virt_copy_out_output = virt_copy_out_cmd.output().await?;
    if !virt_copy_out_output.status.success() {
        return Err(ExtractKernelError::CommandError {
            stderr: String::from_utf8_lossy(&virt_copy_out_output.stderr).to_string(),
        });
    }

    Ok(VmImageKernel {
        kernel_path: dest_dir.join(kernel),
        initrd_path: initrd.map(|initrd| dest_dir.join(initrd)),
    })
}
