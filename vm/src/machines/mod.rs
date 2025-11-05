use avo_machine::Machine;
use avo_system::{Arch, Linux};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

use crate::{
    context::Context,
    images::{get_image, VmImageError, VmSourceImage},
    machines::{
        kernel::{extract_kernel, ExtractKernelError, VmImageKernelDetails},
        overlay::{create_overlay_image, CreateOverlayImageError},
        ovmf::{convert_ovmf_uefi_variables, ConvertOvmfVarsError},
    },
};

mod kernel;
mod overlay;
mod ovmf;

#[derive(Error, Debug)]
pub enum VmMachineError {
    #[error(transparent)]
    Image(#[from] VmImageError),

    #[error(transparent)]
    ConvertOvmfVars(#[from] ConvertOvmfVarsError),

    #[error(transparent)]
    ExtractKernel(#[from] ExtractKernelError),

    #[error(transparent)]
    CreateOverlayImage(#[from] CreateOverlayImageError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum VmMachineImage {
    Linux {
        arch: Arch,
        linux: Linux,
        overlay_image_path: PathBuf,
        ovmf_vars_path: PathBuf,
        kernel_path: PathBuf,
        initrd_path: Option<PathBuf>,
    },
}

pub async fn setup_machine_image(
    ctx: &mut Context,
    machine: Machine,
) -> Result<VmMachineImage, VmMachineError> {
    let source_image = get_image(ctx, machine.clone()).await?;

    let VmSourceImage::Linux {
        arch,
        linux,
        image_path,
    } = source_image
    else {
        unimplemented!();
    };

    let machine_id = machine.hostname.as_ref();

    let overlay_image_path = create_overlay_image(ctx.paths(), machine_id, &image_path).await?;
    let ovmf_vars_path = convert_ovmf_uefi_variables(ctx.paths(), machine_id, &image_path).await?;
    let VmImageKernelDetails {
        kernel_path,
        initrd_path,
    } = extract_kernel(ctx, machine_id, linux.clone(), &image_path).await?;

    Ok(VmMachineImage::Linux {
        arch,
        linux,
        overlay_image_path,
        ovmf_vars_path,
        kernel_path,
        initrd_path,
    })
}

// fn prepare_machine(paths: &Path, run_id: &)
// fn extract_kernel(paths: &Path, run_id: &)
// fn convert_ovmf_uefi_variables
// fn warmup(paths: &Path, run_id: &)
