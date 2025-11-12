use async_ssh2_tokio::{
    AuthMethod, Client, Config, Error as AsyncSshError, ServerCheckMethod,
    ToSocketAddrsWithHostname,
};
use tracing::info;

use crate::{run::CancellationTokens, ssh::error::SshError};

pub mod error;
pub mod keypair;

pub struct SshLaunchOpts<Addr>
where
    Addr: ToSocketAddrsWithHostname,
{
    pub addr: Addr,
    pub username: String,
    pub private_key: String,
    pub config: async_ssh2_tokio::Config,
    pub command: String,
}

pub async fn ssh_command<Addr>(
    opts: SshLaunchOpts<Addr>,
    _cancellation_tokens: Option<CancellationTokens>,
) -> Result<u32, SshError>
where
    Addr: ToSocketAddrsWithHostname,
{
    let SshLaunchOpts {
        addr,
        username,
        private_key,
        config,
        command,
    } = opts;

    let auth = AuthMethod::with_key(&private_key, None);
    let server_check = async_ssh2_tokio::ServerCheckMethod::NoCheck;

    let client = ssh_connect_with_retry(addr, &username, auth, server_check, config).await?;

    let result = client.execute(&command).await?;

    info!("SSH stdout: {}", result.stdout);
    info!("SSH stderr: {}", result.stderr);

    Ok(result.exit_status)
}

async fn ssh_connect_with_retry<Addr>(
    addr: Addr,
    username: &str,
    auth: AuthMethod,
    server_check: ServerCheckMethod,
    config: Config,
) -> Result<Client, SshError>
where
    Addr: ToSocketAddrsWithHostname + Clone,
{
    loop {
        match Client::connect_with_config(
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
