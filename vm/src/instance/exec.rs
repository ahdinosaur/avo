use std::{net::Ipv4Addr, sync::Arc, time::Duration};
use thiserror::Error;
use tracing::info;

use crate::{
    instance::Instance,
    ssh::{Ssh, SshConnectOptions, SshError},
};

#[derive(Error, Debug)]
pub enum InstanceExecError {
    #[error(transparent)]
    Ssh(#[from] SshError),
}

pub(super) async fn instance_exec(
    instance: &Instance,
    command: &str,
    timeout: Duration,
) -> Result<u32, InstanceExecError> {
    let ssh_keypair = instance.ssh_keypair().await?;
    let ssh_port = instance.ssh_port;
    let username = instance.user.clone();
    let volumes = instance.volumes.clone();

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
    let exit_code = ssh.command(command).await?;

    ssh.disconnect().await?;

    Ok(exit_code)
}
