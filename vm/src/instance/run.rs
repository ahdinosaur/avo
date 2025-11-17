use avo_machine::Machine;
use avo_system::{CpuCount, MemorySize};
use std::{net::Ipv4Addr, path::PathBuf, time::Duration};
use thiserror::Error;
use tokio::task::JoinError;
use tracing::info;

use crate::{
    context::Context,
    instance::{
        setup::{setup_instance, SetupInstanceError},
        Instance, InstanceHandle, VmPort, VmVolume,
    },
    paths::ExecutablePaths,
    qemu::{Qemu, QemuError},
    ssh::{error::SshError, ssh_command, SshLaunchOpts},
};

#[derive(Error, Debug)]
pub enum InstanceStartError {
    #[error(transparent)]
    Qemu(#[from] QemuError),
}

pub async fn instance_start(
    executables: &ExecutablePaths,
    instance: &Instance,
) -> Result<Option<u32>, InstanceStartError> {
    let Instance {
        id: instance_id,
        dir: instance_dir,
        arch: _,
        linux: _,
        kernel_root,
        user: _,
        has_initrd,
        ssh_port,
        memory_size,
        cpu_count,
        volumes,
        ports,
        graphics,
        kvm,
    } = instance;
    let paths = instance.paths();

    let volumes = instance.volumes;
    let other_ports = instance.ports;
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

    let mut qemu = Qemu::new(executables.qemu_x86_64());

    qemu.easy()
        .cpu_count(cpu_count.to_string())
        .memory(memory_size_in_gb)
        .plash_drives(&paths.ovmf_code_system_path(), &paths.ovmf_vars_path());

    qemu.kernel(
        &paths.kernel_path(),
        Some(&format!("rw root={}", kernel_root)),
    );
    if has_initrd {
        qemu.initrd(&paths.initrd_path());
    }

    qemu.qmp_socket(&paths.qemu_qmp_socket_path())
        .kvm(kvm)
        .pid_file(&paths.qemu_pid_path())
        .graphics(graphics)
        .ports(&ports);

    // Overlay and cloud-init drives
    qemu.virtio_drive("overlay-disk", "qcow2", &paths.overlay_image_path())
        .virtio_drive("cloud-init", "raw", &paths.cloud_init_image_path());

    // virtiofsd-based directory shares and fstab injection
    for vol in &volumes {
        qemu.volume(executables, &instance_dir, vol).await?;
    }
    // Inject fstab via SMBIOS (must be run after virtiofsd)
    qemu.inject_fstab_smbios();

    info!("run qemu cmd: {:?}", qemu);

    let child = qemu.spawn().await?;

    Ok(InstanceHandle::new(instance_dir))
}

/*
#[derive(Error, Debug)]
pub enum InstanceAttachError {}
*/

pub async fn instance_attach(ctx: &mut Context, instance_id: &str) -> InstanceHandle {
    let paths = ctx.paths();
    let instance_dir = paths.instance_dir(instance_id);
    InstanceHandle::new(instance_dir)
}

#[derive(Error, Debug)]
pub enum InstanceExecError {
    #[error(transparent)]
    Ssh(#[from] SshError),
}

pub async fn instance_exec(
    ctx: &mut Context,
    instance_handle: &InstanceHandle,
    command: &str,
) -> Result<Option<u32>, InstanceExecError> {
    let ssh_keypair = instance_handle.ssh_keypair().await?;
    let ssh_launch_opts = SshLaunchOpts {
        private_key: ssh_keypair.private_key,
        addrs: (Ipv4Addr::LOCALHOST, ssh_port),
        username,
        config: Default::default(),
        command: command.to_owned(),
        timeout: Duration::from_secs(120),
    };
}
