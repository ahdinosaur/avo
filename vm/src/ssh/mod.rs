use async_ssh2_russh::{russh::client, AsyncSession, Config, NoCheckHandler};
use std::net::ToSocketAddrs;
use tokio::io::AsyncBufReadExt;

use tracing::info;

use crate::{run::CancellationTokens, ssh::error::SshError};

pub mod error;
pub mod keypair;

pub struct SshLaunchOpts<Addr>
where
    Addr: ToSocketAddrs,
{
    pub addrs: Addr,
    pub username: String,
    pub private_key: String,
    pub config: Config,
    pub command: String,
}

pub async fn ssh_command<Addrs>(
    opts: SshLaunchOpts<Addrs>,
    _cancellation_tokens: Option<CancellationTokens>,
) -> Result<u32, SshError>
where
    Addrs: ToSocketAddrs,
{
    let SshLaunchOpts {
        addrs,
        username,
        private_key,
        config,
        command,
    } = opts;

    let client = ssh_connect_with_retry(addrs, username, config).await?;

    let result = client.execute(&command).await?;

    info!("SSH stdout: {}", result.stdout);
    info!("SSH stderr: {}", result.stderr);

    Ok(result.exit_status)
}

async fn ssh_connect_with_retry<Addrs>(
    addrs: Addrs,
    username: String,
    config: Config,
) -> Result<AsyncSession<NoCheckHandler>, SshError>
where
    Addrs: ToSocketAddrs,
{
    loop {
        match AsyncSession::connect_publickey(
            config,
            addrs,
            username,
            addr.clone(),
            &username,
            auth.clone(),
            server_check.clone(),
            config.clone(),
        )
        .await
        {
            Ok(client) => return Ok(client),
            Err(AsyncSshError::SshError(russh::Error::IO(e))) => {
                match e.kind() {
                    // The VM is still booting at this point so we're just ignoring these errors
                    // for some time.
                    std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::ConnectionReset => {
                        continue;
                    }
                    _ => return Err(SshError::Ssh(AsyncSshError::SshError(russh::Error::IO(e)))),
                }
            }
            Err(AsyncSshError::SshError(russh::Error::Disconnect)) => {
                continue;
            }
            Err(error) => return Err(SshError::Ssh(error)),
        }
    }
}
