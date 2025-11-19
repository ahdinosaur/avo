use russh::{
    client::{connect_stream, Config, Handle, Handler},
    keys::{PrivateKey, PrivateKeyWithHashAlg},
};
use std::{sync::Arc, time::Duration};
use tokio::{
    net::{TcpStream, ToSocketAddrs},
    time::{sleep, Instant},
};

use crate::ssh::error::SshError;

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

pub(super) async fn connect_with_retry<Addrs>(
    options: SshConnectOptions<Addrs>,
) -> Result<Handle<SshClient>, SshError>
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

    let mut handle = loop {
        let stream = match TcpStream::connect(addrs.clone()).await {
            Ok(stream) => Ok::<Option<TcpStream>, SshError>(Some(stream)),
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
                    return Err(SshError::Timeout);
                }
                Ok(None)
            }
            Err(error) => return Err(SshError::from(error)),
        }?;

        if let Some(stream) = stream {
            match connect_stream(config.clone(), stream, handler.clone()).await {
                Ok(handle) => break handle,
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
    };

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

    Ok(handle)
}
