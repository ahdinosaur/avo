use russh::{
    client::{connect_stream, Config, Handle, Handler},
    keys::{PrivateKey, PrivateKeyWithHashAlg},
};
use std::{sync::Arc, time::Duration};
use thiserror::Error;
use tokio::{
    net::{TcpStream, ToSocketAddrs},
    time::{sleep, Instant},
};

#[derive(Debug, Clone)]
pub struct SshConnectOptions<Addrs>
where
    Addrs: ToSocketAddrs + Clone + Send,
{
    pub private_key: PrivateKey,
    pub addrs: Addrs,
    pub username: String,
    pub config: Arc<Config>,
    pub timeout: Duration,
}

#[derive(Debug, Clone)]
pub(super) struct SshClient;

impl Handler for SshClient {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

#[derive(Error, Debug)]
pub enum SshConnectError {
    #[error("timed out connecting to SSH server")]
    Timeout,

    #[error("SSH authentication failed (public key)")]
    AuthFailed,

    #[error("TCP error while connecting: {0}")]
    Tcp(#[from] std::io::Error),

    #[error("SSH protocol error: {0}")]
    Russh(#[from] russh::Error),
}

#[tracing::instrument(skip(options))]
pub(super) async fn connect_with_retry<Addrs>(
    options: SshConnectOptions<Addrs>,
) -> Result<Handle<SshClient>, SshConnectError>
where
    Addrs: ToSocketAddrs + Clone + Send,
{
    let SshConnectOptions {
        private_key,
        addrs,
        username,
        config,
        timeout,
    } = options;

    let handler = SshClient;
    let start = Instant::now();

    tracing::info!("Connecting to SSH");

    let mut handle = loop {
        // TCP connect
        let stream = match TcpStream::connect(addrs.clone()).await {
            Ok(stream) => Ok::<Option<TcpStream>, SshConnectError>(Some(stream)),
            Err(ref error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::ConnectionRefused
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::NotFound
                ) =>
            {
                if start.elapsed() > timeout {
                    tracing::warn!("Connect retry timeout exceeded");
                    return Err(SshConnectError::Timeout);
                }
                tracing::debug!(
                    err = %error,
                    elapsed_ms = start.elapsed().as_millis(),
                    "TCP connect not ready yet; will retry"
                );
                Ok(None)
            }
            Err(error) => {
                tracing::warn!(err = %error, "Non-retryable TCP error");
                return Err(SshConnectError::from(error));
            }
        }?;

        // SSH handshake
        if let Some(stream) = stream {
            match connect_stream(config.clone(), stream, handler.clone()).await {
                Ok(handle) => {
                    tracing::trace!("SSH transport established");
                    break handle;
                }
                Err(russh::Error::IO(ref error))
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::ConnectionReset
                    ) =>
                {
                    if start.elapsed() > timeout {
                        tracing::warn!("Handshake retry timeout exceeded");
                        return Err(SshConnectError::Timeout);
                    }
                    tracing::debug!(
                        err = %error,
                        elapsed_ms = start.elapsed().as_millis(),
                        "SSH handshake not ready; will retry"
                    );
                }
                Err(error) => {
                    tracing::warn!(err = %error, "Non-retryable SSH handshake error");
                    return Err(SshConnectError::from(error));
                }
            };
        }

        sleep(Duration::from_millis(100)).await;
    };

    // Public key authentication
    tracing::debug!(username = %username, "Authenticating over SSH");
    let auth = handle
        .authenticate_publickey(
            &username,
            PrivateKeyWithHashAlg::new(Arc::new(private_key), None),
        )
        .await?;
    if !auth.success() {
        tracing::warn!("SSH authentication failed");
        return Err(SshConnectError::AuthFailed);
    }

    tracing::info!("SSH authentication successful");
    Ok(handle)
}
