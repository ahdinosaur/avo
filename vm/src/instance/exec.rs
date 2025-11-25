use lusid_ssh::{Ssh, SshConnectOptions, SshError, SshVolume};
use std::{net::Ipv4Addr, sync::Arc, time::Duration};
use thiserror::Error;
use tokio::io::{self, copy};
use tracing::info;

use crate::instance::Instance;

#[derive(Error, Debug)]
pub enum InstanceExecError {
    #[error(transparent)]
    Ssh(#[from] SshError),

    #[error(transparent)]
    Io(#[from] io::Error),
}

pub(super) async fn instance_exec(
    instance: &Instance,
    command: &str,
    volumes: Vec<SshVolume>,
    timeout: Duration,
) -> Result<Option<u32>, InstanceExecError> {
    let ssh_keypair = instance.ssh_keypair().await.map_err(SshError::Keypair)?;
    let ssh_port = instance.ssh_port;
    let username = instance.user.clone();

    let mut ssh = Ssh::connect(SshConnectOptions {
        private_key: ssh_keypair.private_key,
        addrs: (Ipv4Addr::LOCALHOST, ssh_port),
        username,
        config: Arc::new(Default::default()),
        timeout,
    })
    .await?;

    for volume in volumes {
        info!("ssh.sync: {:?}", volume);
        ssh.sync(volume).await?;
    }

    info!("ssh.command: {}", command);
    let mut handle = ssh.command(command).await?;

    {
        let mut channel_stdout = &mut handle.stdout;
        let mut terminal_stdout = tokio::io::stdout();
        let stdout_future = copy(&mut channel_stdout, &mut terminal_stdout);
        tokio::pin!(stdout_future);

        let mut channel_stderr = &mut handle.stderr;
        let mut terminal_stderr = tokio::io::stderr();
        let stderr_future = copy(&mut channel_stderr, &mut terminal_stderr);
        tokio::pin!(stderr_future);
        tokio::select! {
            res = stdout_future => {
                let _ = res?;
            },
            res = stderr_future => {
                let _ = res?;
            },
        };
    }

    let exit_code = handle.wait().await?;

    ssh.disconnect().await?;

    Ok(exit_code)
}
