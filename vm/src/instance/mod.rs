use avo_machine::Machine;
use avo_system::{Arch, Linux};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

use crate::{
    context::Context,
    fs::{self, FsError},
    image::{get_image, VmImage, VmImageError},
    instance::{
        kernel::{extract_kernel, ExtractKernelError, VmImageKernelDetails},
        overlay::{create_overlay_image, CreateOverlayImageError},
        ovmf::{convert_ovmf_uefi_variables, ConvertOvmfVarsError},
    },
};

mod kernel;
mod overlay;
mod ovmf;

#[derive(Error, Debug)]
pub enum VmInstanceError {
    #[error(transparent)]
    Image(#[from] VmImageError),

    #[error(transparent)]
    ConvertOvmfVars(#[from] ConvertOvmfVarsError),

    #[error(transparent)]
    ExtractKernel(#[from] ExtractKernelError),

    #[error(transparent)]
    CreateOverlayImage(#[from] CreateOverlayImageError),

    #[error(transparent)]
    Fs(#[from] FsError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum VmInstance {
    Linux {
        arch: Arch,
        linux: Linux,
        overlay_image_path: PathBuf,
        ovmf_vars_path: PathBuf,
        kernel_path: PathBuf,
        initrd_path: Option<PathBuf>,
    },
}

pub fn get_machine_id(machine: &Machine) -> &str {
    machine.hostname.as_ref()
}

pub async fn setup_instance(
    ctx: &mut Context,
    machine: &Machine,
) -> Result<VmInstance, VmInstanceError> {
    let source_image = get_image(ctx, machine).await?;

    #[allow(irrefutable_let_patterns)]
    let VmImage::Linux {
        arch,
        linux,
        image_path,
    } = source_image
    else {
        unimplemented!();
    };

    let machine_id = get_machine_id(machine);
    let machine_dir = ctx.paths().machine_dir(machine_id);
    fs::setup_directory_access(machine_dir).await?;

    let overlay_image_path = create_overlay_image(ctx.paths(), machine_id, &image_path).await?;
    let ovmf_vars_path = convert_ovmf_uefi_variables(ctx.paths(), machine_id).await?;
    let VmImageKernelDetails {
        kernel_path,
        initrd_path,
    } = extract_kernel(ctx, machine_id, &image_path).await?;

    Ok(VmInstance::Linux {
        arch,
        linux,
        overlay_image_path,
        ovmf_vars_path,
        kernel_path,
        initrd_path,
    })
}
