use russh::ChannelMsg;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::ToSocketAddrs,
};

use crate::ssh::connect::SshConnectOptions;

use super::{connect::connect_with_retry, SshError};

#[derive(Debug)]
pub struct SshCommandOptions<Addrs>
where
    Addrs: ToSocketAddrs + Clone + Send,
{
    pub connect: SshConnectOptions<Addrs>,
    pub command: String,
}

pub async fn ssh_command<Addrs: ToSocketAddrs + Clone + Send + Sync + 'static>(
    options: SshCommandOptions<Addrs>,
) -> Result<u32, SshError> {
    let SshCommandOptions { connect, command } = options;

    // Create SSH connection
    let handle = connect_with_retry(connect).await?;

    // Open session channel
    let mut channel = handle.channel_open_session().await?;

    // Execute command
    channel.exec(true, command).await?;

    // Local I/O setup
    let mut stdin = tokio::io::stdin();
    let mut stdin_buf = vec![0u8; 4096];
    let mut stdin_open = true;

    let mut stdout = tokio::io::stdout();
    let mut stderr = tokio::io::stderr();

    // Event loop: forward data, gather exit code
    let mut exit_code: Option<u32> = None;

    loop {
        tokio::select! {
            // Local stdin -> remote
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

            // Remote -> local
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
                    }
                    Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => {
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    // Cleanly close channel and disconnect
    let _ = channel.eof().await;
    let _ = channel.close().await;
    let _ = handle
        .disconnect(russh::Disconnect::ByApplication, "", "English")
        .await;

    Ok(exit_code.unwrap_or(255))
}
