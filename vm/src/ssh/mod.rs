use russh::client;
use russh::keys::key::PrivateKeyWithHashAlg;
use russh::keys::PrivateKey;
use russh::ChannelMsg;
use std::io::{self, ErrorKind};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{self as aio, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, ToSocketAddrs as IntoSocketAddrs};
use tokio::time::timeout;

use crate::run::CancellationTokens;
use crate::ssh::error::SshError;

pub mod error;
pub mod keypair;

#[derive(Debug)]
pub struct SshLaunchOpts<Addrs>
where
    Addrs: IntoSocketAddrs + Send,
{
    pub private_key: String,
    pub addrs: Addrs,
    pub username: String,
    pub config: client::Config,
    pub timeout: Duration,
    pub command: String,
}

#[derive(Debug, Clone)]
struct SshClient;

impl client::Handler for SshClient {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

pub async fn ssh_command<Addrs>(
    opts: SshLaunchOpts<Addrs>,
    cancellation: Option<CancellationTokens>,
) -> Result<Option<u32>, SshError>
where
    Addrs: IntoSocketAddrs + Send,
{
    // Parse private key
    let private_key =
        PrivateKey::from_openssh(opts.private_key).map_err(|e| SshError::Russh(e.into()))?;

    // TCP connect with timeout
    let stream = timeout(opts.timeout, TcpStream::connect(opts.addrs))
        .await
        .map_err(|_| {
            SshError::Russh(io::Error::new(ErrorKind::TimedOut, "TCP connect timeout").into())
        })?
        .map_err(|e| SshError::Russh(e.into()))?;

    // SSH connect
    let config = Arc::new(opts.config);
    let mut handle = client::connect_stream(config, stream, SshClient)
        .await
        .map_err(SshError::Russh)?;

    // Authenticate
    let auth = handle
        .authenticate_publickey(
            &opts.username,
            PrivateKeyWithHashAlg::new(Arc::new(private_key), None),
        )
        .await
        .map_err(SshError::Russh)?;
    if !auth.success() {
        return Err(SshError::Russh(
            io::Error::new(ErrorKind::PermissionDenied, "authentication failed").into(),
        ));
    }

    // Open session and exec
    let mut channel = handle
        .channel_open_session()
        .await
        .map_err(SshError::Russh)?;
    channel
        .exec(true, opts.command)
        .await
        .map_err(SshError::Russh)?;

    // Plain stdin/stdout/stderr forwarding
    let mut stdin = aio::stdin();
    let mut stdout = aio::stdout();
    let mut stderr = aio::stderr();
    let mut stdin_buf = [0u8; 8192];
    let cancel = cancellation.as_ref().map(|t| t.ssh.clone());

    let mut exit_code: Option<u32> = None;

    loop {
        tokio::select! {
          // Optional cancellation
          _ = async { if let Some(c) = &cancel { c.cancelled().await } }, if cancel.is_some() => {
            let _ = channel.eof().await;
            let _ = channel.close().await;
            let _ = handle
              .disconnect(russh::Disconnect::ByApplication, "", "English")
              .await;
            return Ok(None);
          }

          // Local stdin -> remote
          read = stdin.read(&mut stdin_buf) => {
            match read {
              Ok(0) => {
                let _ = channel.eof().await;
              }
              Ok(n) => {
                channel.data(&stdin_buf[..n]).await.map_err(SshError::Russh)?;
              }
              Err(e) => {
                let _ = channel.eof().await;
                return Err(SshError::Russh(e.into()));
              }
            }
          }

          // Remote -> local
          msg = channel.wait() => {
            match msg {
              Some(ChannelMsg::Data { data }) => {
                stdout.write_all(&data).await?;
                stdout.flush().await?;
              }
              Some(ChannelMsg::ExtendedData { data, ext }) => {
                if ext == 1 {
                  stderr.write_all(&data).await?;
                  stderr.flush().await?;
                }
              }
              Some(ChannelMsg::ExitStatus { exit_status }) => {
                exit_code = Some(exit_status);
              }
              Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => {
                break;
              }
              _ => {}
            }
          }
        }
    }

    // Clean up
    let _ = channel.eof().await;
    let _ = channel.close().await;
    let _ = handle
        .disconnect(russh::Disconnect::ByApplication, "", "English")
        .await;

    Ok(exit_code)
}
