use std::{
    path::{Path, PathBuf},
    sync::LazyLock,
};

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

    pub fn ovmf_code_system_file(&self) -> &Path {
        static OVMF_CODE_SYSTEM_FILE: LazyLock<PathBuf> =
            LazyLock::new(|| PathBuf::from("/usr/share/OVMF/OVMF_CODE_4M.fd"));

        OVMF_CODE_SYSTEM_FILE.as_path()
    }

    pub fn ovmf_vars_system_file(&self) -> &Path {
        static OVMF_VARS_SYSTEM_FILE: LazyLock<PathBuf> =
            LazyLock::new(|| PathBuf::from("/usr/share/OVMF/OVMF_VARS_4M.fd"));

        OVMF_VARS_SYSTEM_FILE.as_path()
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

    pub fn instances_dir(&self) -> PathBuf {
        self.data_dir().join("vm/machines")
    }

    pub fn instance_dir(&self, instance_id: &str) -> PathBuf {
        self.instances_dir().join(instance_id)
    }

    pub fn ovmf_vars_file(&self, instance_id: &str) -> PathBuf {
        self.instance_dir(instance_id).join("OVMF_VARS.4m.fd.qcow2")
    }

    pub fn overlay_image_file(&self, instance_id: &str) -> PathBuf {
        self.instance_dir(instance_id).join("overlay.qcow2")
    }

    pub fn cloud_init_meta_data_file(&self, instance_id: &str) -> PathBuf {
        self.instance_dir(instance_id).join("cloud-init-meta-data")
    }

    pub fn cloud_init_user_data_file(&self, instance_id: &str) -> PathBuf {
        self.instance_dir(instance_id).join("cloud-init-user-data")
    }

    pub fn cloud_init_image_file(&self, instance_id: &str) -> PathBuf {
        self.instance_dir(instance_id).join("cloud-init.iso")
    }
}

#[derive(Error, Debug)]
#[error(transparent)]
pub struct ExecutablePathsError(#[from] which::Error);

#[derive(Debug, Clone)]
pub struct ExecutablePaths {
    virt_copy_out: PathBuf,
    virt_get_kernel: PathBuf,
    virtiofsd: PathBuf,
    qemu_x86_64: PathBuf,
    qemu_aarch64: PathBuf,
    mkisofs: PathBuf,
    unshare: PathBuf,
}

impl ExecutablePaths {
    pub fn new() -> Result<ExecutablePaths, ExecutablePathsError> {
        let virt_copy_out = which_global("virt-copy-out")?;
        let virt_get_kernel = which_global("virt-get-kernel")?;
        let virtiofsd = which_global("virtiofsd")?;
        let qemu_x86_64 = which_global("qemu-system-x86_64")?;
        let qemu_aarch64 = which_global("qemu-system-aarch64")?;
        let mkisofs = which_global("mkisofs")?;
        let unshare = which_global("unshare")?;

        Ok(ExecutablePaths {
            virt_copy_out,
            virt_get_kernel,
            virtiofsd,
            qemu_x86_64,
            qemu_aarch64,
            mkisofs,
            unshare,
        })
    }

    pub fn virt_copy_out(&self) -> &Path {
        &self.virt_copy_out
    }
    pub fn virt_get_kernel(&self) -> &Path {
        &self.virt_get_kernel
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

    pub fn mkisofs(&self) -> &Path {
        &self.mkisofs
    }

    pub fn unshare(&self) -> &Path {
        &self.unshare
    }
}
