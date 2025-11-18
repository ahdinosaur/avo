use thiserror::Error;

use crate::fs::FsError;

#[derive(Error, Debug)]
pub enum SshError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Fs(#[from] FsError),

    #[error(transparent)]
    Russh(#[from] russh::Error),

    #[error("Timed out connecting to virtual machine via SSH")]
    Timeout,

    #[error("SSH authentication (public key) failed")]
    AuthFailed,

    #[error("SSH keys error: {0}")]
    RusshKey(#[from] russh::keys::ssh_key::Error),

    #[error("failed to parse string as port integer: {0}")]
    ParsePort(#[from] std::num::ParseIntError),
}
