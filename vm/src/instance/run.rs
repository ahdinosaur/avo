use std::{
    net::Ipv4Addr, path::PathBuf, sync::{atomic::AtomicBool, Arc}, time::Duration
};
use avo_machine::Machine;
use avo_system::{CpuCount, MemorySize};
use thiserror::Error;
use tokio::task::{JoinError, JoinSet};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::{
    context::Context,
    instance::{
        setup::{setup_instance, SetupInstanceError, VmInstance},
        VmPort, VmVolume,
    },
    qemu::{Qemu, QemuError},
    ssh::{error::SshError, ssh_command, SshLaunchOpts},
};


pub struct InstanceRunner {
    instance_id: String,
    instance_dir: PathBuf,
    machine: &Machine

}

#[derive(Error, Debug)]
pub enum RunInstanceError {
    #[error(transparent)]
    SetupInstance(#[from] SetupInstanceError),

    #[error(transparent)]
    DirLock(#[from] dir_lock::Error),

    #[error(transparent)]
    Qemu(#[from] QemuError),

    #[error(transparent)]
    Join(#[from] JoinError),

    #[error(transparent)]
    Ssh(#[from] SshError),
}

#[derive(Debug, Clone)]
pub struct RunInstanceOptions {
    pub volumes: Vec<VmVolume>,
    pub ports: Vec<VmPort>,
    pub show_window: bool,
    pub disable_kvm: bool,
}

impl Default for RunInstanceOptions {
    fn default() -> Self {
        Self {
            volumes: vec![],
            ports: vec![],
            show_window: true,
            disable_kvm: false,
        }
    }
}

pub async fn start_instance(
    ctx: &mut Context,
    instance_id: &str,
    machine: &Machine,
    command: &str,
    options: RunInstanceOptions,
) -> Result<Option<u32>, RunInstanceError> {
    let paths = ctx.paths();
    let executables = ctx.executables();

    let RunInstanceOptions {
        volumes,
        ports: other_ports,
        show_window,
        disable_kvm,
    } = options;

    let vm = machine.vm;
    let vm_instance = setup_instance(ctx, instance_id, machine).await?;

    let mut ports = vec![VmPort {
        host_ip: Some(Ipv4Addr::LOCALHOST),
        host_port: Some(ssh_port),
        vm_port: 22,
    }];
    ports.extend(other_ports);

    let private_key = vm_instance.ssh_keypair.private_key.clone();
    let username = vm_instance.user.clone();
    let ssh_port = vm_instance.ssh_port.as_u16();

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
            .nographic(!show_window)
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

    let qemu_launch_opts = QemuLaunchOpts {
        vm: machine.vm.clone(),
        vm_instance,
        volumes,
        ports,
        show_vm_window: show_window,
        disable_kvm: false,
    };

    let ssh_launch_opts = SshLaunchOpts {
        private_key,
        addrs: (Ipv4Addr::LOCALHOST, ssh_port),
        username,
        config: Default::default(),
        command: command.to_owned(),
        timeout: Duration::from_secs(120),
    };

    let cancellatation_tokens = CancellationTokens::default();

    let mut joinset: JoinSet<Result<Option<u32>, RunError>> = JoinSet::new();
    joinset.spawn({
        let paths = ctx.paths().clone();
        let executables = ctx.executables().clone();
        let cancellatation_tokens = cancellatation_tokens.clone();
        let qemu_should_exit = Arc::new(AtomicBool::new(false));

        async move {
            launch_qemu(
                &paths,
                &executables,
                qemu_launch_opts,
                cancellatation_tokens,
                qemu_should_exit,
            )
            .await?;

            Ok(None)
        }
    });
    joinset.spawn({
        let cancellatation_tokens = cancellatation_tokens.clone();

        async move {
            let exit_code = ssh_command(ssh_launch_opts, Some(cancellatation_tokens)).await?;

            Ok(exit_code)
        }
    });

    let mut exit_code = None;
    while let Some(res) = joinset.join_next().await {
        // Workaround to make sure we only return an exit code
        // from SSH. The qemu task will always return None
        if let Some(actual_code) = res?? {
            exit_code = Some(actual_code);
        };
    }

    Ok(exit_code)
}

pub async fn attach_instance(
    ctx: &mut Context,
    instance_id: &str,
    machine: &Machine,
    command: &str,
    options: RunInstanceOptions,
) -> Result<Option<u32>, RunInstanceError> {
    let paths = ctx.paths();
    let executables = ctx.executables();

    let RunInstanceOptions {
        volumes,
        ports: other_ports,
        show_window,
        disable_kvm,
    } = options;

    let vm = machine.vm;
    let vm_instance = setup_instance(ctx, instance_id, machine).await?;

    let mut ports = vec![VmPort {
        host_ip: Some(Ipv4Addr::LOCALHOST),
        host_port: Some(ssh_port),
        vm_port: 22,
    }];
    ports.extend(other_ports);

    let private_key = vm_instance.ssh_keypair.private_key.clone();
    let username = vm_instance.user.clone();
    let ssh_port = vm_instance.ssh_port.as_u16();
