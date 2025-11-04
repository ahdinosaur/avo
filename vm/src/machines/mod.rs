use avo_system::{Arch, CpuCount, Linux, MemorySize};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

use crate::{
    context::Context,
    images::{get_image, VmImageError, VmSourceImage},
    machines::ovmf::{convert_ovmf_uefi_variables, ConvertOvmfVarsError},
};

mod kernel;
mod ovmf;

#[derive(Error, Debug)]
pub enum VmMachineError {
    #[error(transparent)]
    Image(#[from] VmImageError),

    #[error(transparent)]
    ConvertOvmfVars(#[from] ConvertOvmfVarsError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VmMachineImage {
    Linux {
        arch: Arch,
        linux: Linux,
        overlay_image_path: PathBuf,
        ovmf_eufi_vars_path: PathBuf,
        kernel_path: PathBuf,
        initrd_path: Option<PathBuf>,
    },
}

pub type VmMachine = avo_machine::Machine<MachineVmOptions>;

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct MachineVmOptions {
    pub memory_size: Option<MemorySize>,
    pub cpu_count: Option<CpuCount>,
}

pub async fn setup_machine_image(
    ctx: &mut Context,
    machine: VmMachine,
) -> Result<VmMachineImage, VmMachineError> {
    let source_image = get_image(ctx, machine).await?;

    let VmSourceImage::Linux {
        arch,
        linux,
        image_path,
    } = source_image
    else {
        unimplemented!();
    };

    convert_ovmf_uefi_variables(ctx.paths(), machine.hostname.as_ref(), &image_path).await?;

    Ok(())
}

// fn prepare_machine(paths: &Path, run_id: &)
// fn extract_kernel(paths: &Path, run_id: &)
// fn convert_ovmf_uefi_variables
// fn warmup(paths: &Path, run_id: &)
