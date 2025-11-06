use std::{
    io::IsTerminal,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use avo_machine::Machine;
use dir_lock::DirLock;
use rand::Rng;
use thiserror::Error;
use tokio::task::{JoinError, JoinSet};
use tokio_util::sync::CancellationToken;
use tracing::debug;

use crate::{
    context::Context,
    machines::{get_machine_id, setup_machine_image, VmMachineError},
    qemu::{launch_qemu, QemuLaunchError, QemuLaunchOpts},
    ssh::{
        connect_ssh_for_command, error::SshError, keypair::ensure_keypair, Interactive,
        SshLaunchOpts,
    },
};

#[derive(Debug, Clone, Default)]
pub struct CancellationTokens {
    pub qemu: CancellationToken,
    pub ssh: CancellationToken,
}

#[derive(Error, Debug)]
pub enum RunError {
    #[error(transparent)]
    Machine(#[from] VmMachineError),

    #[error(transparent)]
    Ssh(#[from] SshError),

    #[error(transparent)]
    DirLock(#[from] dir_lock::Error),

    #[error(transparent)]
    Qemu(#[from] QemuLaunchError),

    #[error(transparent)]
    Join(#[from] JoinError),
}

pub async fn run(ctx: &mut Context, machine: &Machine) -> Result<Option<u32>, RunError> {
    let machine_image = setup_machine_image(ctx, machine).await?;
    debug!("got machine image");

    let mut rng = rand::rng();

    let machine_id = get_machine_id(machine);
    let machine_dir = ctx.paths().machine_dir(machine_id);
    debug!("going to ensure keypair");
    let ssh_keypair = ensure_keypair(&machine_dir).await?;
    debug!("ensured keypair");

    let cid = rng.random();

    let qemu_launch_opts = QemuLaunchOpts {
        vm: machine.vm.clone(),
        vm_image: machine_image,
        cid,
        volumes: vec![],
        published_ports: vec![],
        show_vm_window: true,
        ssh_pubkey: ssh_keypair.public_key,
        disable_kvm: false,
    };

    let ssh_launch_opts = SshLaunchOpts {
        private_key: ssh_keypair.private_key,
        tty: std::io::stdout().is_terminal(),
        interactive: Interactive::Auto,
        timeout: Duration::from_secs(20),
        env_vars: vec![],
        args: vec!["echo".into(), "hi".into()],
        cid,
        port: None,
    };

    let cancellatation_tokens = CancellationTokens::default();

    let mut joinset: JoinSet<Result<Option<u32>, RunError>> = JoinSet::new();
    joinset.spawn({
        let paths = ctx.paths().clone();
        let executables = ctx.executables().clone();
        let machine_id = machine_id.to_owned();
        let cancellatation_tokens = cancellatation_tokens.clone();
        let qemu_should_exit = Arc::new(AtomicBool::new(false));

        async move {
            debug!("call launch_qemu()");
            launch_qemu(
                &paths,
                &executables,
                &machine_id,
                qemu_launch_opts,
                cancellatation_tokens,
                qemu_should_exit,
                &machine_dir,
            )
            .await?;

            Ok(None)
        }
    });
    joinset.spawn({
        let cancellatation_tokens = cancellatation_tokens.clone();

        async move {
            debug!("call connect_ssh_for_command()");
            let exit_code =
                connect_ssh_for_command(ssh_launch_opts, Some(cancellatation_tokens)).await?;

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
