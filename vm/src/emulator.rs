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
    instance::VmInstance,
    qemu::{Qemu, QemuError},
    utils::escape_path,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VmPort {
    pub host_ip: Option<Ipv4Addr>,
    pub host_port: Option<u16>,
    pub vm_port: u16,
}

impl Display for VmPort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut wrote_left = false;

        if let Some(ip) = self.host_ip {
            write!(f, "{}", ip)?;
            wrote_left = true;
        }

        if let Some(port) = self.host_port {
            if wrote_left {
                write!(f, ":")?;
            }
            write!(f, "{}", port)?;
            wrote_left = true;
        }

        if wrote_left {
            write!(f, "->")?;
        }

        write!(f, "{}/tcp", self.vm_port)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmVolume {
    pub source: PathBuf,
    pub dest: PathBuf,
    pub read_only: bool,
}

impl VmVolume {
    pub fn tag(&self) -> String {
        escape_path(&self.dest.to_string_lossy())
    }

    pub fn socket_name(&self) -> String {
        format!("{}.sock", self.tag())
    }
}

impl Display for VmVolume {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let source = self.source.to_string_lossy();
        let dest = self.dest.to_string_lossy();
        if self.read_only {
            write!(f, "{source}:{dest}:ro")
        } else {
            write!(f, "{source}:{dest}")
        }
    }
}

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

        let kernel_path_str = kernel_path.to_string_lossy().to_string();
        let initrd_path_str = initrd_path.map(|p| p.to_string_lossy().into_owned());

        let memory_size = vm
            .memory_size
            .unwrap_or_else(|| MemorySize::new(8 * 1024 * 1024 * 1024));
        let memory_size_in_gb: u64 = u64::from(memory_size) / 1024 / 1024 / 1024;
        let cpu_count = vm.cpu_count.unwrap_or_else(|| CpuCount::new(2));

        let qmp_socket_path = instance_dir.join("qmp.sock,server,wait=off");
        let qmp_socket_path_str = qmp_socket_path.to_string_lossy().to_string();

        let mut qemu = Qemu::new(executables.qemu_x86_64());

        qemu.easy()
            .cpu_count(cpu_count.to_string())
            .memory(memory_size_in_gb)
            .plash_drives(&paths.ovmf_code_system_file(), &ovmf_vars_path);

        qemu.kernel(&kernel_path, Some(&format!("rw root={}", kernel_root)));
        if let Some(initrd) = initrd_path {
            qemu = qemu.initrd(&initrd);
        }

        qemu.ports(&ports)
            .qmp_unix(&qmp_socket_path_str)
            .kvm(!disable_kvm)
            .pid_file(&paths.qemu_pid_file(&instance_id))
            .nographic(!show_vm_window);

        // Overlay and cloud-init drives
        qemu = qemu
            .virtio_drive("overlay-disk", "qcow2", &overlay_image_path)
            .virtio_drive("cloud-init", "raw", &cloud_init_image);

        // virtiofsd-based directory shares and fstab injection
        for vol in &volumes {
            qemu.volume(executables, &instance_dir, vol).await?;
        }
        // Inject fstab via SMBIOS (must be run after virtiofsd)
        qemu.inject_fstab_smbios();

        info!("run qemu cmd: {:?}", qemu);

        // Spawn QEMU
        let mut child = qemu.spawn().await?;

        let pid = child.id().ok_or(EmulatorError::PidUnavailable)?;

        Ok(EmulatorHandle { pid })
    }
}

pub struct EmulatorHandle {
    pid: u32,
}

#[derive(Error, Debug)]
pub enum EmulatorHandleError {
    #[error(transparent)]
    Nix(#[from] nix::errno::Errno),
}

impl EmulatorHandle {
    fn kill(&self, signal: Signal) -> Result<(), EmulatorHandleError> {
        let pid = Pid::from_raw(self.pid as i32);
        kill(pid, Some(signal))?;
        Ok(())
    }
}
