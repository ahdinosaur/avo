use thiserror::Error;

use crate::fs::FsError;

#[derive(Error, Debug)]
pub enum SshError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Fs(#[from] FsError),

    #[error("SSH error: {0}")]
    Ssh(#[from] async_ssh2_tokio::Error),

    #[error("SSH key encoding error: {0}")]
    KeyEncoding(String),
}
