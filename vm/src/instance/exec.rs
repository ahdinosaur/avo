use std::{net::Ipv4Addr, time::Duration};
use thiserror::Error;

use crate::{
    instance::Instance,
    ssh::{error::SshError, ssh_command, SshCommandOptions},
};

#[derive(Error, Debug)]
pub enum InstanceExecError {
    #[error(transparent)]
    Ssh(#[from] SshError),
}

pub async fn instance_exec(instance: &Instance, command: &str) -> Result<u32, InstanceExecError> {
    let ssh_keypair = instance.ssh_keypair().await?;
    let ssh_port = instance.ssh_port;
    let username = instance.user.clone();

    let ssh_launch_opts = SshCommandOptions {
        private_key: ssh_keypair.private_key,
        addrs: (Ipv4Addr::LOCALHOST, ssh_port),
        username,
        config: Default::default(),
        command: command.to_owned(),
        timeout: Duration::from_secs(120),
    };

    let exit_code = ssh_command(ssh_launch_opts).await?;

    Ok(exit_code)
}
