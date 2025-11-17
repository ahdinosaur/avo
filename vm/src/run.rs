use std::{
    net::Ipv4Addr,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use avo_machine::Machine;
use thiserror::Error;
use tokio::task::{JoinError, JoinSet};
use tokio_util::sync::CancellationToken;

use crate::{
    context::Context,
    instance::{setup_instance, VmInstanceError},
    qemu::{launch_qemu, QemuLaunchError, QemuLaunchOpts, VmPort, VmVolume},
    ssh::{error::SshError, ssh_command, SshLaunchOpts},
};

#[derive(Debug, Clone, Default)]
pub struct CancellationTokens {
    pub qemu: CancellationToken,
    pub ssh: CancellationToken,
}

#[derive(Error, Debug)]
pub enum RunError {
    #[error(transparent)]
    Instance(#[from] VmInstanceError),

    #[error(transparent)]
    DirLock(#[from] dir_lock::Error),

    #[error(transparent)]
    Qemu(#[from] QemuLaunchError),

    #[error(transparent)]
    Join(#[from] JoinError),

    #[error(transparent)]
    Ssh(#[from] SshError),
}

#[derive(Debug, Clone)]
pub struct VmRunOptions {
    pub volumes: Vec<VmVolume>,
    pub ports: Vec<VmPort>,
    pub show_window: bool,
}

impl Default for VmRunOptions {
    fn default() -> Self {
        Self {
            volumes: vec![],
            ports: vec![],
            show_window: true,
        }
    }
}

pub async fn run(
    ctx: &mut Context,
    instance_id: &str,
    machine: &Machine,
    command: &str,
    options: VmRunOptions,
) -> Result<Option<u32>, RunError> {
    let VmRunOptions {
        volumes,
        ports: other_ports,
        show_window,
    } = options;

    let vm_instance = setup_instance(ctx, instance_id, machine).await?;

    let private_key = vm_instance.ssh_keypair.private_key.clone();
    let username = vm_instance.user.clone();
    let ssh_port = vm_instance.ssh_port.as_u16();

    let mut ports = vec![VmPort {
        host_ip: Some(Ipv4Addr::LOCALHOST),
        host_port: Some(ssh_port),
        vm_port: 22,
    }];
    ports.extend(other_ports);

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
