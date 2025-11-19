use ludis_system::{CpuCount, MemorySize};
use std::net::Ipv4Addr;
use thiserror::Error;
use tracing::info;

use crate::{
    instance::{Instance, VmPort},
    paths::ExecutablePaths,
    qemu::{Qemu, QemuError},
};

#[derive(Error, Debug)]
pub enum InstanceStartError {
    #[error(transparent)]
    Qemu(#[from] QemuError),
}

pub(super) async fn instance_start(
    executables: &ExecutablePaths,
    instance: &Instance,
) -> Result<(), InstanceStartError> {
    let Instance {
        id: _instance_id,
        dir: _instance_dir,
        arch,
        linux: _,
        kernel_root,
        user: _,
        has_initrd,
        ssh_port,
        memory_size,
        cpu_count,
        volumes: _,
        ports,
        graphics,
        kvm,
    } = instance;
    let paths = instance.paths();

    let other_ports = ports.clone();
    let mut ports = vec![VmPort {
        host_ip: Some(Ipv4Addr::LOCALHOST),
        host_port: Some(*ssh_port),
        vm_port: 22,
    }];
    ports.extend(other_ports);

    let memory_size = memory_size.unwrap_or_else(|| MemorySize::new(8 * 1024 * 1024 * 1024));
    let memory_size_in_gb: u64 = u64::from(memory_size) / 1024 / 1024 / 1024;
    let cpu_count = cpu_count.unwrap_or_else(|| CpuCount::new(2));
    let graphics = graphics.unwrap_or(true);
    let kvm = kvm.unwrap_or(true);

    let qemu_executable = match arch {
        ludis_system::Arch::X86_64 => executables.qemu_x86_64(),
        ludis_system::Arch::Aarch64 => executables.qemu_aarch64(),
    };
    let mut qemu = Qemu::new(qemu_executable);

    qemu.easy()
        .cpu_count(cpu_count.to_string())
        .memory(memory_size_in_gb)
        .plash_drives(paths.ovmf_code_system_path(), &paths.ovmf_vars_path());

    qemu.kernel(
        &paths.kernel_path(),
        Some(&format!("rw root={}", kernel_root)),
    );
    if *has_initrd {
        qemu.initrd(&paths.initrd_path());
    }

    qemu.qmp_socket(&paths.qemu_qmp_socket_path())
        .kvm(kvm)
        .pid_file(paths.qemu_pid_path())
        .graphics(graphics)
        .ports(&ports);

    // Overlay and cloud-init drives
    qemu.virtio_drive("overlay-disk", "qcow2", &paths.overlay_image_path())
        .virtio_drive("cloud-init", "raw", &paths.cloud_init_image_path());

    info!("run qemu cmd: {:?}", qemu);

    let _child = qemu.spawn().await?;

    Ok(())
}
