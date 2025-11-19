use directories::ProjectDirs;
use std::path::{Path, PathBuf};
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
            ProjectDirs::from("dev", "Ludis Org", "Ludis").expect("Failed to get project directory");
        let data_dir = dirs.data_dir();
        let cache_dir = dirs.cache_dir();
        let runtime_dir = dirs.runtime_dir().unwrap_or(cache_dir);
        Self {
            data_dir: data_dir.into(),
            cache_dir: cache_dir.into(),
            runtime_dir: runtime_dir.into(),
        }
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    #[allow(dead_code)]
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
        self.data_dir().join("vm/instances")
    }

    pub fn instance_dir(&self, instance_id: &str) -> PathBuf {
        self.instances_dir().join(instance_id)
    }
}

#[derive(Error, Debug)]
#[error(transparent)]
pub struct ExecutablePathsError(#[from] which::Error);

#[derive(Debug, Clone)]
pub struct ExecutablePaths {
    virt_get_kernel: PathBuf,
    qemu_x86_64: PathBuf,
    qemu_aarch64: PathBuf,
    qemu_img: PathBuf,
    mkisofs: PathBuf,
}

impl ExecutablePaths {
    pub fn new() -> Result<ExecutablePaths, ExecutablePathsError> {
        let virt_get_kernel = which_global("virt-get-kernel")?;
        let qemu_x86_64 = which_global("qemu-system-x86_64")?;
        let qemu_aarch64 = which_global("qemu-system-aarch64")?;
        let qemu_img = which_global("qemu-img")?;
        let mkisofs = which_global("mkisofs")?;

        Ok(ExecutablePaths {
            virt_get_kernel,
            qemu_x86_64,
            qemu_aarch64,
            qemu_img,
            mkisofs,
        })
    }

    pub fn virt_get_kernel(&self) -> &Path {
        &self.virt_get_kernel
    }

    pub fn qemu_x86_64(&self) -> &Path {
        &self.qemu_x86_64
    }

    pub fn qemu_aarch64(&self) -> &Path {
        &self.qemu_aarch64
    }

    pub fn qemu_img(&self) -> &Path {
        &self.qemu_img
    }

    pub fn mkisofs(&self) -> &Path {
        &self.mkisofs
    }
}
