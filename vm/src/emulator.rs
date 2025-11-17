use avo_machine::MachineVmOptions;
use avo_system::{CpuCount, MemorySize};
use nix::{
    sys::signal::{kill, Signal},
    unistd::Pid,
};
use serde::{Deserialize, Serialize};
use std::{fmt::Display, net::Ipv4Addr, path::PathBuf};
use thiserror::Error;
use tracing::info;

use crate::{
    context::Context,
    fs::{self, FsError},
    instance::VmInstance,
    qemu::{Qemu, QemuError},
    utils::escape_path,
};

#[derive(Error, Debug)]
pub enum EmulatorError {
    #[error(transparent)]
    Qemu(#[from] QemuError),

    #[error("pid unavailable, process exited immediately?")]
    PidUnavailable,
}

#[derive(Debug)]
pub struct Emulator {
    qemu: Qemu,
}

#[derive(Debug, Clone)]
pub struct EmulatorLaunchOptions {
    pub vm: MachineVmOptions,
    pub vm_instance: VmInstance,
    pub volumes: Vec<VmVolume>,
    pub ports: Vec<VmPort>,
    pub show_vm_window: bool,
    pub disable_kvm: bool,
}

impl Emulator {
    pub async fn launch(
        ctx: &mut Context,
        options: EmulatorLaunchOptions,
    ) -> Result<EmulatorHandle, EmulatorError> {
        let paths = ctx.paths();
        let executables = ctx.executables();

        let EmulatorLaunchOptions {
            vm,
            vm_instance,
            volumes,
            ports,
            show_vm_window,
            disable_kvm,
        } = options;

        let VmInstance {
            id: instance_id,
            dir: instance_dir,
            arch: _,
            linux: _,
            kernel_root,
            user: _,
            overlay_image_path,
            kernel_path,
            initrd_path,
            ovmf_vars_path,
            ssh_keypair: _,
            ssh_port: _,
            cloud_init_image,
        } = vm_instance;

        let pid_file_path = paths.qemu_pid_file(&instance_id);

        let memory_size = vm
            .memory_size
            .unwrap_or_else(|| MemorySize::new(8 * 1024 * 1024 * 1024));
        let memory_size_in_gb: u64 = u64::from(memory_size) / 1024 / 1024 / 1024;
        let cpu_count = vm.cpu_count.unwrap_or_else(|| CpuCount::new(2));

        let mut qemu = Qemu::new(executables.qemu_x86_64());

        qemu.easy()
            .cpu_count(cpu_count.to_string())
            .memory(memory_size_in_gb)
            .plash_drives(&paths.ovmf_code_system_file(), &ovmf_vars_path);

        qemu.kernel(&kernel_path, Some(&format!("rw root={}", kernel_root)));
        if let Some(initrd) = initrd_path {
            qemu.initrd(&initrd);
        }

        qemu.qmp_socket(&paths.qemu_qmp_socket(&instance_id))
            .kvm(!disable_kvm)
            .pid_file(&pid_file_path)
            .nographic(!show_vm_window)
            .ports(&ports);

        // Overlay and cloud-init drives
        qemu.virtio_drive("overlay-disk", "qcow2", &overlay_image_path)
            .virtio_drive("cloud-init", "raw", &cloud_init_image);

        // virtiofsd-based directory shares and fstab injection
        for vol in &volumes {
            qemu.volume(executables, &instance_dir, vol).await?;
        }
        // Inject fstab via SMBIOS (must be run after virtiofsd)
        qemu.inject_fstab_smbios();

        info!("run qemu cmd: {:?}", qemu);

        // Spawn QEMU
        let child = qemu.spawn().await?;

        Ok(EmulatorHandle { pid_file_path })
    }
}

pub struct EmulatorHandle {
    pid: u32,
    pid_file_path: PathBuf,
}

#[derive(Error, Debug)]
pub enum EmulatorHandleError {
    #[error(transparent)]
    Fs(#[from] FsError),

    #[error(transparent)]
    Nix(#[from] nix::errno::Errno),
}

impl EmulatorHandle {
    pub async fn is_running(&self) -> Result<bool, EmulatorHandleError> {
        let pid_exists = fs::path_exists(&self.pid_file_path).await?;
        Ok(pid_exists)
    }

    pub fn stop(&self) -> Result<(), EmulatorHandleError> {
        let pid = Pid::from_raw(self.pid as i32);
        kill(pid, Some(Signal::SIGKILL))?;
        Ok(())
    }
}
