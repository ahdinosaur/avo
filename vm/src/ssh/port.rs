use std::{
    fmt::Display,
    net::{Ipv4Addr, SocketAddrV4, TcpListener},
    path::Path,
    str::FromStr,
};

use crate::{fs, ssh::error::SshError};

const SSH_PORT_FILE: &str = "ssh-port";

#[derive(Debug, Copy, Clone)]
pub struct SshPort(u16);

impl Display for SshPort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for SshPort {
    type Err = <u16 as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(u16::from_str(s)?))
    }
}

impl SshPort {
    pub async fn load_or_create(directory: &Path) -> Result<Self, SshError> {
        if Self::exists(directory).await? {
            return Self::load(directory).await;
        }

        let port = Self::create()?;

        port.save(directory).await?;

        Ok(port)
    }

    pub fn create() -> Result<Self, SshError> {
        let port = get_free_tcp_port().ok_or(SshError::NoOpenPortsAvailable)?;
        Ok(Self(port))
    }

    pub async fn save(&self, directory: &Path) -> Result<(), SshError> {
        fs::setup_directory_access(directory).await?;

        let port_path = directory.join(SSH_PORT_FILE);
        let port_string = self.to_string();

        fs::write_file(&port_path, port_string.as_bytes()).await?;

        Ok(())
    }

    pub async fn exists(directory: &Path) -> Result<bool, SshError> {
        let port_path = directory.join(SSH_PORT_FILE);
        let port_exists = fs::path_exists(&port_path).await?;
        Ok(port_exists)
    }

    pub async fn load(directory: &Path) -> Result<Self, SshError> {
        let port_path = directory.join(SSH_PORT_FILE);
        let port_string = fs::read_file_to_string(&port_path).await?;
        let port = SshPort::from_str(&port_string)?;
        Ok(port)
    }

    pub fn as_u16(&self) -> u16 {
        self.0
    }
}
