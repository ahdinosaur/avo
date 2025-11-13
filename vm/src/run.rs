use std::{
    net::Ipv4Addr,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use avo_machine::Machine;
use thiserror::Error;
use tokio::task::{JoinError, JoinSet};
use tokio_util::sync::CancellationToken;
use tracing::debug;

use crate::{
    context::Context,
    instance::{setup_instance, VmInstanceError},
    qemu::{launch_qemu, PublishPort, QemuLaunchError, QemuLaunchOpts},
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

pub async fn run(ctx: &mut Context, machine: &Machine) -> Result<Option<u32>, RunError> {
    let vm_instance = setup_instance(ctx, machine).await?;

    let private_key = vm_instance.ssh_keypair.private_key.clone();

    let qemu_launch_opts = QemuLaunchOpts {
        vm: machine.vm.clone(),
        vm_instance,
        volumes: vec![],
        published_ports: vec![PublishPort {
            host_ip: Some(Ipv4Addr::LOCALHOST),
            host_port: Some(2222),
            vm_port: 22,
        }],
        show_vm_window: true,
        disable_kvm: false,
    };

    let ssh_launch_opts = SshLaunchOpts {
        private_key,
        addrs: (Ipv4Addr::LOCALHOST, 2222),
        username: "debian".to_owned(),
        config: Default::default(),
        command: "echo hi".to_owned(),
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
            debug!("call launch_qemu()");
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
            debug!("call ssh_command()");
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
