use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Paths {
    data_dir: PathBuf,
    cache_dir: PathBuf,
    runtime_dir: PathBuf,
}

impl Paths {
    pub fn new(
        data_dir: impl Into<PathBuf>,
        cache_dir: impl Into<PathBuf>,
        runtime_dir: impl Into<PathBuf>,
    ) -> Self {
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

    pub fn runtime_dir(&self) -> &Path {
        &self.runtime_dir
    }

    pub fn images_dir(&self) -> PathBuf {
        self.cache_dir.join("images")
    }

    pub fn image_file(&self, image: &str) -> PathBuf {
        self.images_dir().join(image)
    }

    pub fn image_cache_file(&self) -> PathBuf {
        self.cache_dir.join("images.cache")
    }

    pub fn instances_dir(&self) -> PathBuf {
        self.data_dir.join("instance")
    }

    pub fn instance_dir(&self, instance: &str) -> PathBuf {
        self.instances_dir().join(instance)
    }

    pub fn instance_config_file(&self, instance: &str) -> PathBuf {
        self.instance_dir(instance).join("machine.yaml")
    }

    pub fn instance_image_file(&self, instance: &str) -> PathBuf {
        self.instance_dir(instance).join("machine.img")
    }

    pub fn instance_cache_dir(&self, instance: &str) -> PathBuf {
        self.cache_dir.join("instances").join(instance)
    }

    pub fn instance_runtime_dir(&self, instance: &str) -> PathBuf {
        self.runtime_dir.join("instances").join(instance)
    }

    pub fn qemu_pid_file(&self, instance: &str) -> PathBuf {
        self.instance_runtime_dir(instance).join("qemu.pid")
    }
}
