use russh::ChannelMsg;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
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

    let mut stdin = tokio::io::stdin();
    let mut stdin_buf = vec![0u8; 4096];
    let mut stdin_open = true;

    let mut stdout = tokio::io::stdout();
    let mut stderr = tokio::io::stderr();

    let mut exit_code: Option<u32> = None;

    loop {
        tokio::select! {
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
                        debug!(exit_status, "Remote process reported exit status");
                    }
                    Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => {
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    let _ = channel.eof().await;
    let _ = channel.close().await;

    // Close the transport after command completion.
    let _ = handle
        .disconnect(russh::Disconnect::ByApplication, "", "English")
        .await;

    let code = exit_code.unwrap_or(255);

    info!(exit_code = code, "Remote command completed");

    Ok(code)
}
