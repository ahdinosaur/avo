mod command;
mod connect;
mod error;
mod keypair;
mod sync;

pub use connect::SshConnectOptions;
pub use error::SshError;
pub use keypair::SshKeypair;
use russh::client::Handle;
use tokio::net::ToSocketAddrs;

use crate::{
    ssh::connect::{connect_with_retry, SshClient},
    VmVolume,
};

use self::{command::ssh_command, sync::ssh_sync};

type SshClientHandle = Handle<SshClient>;

pub struct Ssh {
    handle: SshClientHandle,
}

impl Ssh {
    pub async fn connect<Addrs>(options: SshConnectOptions<Addrs>) -> Result<Self, SshError>
    where
        Addrs: ToSocketAddrs + Clone + Send,
    {
        let handle = connect_with_retry(options).await?;
        Ok(Self { handle })
    }

    pub async fn command(&mut self, command: &str) -> Result<u32, SshError> {
        ssh_command(&mut self.handle, command).await
    }

    pub async fn sync(&mut self, volume: VmVolume) -> Result<(), SshError> {
        ssh_sync(&mut self.handle, volume).await
    }
}
