use std::path::{Path, PathBuf};
use std::sync::LazyLock;

pub struct VmPaths<'a> {
    instance_dir: &'a Path,
}

impl<'a> VmPaths<'a> {
    pub fn new(instance_dir: &'a Path) -> Self {
        Self { instance_dir }
    }

    pub fn instance_dir(&self) -> &'a Path {
        self.instance_dir
    }

    pub fn state(&self) -> PathBuf {
        self.instance_dir.join("state.json")
    }

    pub fn overlay_image_path(&self) -> PathBuf {
        self.instance_dir.join("overlay.qcow2")
    }

    pub fn ovmf_vars_system_path(&self) -> &Path {
        static OVMF_VARS_SYSTEM_FILE: LazyLock<PathBuf> =
            LazyLock::new(|| PathBuf::from("/usr/share/OVMF/OVMF_VARS_4M.fd"));

        OVMF_VARS_SYSTEM_FILE.as_path()
    }

    pub fn ovmf_vars_path(&self) -> PathBuf {
        self.instance_dir.join("OVMF_VARS.4m.fd.qcow2")
    }

    pub fn ovmf_code_system_path(&self) -> &Path {
        static OVMF_CODE_SYSTEM_FILE: LazyLock<PathBuf> =
            LazyLock::new(|| PathBuf::from("/usr/share/OVMF/OVMF_CODE_4M.fd"));

        OVMF_CODE_SYSTEM_FILE.as_path()
    }

    pub fn kernel_path(&self) -> PathBuf {
        self.instance_dir.join("vmlinuz")
    }

    pub fn initrd_path(&self) -> PathBuf {
        self.instance_dir.join("initrd.img")
    }

    pub fn cloud_init_meta_data_path(&self) -> PathBuf {
        self.instance_dir.join("cloud-init-meta-data")
    }

    pub fn cloud_init_user_data_path(&self) -> PathBuf {
        self.instance_dir.join("cloud-init-user-data")
    }

    pub fn cloud_init_image_path(&self) -> PathBuf {
        self.instance_dir.join("cloud-init.iso")
    }

    pub fn qemu_pid_path(&self) -> PathBuf {
        self.instance_dir.join("qemu.pid")
    }

    pub fn qemu_qmp_socket_path(&self) -> PathBuf {
        self.instance_dir.join("qmp.sock")
    }
}
