use std::{net::Ipv4Addr, sync::Arc, time::Duration};
use thiserror::Error;

use crate::{
    instance::Instance,
    ssh::{ssh_command, ssh_sync, SshCommandOptions, SshConnectOptions, SshError, SshSyncOptions},
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

    let ssh_connect = SshConnectOptions {
        private_key: ssh_keypair.private_key,
        addrs: (Ipv4Addr::LOCALHOST, ssh_port),
        username,
        config: Arc::new(Default::default()),
        timeout,
    };

    for volume in volumes {
        let ssh_sync_options = SshSyncOptions {
            connect: ssh_connect.clone(),
            volume,
            follow_symlinks: true,
        };
        ssh_sync(ssh_sync_options).await?;
    }

    let ssh_command_options = SshCommandOptions {
        connect: ssh_connect.clone(),
        command: command.to_owned(),
    };
    let exit_code = ssh_command(ssh_command_options).await?;

    Ok(exit_code)
}
