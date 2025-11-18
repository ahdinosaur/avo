pub mod error;
pub mod keypair;

use russh::{
    client::{connect_stream, Config, Handle, Handler},
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
pub struct SshCommandOptions<Addrs>
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
    options: SshCommandOptions<Addrs>,
) -> Result<u32, SshError> {
    let SshCommandOptions {
        private_key,
        addrs,
        username,
        config,
        timeout,
        command,
    } = options;

    // Create SSH client and connect
    let config = Arc::new(config);
    let handler = SshClient {};
    let mut handle = connect_with_retry(addrs, config, handler, timeout).await?;

    // Authenticate using the provided private key and username
    let auth = handle
        .authenticate_publickey(
            &username,
            PrivateKeyWithHashAlg::new(Arc::new(private_key), None),
        )
        .await?;
    if !auth.success() {
        return Err(SshError::AuthFailed);
    }

    // Open session channel
    let mut channel = handle.channel_open_session().await?;

    // Execute command
    channel.exec(true, command).await?;

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

async fn connect_with_retry<Addrs, H>(
    addrs: Addrs,
    config: Arc<Config>,
    handler: H,
    timeout: Duration,
) -> Result<Handle<H>, SshError>
where
    Addrs: ToSocketAddrs + Clone + Send,
    H: Handler<Error = russh::Error> + Clone + Send + 'static,
{
    let start = Instant::now();

    loop {
        let stream = match TcpStream::connect(addrs.clone()).await {
            Ok(stream) => Ok::<Option<TcpStream>, SshError>(Some(stream)),
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
                Ok(None)
            }
            Err(e) => return Err(SshError::Io(e)),
        }?;

        if let Some(stream) = stream {
            match connect_stream(config.clone(), stream, handler.clone()).await {
                Ok(handle) => return Ok(handle),
                Err(russh::Error::IO(ref error))
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::ConnectionReset
                    ) =>
                {
                    if start.elapsed() > timeout {
                        return Err(SshError::Timeout);
                    }
                }
                Err(error) => return Err(SshError::from(error)),
            };
        }

        sleep(Duration::from_millis(100)).await;
    }
}
