pub mod error;
pub mod keypair;
pub mod port;

use russh::{
    client::{connect_stream, Config, Handler},
    keys::{key::PrivateKeyWithHashAlg, PrivateKey},
    ChannelMsg,
};
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpStream, ToSocketAddrs},
    time::{sleep, Instant},
};

use crate::ssh::error::SshError;

#[derive(Debug)]
pub struct SshLaunchOpts<Addrs>
where
    Addrs: ToSocketAddrs + Clone + Send,
{
    pub private_key: PrivateKey,
    pub addrs: Addrs,
    pub username: String,
    pub config: Config,
    pub timeout: Duration,
    pub command: String,
}

#[derive(Debug, Clone)]
struct SshClient;

impl Handler for SshClient {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

pub async fn ssh_command<Addrs: ToSocketAddrs + Clone + Send + Sync + 'static>(
    opts: SshLaunchOpts<Addrs>,
) -> Result<u32, SshError> {
    // Establish TCP connection with retry/backoff
    let stream = connect_tcp_with_retry(opts.addrs.clone(), opts.timeout).await?;

    // Create SSH client and connect
    let config = Arc::new(opts.config);
    let handler = SshClient {};
    let mut handle = connect_stream(config, stream, handler.clone()).await?;

    // Authenticate using the provided private key and username
    let auth = handle
        .authenticate_publickey(
            &opts.username,
            PrivateKeyWithHashAlg::new(Arc::new(opts.private_key), None),
        )
        .await?;
    if !auth.success() {
        return Err(SshError::AuthFailed);
    }

    // Open session channel
    let mut channel = handle.channel_open_session().await?;

    // Execute command
    channel.exec(true, opts.command.clone()).await?;

    // Local I/O setup
    let mut stdin = tokio::io::stdin();
    let mut stdin_buf = vec![0u8; 4096];
    let mut stdin_open = true;

    let mut stdout = tokio::io::stdout();
    let mut stderr = tokio::io::stderr();

    // Event loop: forward data, gather exit code
    let mut exit_code: Option<u32> = None;

    loop {
        tokio::select! {
            // Local stdin -> remote
            read = stdin.read(&mut stdin_buf), if stdin_open => {
                match read {
                    Ok(0) => {
                        stdin_open = false;
                        let _ = channel.eof().await;
                    }
                    Ok(n) => {
                        channel.data(&stdin_buf[..n]).await?;
                    }
                    Err(e) => {
                        stdin_open = false;
                        let _ = channel.eof().await;
                        eprintln!("stdin read error: {e}");
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

    // Cleanly close channel and disconnect
    let _ = channel.eof().await;
    let _ = channel.close().await;
    let _ = handle
        .disconnect(russh::Disconnect::ByApplication, "", "English")
        .await;

    Ok(exit_code.unwrap_or(255))
}

async fn connect_tcp_with_retry<Addrs>(
    addrs: Addrs,
    timeout: Duration,
) -> Result<TcpStream, SshError>
where
    Addrs: ToSocketAddrs + Clone + Send,
{
    let start = Instant::now();
    let mut backoff_ms = 50u64;

    loop {
        match TcpStream::connect(addrs.clone()).await {
            Ok(stream) => return Ok(stream),
            Err(ref e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::ConnectionRefused
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::NotFound
                ) =>
            {
                if start.elapsed() > timeout {
                    return Err(SshError::Timeout);
                }
                sleep(Duration::from_millis(backoff_ms)).await;
                backoff_ms = (backoff_ms * 2).min(1_000);
            }
            Err(e) => return Err(SshError::Io(e)),
        }
    }
}
