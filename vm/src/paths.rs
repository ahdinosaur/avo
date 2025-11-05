use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use thiserror::Error;
use which::which_global;

#[derive(Debug, Clone)]
pub struct Paths {
    data_dir: PathBuf,
    cache_dir: PathBuf,
    runtime_dir: PathBuf,
}

impl Paths {
    pub fn new() -> Self {
        let dirs =
            ProjectDirs::from("dev", "Avo Org", "Avo").expect("Failed to get project directory");
        let data_dir = dirs.data_dir();
        let cache_dir = dirs.cache_dir();
        let runtime_dir = dirs.runtime_dir().unwrap_or(cache_dir);
        Self {
            data_dir: data_dir.into(),
            cache_dir: cache_dir.into(),
            runtime_dir: runtime_dir.into(),
        }
    }

    pub fn ovmf_vars_system_file(&self) -> &Path {
        &PathBuf::from("/usr/share/OVMF/OVMF_VARS_4M.fd")
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    pub fn runtime_dir(&self) -> &Path {
        &self.runtime_dir
    }

    pub fn images_dir(&self) -> PathBuf {
        self.cache_dir().join("vm/images")
    }

    pub fn image_file(&self, image_file_name: &str) -> PathBuf {
        self.images_dir().join(image_file_name)
    }

    pub fn machines_dir(&self) -> PathBuf {
        self.runtime_dir().join("vm/machines")
    }

    pub fn machine_dir(&self, machine_id: &str) -> PathBuf {
        self.machines_dir().join(machine_id)
    }

    pub fn ovmf_vars_file(&self, machine_id: &str) -> PathBuf {
        self.machine_dir(machine_id).join("OVMF_VARS.4m.fd.qcow2")
    }

    pub fn overlay_image_file(&self, machine_id: &str) -> PathBuf {
        self.machine_dir(machine_id).join("overlay.qcow2")
    }
}

#[derive(Error, Debug)]
#[error(transparent)]
pub struct ExecutablePathsError(#[from] which::Error);

#[derive(Debug, Clone)]
pub struct ExecutablePaths {
    virt_copy_out: PathBuf,
    virtiofsd: PathBuf,
    qemu_x86_64: PathBuf,
    qemu_aarch64: PathBuf,
    unshare: PathBuf,
}

impl ExecutablePaths {
    pub fn new() -> Result<ExecutablePaths, ExecutablePathsError> {
        let virt_copy_out = which_global("virt-copy-out")?;
        let virtiofsd = which_global("virtiofsd")?;
        let qemu_x86_64 = which_global("qemu-system-x86_64")?;
        let qemu_aarch64 = which_global("qemu-system-aarch64")?;
        let unshare = which_global("unshare")?;

        Ok(ExecutablePaths {
            virt_copy_out,
            virtiofsd,
            qemu_x86_64,
            qemu_aarch64,
            unshare,
        })
    }

    pub fn virt_copy_out(&self) -> &Path {
        &self.virt_copy_out
    }

    pub fn virtiofsd(&self) -> &Path {
        &self.virtiofsd
    }

    pub fn qemu_x86_64(&self) -> &Path {
        &self.qemu_x86_64
    }

    pub fn qemu_aarch64(&self) -> &Path {
        &self.qemu_aarch64
    }

    pub fn unshare(&self) -> &Path {
        &self.unshare
    }
}
