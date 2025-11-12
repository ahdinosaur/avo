use thiserror::Error;

use crate::fs::FsError;

#[derive(Error, Debug)]
pub enum SshError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Fs(#[from] FsError),

    #[error("SSH error: {0}")]
    Russh(#[from] russh::Error),

    #[error("Timed out connecting to virtual machine via SSH")]
    Timeout,

    #[error("SSH authentication (public key) failed")]
    AuthFailed,

    #[error("SSH key encoding error: {0}")]
    KeyEncoding(String),
}
