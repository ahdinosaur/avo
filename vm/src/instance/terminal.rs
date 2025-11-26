use lusid_ssh::{Ssh, SshConnectOptions, SshError};
use std::{net::Ipv4Addr, sync::Arc, time::Duration};
use thiserror::Error;
use tokio::io::{self};

use crate::instance::Instance;

#[derive(Error, Debug)]
pub enum InstanceTerminalError {
    #[error(transparent)]
    Ssh(#[from] SshError),

    #[error(transparent)]
    Io(#[from] io::Error),
}

pub(super) async fn instance_terminal(
    instance: &Instance,
    timeout: Duration,
) -> Result<Option<u32>, InstanceTerminalError> {
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

    let exit_code = ssh.terminal().await?;

    ssh.disconnect().await?;

    Ok(exit_code)
}
