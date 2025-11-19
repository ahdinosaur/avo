mod command;
mod connect;
mod keypair;
mod sync;

pub use self::command::SshCommandError;
pub use self::connect::{SshConnectError, SshConnectOptions};
pub use self::keypair::{SshKeypair, SshKeypairError};
pub use self::sync::SshSyncError;

use russh::client::Handle;
use thiserror::Error;
use tokio::net::ToSocketAddrs;

use crate::{
    ssh::connect::{connect_with_retry, SshClient},
    VmVolume,
};

type SshClientHandle = Handle<SshClient>;

#[derive(Error, Debug)]
pub enum SshError {
    #[error(transparent)]
    Connect(#[from] SshConnectError),

    #[error(transparent)]
    Command(#[from] SshCommandError),

    #[error(transparent)]
    Sync(#[from] SshSyncError),

    #[error(transparent)]
    Keypair(#[from] SshKeypairError),

    #[error("failed to disconnect: {error}")]
    Disconnect {
        #[source]
        error: russh::Error,
    },
}

pub struct Ssh {
    handle: SshClientHandle,
}

impl Ssh {
    /// Establish an SSH connection using the provided options.
    #[tracing::instrument(skip(options), level = tracing::Level::DEBUG)]
    pub async fn connect<Addrs>(options: SshConnectOptions<Addrs>) -> Result<Self, SshError>
    where
        Addrs: ToSocketAddrs + Clone + Send,
    {
        let handle = connect_with_retry(options).await?;
        Ok(Self { handle })
    }

    /// Execute a remote command, streaming stdio to the current process.
    #[tracing::instrument(skip(self), level = tracing::Level::DEBUG)]
    pub async fn command(&mut self, command: &str) -> Result<u32, SshError> {
        command::ssh_command(&mut self.handle, command)
            .await
            .map_err(Into::into)
    }

    /// Open SFTP and upload a volume (file or directory).
    #[tracing::instrument(skip(self), level = tracing::Level::DEBUG)]
    pub async fn sync(&mut self, volume: VmVolume) -> Result<(), SshError> {
        sync::ssh_sync(&mut self.handle, volume)
            .await
            .map_err(Into::into)
    }

    /// Disconnect from the SSH server.
    #[tracing::instrument(skip(self), level = tracing::Level::DEBUG)]
    pub async fn disconnect(&mut self) -> Result<(), SshError> {
        self.handle
            .disconnect(russh::Disconnect::ByApplication, "", "English")
            .await
            .map_err(|error| SshError::Disconnect { error })
    }
}
