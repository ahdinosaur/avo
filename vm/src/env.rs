use std::path::{Path, PathBuf};

#[derive(Default, Clone)]
pub struct Environment {
    data_dir: PathBuf,
    cache_dir: PathBuf,
    runtime_dir: PathBuf,
}

impl Environment {
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

    pub fn get_data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn get_cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    pub fn get_runtime_dir(&self) -> &Path {
        &self.runtime_dir
    }

    pub fn get_images_dir(&self) -> PathBuf {
        self.cache_dir.join("images")
    }

    pub fn get_image_file(&self, image: &str) -> PathBuf {
        self.get_images_dir().join(image)
    }

    pub fn get_image_cache_file(&self) -> PathBuf {
        self.cache_dir.join("images.cache")
    }

    pub fn get_instances_dir(&self) -> PathBuf {
        self.data_dir.join("instance")
    }

    pub fn get_instance_dir(&self, instance: &str) -> PathBuf {
        self.get_instances_dir().join(instance)
    }

    pub fn get_instance_config_file(&self, instance: &str) -> PathBuf {
        self.get_instance_dir(instance).join("machine.yaml")
    }

    pub fn get_instance_image_file(&self, instance: &str) -> PathBuf {
        self.get_instance_dir(instance).join("machine.img")
    }

    pub fn get_instance_cache_dir(&self, instance: &str) -> PathBuf {
        self.cache_dir.join("instances").join(instance)
    }

    pub fn get_instance_runtime_dir(&self, instance: &str) -> PathBuf {
        self.runtime_dir.join("instances").join(instance)
    }

    pub fn get_qemu_pid_file(&self, instance: &str) -> PathBuf {
        self.get_instance_runtime_dir(instance).join("qemu.pid")
    }
}
