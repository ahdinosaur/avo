use std::path::PathBuf;

use avo_system::{Arch, CpuCount, Linux, MemorySize};
use serde::{Deserialize, Serialize};

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

// fn prepare_machine(paths: &Path, run_id: &)
// fn extract_kernel(paths: &Path, run_id: &)
// fn convert_ovmf_uefi_variables
// fn warmup(paths: &Path, run_id: &)
