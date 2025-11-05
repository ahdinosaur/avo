// ssh/mod.rs

pub mod error;
pub mod keypair;

use crate::run::CancellationTokens;
use crate::ssh::error::SshError;
use russh::client;
use russh::keys::key::PrivateKeyWithHashAlg;
use russh::keys::PrivateKey;
use russh::ChannelMsg;
use serde::{Deserialize, Serialize};
use std::env;
use std::io::IsTerminal;
use std::sync::Arc;
use std::time::Duration;
use termion::raw::IntoRawMode;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::signal::unix::{signal, SignalKind};
use tokio::time::{sleep, Instant};
use tokio_vsock::{VsockAddr, VsockStream};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Interactive {
    Always,
    Never,
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVar {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SshLaunchOpts {
    #[serde(skip)]
    pub private_key: String,
    pub tty: bool,
    pub interactive: Interactive,
    pub timeout: Duration,
    pub env_vars: Vec<EnvVar>,
    pub args: Vec<String>,
    pub cid: u32,
    pub port: Option<u32>,
}

#[derive(Debug, Clone)]
struct SshClient;

impl client::Handler for SshClient {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

pub async fn connect_ssh_for_command(
    opts: SshLaunchOpts,
    cancellation_tokens: Option<CancellationTokens>,
) -> Result<Option<u32>, SshError> {
    let private_key = PrivateKey::from_openssh(opts.private_key.clone())
        .map_err(|e| SshError::KeyEncoding(e.to_string()))?;

    let cid = opts.cid;
    let port = opts.port.unwrap_or(22);

    // 1) Connect Vsock
    let vsock_addr = VsockAddr::new(cid, port);
    let stream = connect_vsock_with_retry(vsock_addr, opts.timeout).await?;

    // 2) SSH connect
    let config = client::Config {
        keepalive_interval: Some(Duration::from_secs(5)),
        ..<_>::default()
    };
    let config = Arc::new(config);
    let handler = SshClient {};
    let mut handle = client::connect_stream(config, stream, handler.clone()).await?;

    // 3) Authenticate
    let auth = handle
        .authenticate_publickey(
            "root",
            PrivateKeyWithHashAlg::new(Arc::new(private_key), None),
        )
        .await?;
    if !auth.success() {
        return Err(SshError::AuthFailed);
    }

    // 4) Open channel and prepare environment
    let mut channel = handle.channel_open_session().await?;

    // Allocate PTY if requested
    if opts.tty {
        let (cols, rows) = termion::terminal_size().map_err(|_| SshError::TtyUnavailable)?;
        let term = env::var("TERM").unwrap_or_else(|_| "xterm-256color".into());
        channel
            .request_pty(true, &term, cols as u32, rows as u32, 0, 0, &[])
            .await?;
    }

    // Set environment variables
    for ev in &opts.env_vars {
        channel
            .set_env(true, ev.key.clone(), ev.value.clone())
            .await?;
    }

    // 5) Build and execute command
    let command = opts
        .args
        .iter()
        .map(|x| shell_escape::escape(x.clone().into()))
        .collect::<Vec<_>>()
        .join(" ");
    channel.exec(true, command).await?;

    // 6) Optional raw terminal and resize signal
    let _raw_guard = if opts.tty {
        Some(std::io::stdout().into_raw_mode()?)
    } else {
        None
    };
    let mut sig = if opts.tty {
        Some(signal(SignalKind::window_change())?)
    } else {
        None
    };

    // 7) Determine stdin behavior
    let mut stdin = tokio::io::stdin();
    let mut stdin_buf = vec![0u8; 4096];
    let mut stdin_open = match opts.interactive {
        Interactive::Always => true,
        Interactive::Never => false,
        Interactive::Auto => std::io::stdin().is_terminal(),
    };

    let mut stdout = tokio::io::stdout();
    let mut stderr = tokio::io::stderr();

    // 8) Optional cancellation future
    let mut cancel = cancellation_tokens.as_ref().map(|t| t.ssh.cancelled());

    // 9) Drive a small event loop with clear semantics:
    // - Forward SSH stdout/stderr to local stdout/stderr
    // - Forward local stdin to SSH if enabled
    // - Resize PTY on SIGWINCH
    // - Respect external cancellation
    // - Capture exit status and drain until EOF/Close
    let mut exit_code: Option<u32> = None;

    loop {
        tokio::select! {
            // External cancellation
            _ = async { if let Some(c) = &mut cancel { c.await } }, if cancel.is_some() => {
                let _ = channel.eof().await;
                let _ = channel.close().await;
                let _ = handle.disconnect(russh::Disconnect::ByApplication, "", "English").await;
                if let Some(tokens) = &cancellation_tokens {
                    tokens.qemu.cancel();
                }
                return Ok(None);
            }

            // Terminal resize
            _ = async { if let Some(s) = &mut sig { s.recv().await; } }, if sig.is_some() => {
                if let Ok((cols, rows)) = termion::terminal_size() {
                    let _ = channel.window_change(cols as u32, rows as u32, 0, 0).await;
                }
            }

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
                        // keep draining until Eof/Close
                    }
                    Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => {
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    // Close channel and disconnect
    let _ = channel.eof().await;
    let _ = channel.close().await;
    let _ = handle
        .disconnect(russh::Disconnect::ByApplication, "", "English")
        .await;

    Ok(Some(exit_code.unwrap_or(255)))
}

async fn connect_vsock_with_retry(
    addr: VsockAddr,
    timeout: Duration,
) -> Result<VsockStream, SshError> {
    let start = Instant::now();
    let mut backoff_ms = 50u64;

    loop {
        match VsockStream::connect(addr).await {
            Ok(stream) => return Ok(stream),
            Err(ref e)
                if e.raw_os_error() == Some(19)
                    || matches!(
                        e.kind(),
                        std::io::ErrorKind::TimedOut
                            | std::io::ErrorKind::ConnectionRefused
                            | std::io::ErrorKind::ConnectionReset
                    ) =>
            {
                if start.elapsed() > timeout {
                    return Err(SshError::Timeout);
                }
                sleep(Duration::from_millis(backoff_ms)).await;
                backoff_ms = (backoff_ms * 2).min(1_000);
            }
            Err(e) => return Err(SshError::Io(e)),
        }
    }
}
