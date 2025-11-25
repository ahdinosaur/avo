use async_promise::Promise;
use russh::{ChannelMsg, CryptoVec};
use thiserror::Error;
use tokio::sync::mpsc::{self, Receiver};
use tracing::{debug, info, instrument};

use super::SshClientHandle;

/// Command execution specific errors.
#[derive(Error, Debug)]
pub enum SshCommandError {
    #[error("failed to open SSH session channel: {0}")]
    ChannelOpen(#[source] russh::Error),

    #[error("failed to execute remote command `{command}`: {source}")]
    Exec {
        command: String,
        #[source]
        source: russh::Error,
    },

    #[error("I/O error while streaming command data: {0}")]
    Io(#[from] std::io::Error),

    #[error("SSH protocol error: {0}")]
    Russh(#[from] russh::Error),

    #[error("SSH protocol error: {0}")]
    ChannelSend(#[from] mpsc::error::SendError<CryptoVec>),
}

pub struct SshCommandOutput {
    stdout: Receiver<CryptoVec>,
    stderr: Receiver<CryptoVec>,
    exit_code: Promise<u32>,
}

#[instrument(skip(handle), fields(command))]
pub(super) async fn ssh_command(
    handle: &mut SshClientHandle,
    command: &str,
) -> Result<u32, SshCommandError> {
    let mut channel = handle
        .channel_open_session()
        .await
        .map_err(SshCommandError::ChannelOpen)?;

    info!("Executing remote command");

    channel
        .exec(true, command)
        .await
        .map_err(|e| SshCommandError::Exec {
            command: command.to_string(),
            source: e,
        })?;

    let (stdout_tx, stdout_rx) = mpsc::unbounded_channel();
    let (stderr_tx, stderr_rx) = mpsc::unbounded_channel();
    let (resolve_exit_code, exit_code) = async_promise::channel::<u32>();

    loop {
        match channel.wait().await {
            Some(ChannelMsg::Data { data }) => {
                stdout_tx.send(data)?;
            }
            Some(ChannelMsg::ExtendedData { data, ext }) => {
                if ext == 1 {
                    stdout_tx.send(data)?;
                }
            }
            Some(ChannelMsg::ExitStatus { exit_status }) => {
                debug!(exit_status, "Remote process reported exit status");
                resolve_exit_code.into_resolve(exit_status);
            }
            Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => {
                break;
            }
            _ => {}
        }
    }

    info!(exit_code = code, "Remote command completed");

    Ok(code)
}
