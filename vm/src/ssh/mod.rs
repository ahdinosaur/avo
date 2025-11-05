use std::fmt::Debug;
use std::future::pending;
use std::io::ErrorKind;
use std::{env, sync::Arc, time::Duration};

use russh::keys::PrivateKey;
use russh::{keys::key::PrivateKeyWithHashAlg, ChannelMsg, Disconnect};
use serde::{Deserialize, Serialize};
use termion::raw::IntoRawMode;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::Instant;
use tokio_fd::AsyncFd;
use tokio_vsock::{VsockAddr, VsockStream};

use crate::run::CancellationTokens;
use crate::ssh::error::SshError;

mod error;
mod keypair;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Interactive {
    Always,
    Never,
    Auto,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SshLaunchOpts {
    #[serde(skip)]
    pub private_key: String,
    pub tty: bool,
    pub interactive: Interactive,
    pub timeout: Duration,
    // pub env_vars: Vec<EnvVar>,
    pub args: Vec<String>,
    pub cid: u32,
}

#[derive(Debug, Clone)]
struct SshClient {}

impl russh::client::Handler for SshClient {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::PublicKey,
    ) -> std::result::Result<bool, Self::Error> {
        Ok(true)
    }
}

/// This struct is a convenience wrapper around a russh client that handles the
/// input/output event loop
pub struct Session {
    session: russh::client::Handle<SshClient>,
    tty_state: Pty,
}

/// State for handling a pseudo-tty allocated by SSH, if enabled.
enum Pty {
    Enabled {
        /// Last known size of the user's terminal on the host, outside the VM.
        host_terminal_size: (u16, u16),
    },
    Disabled,
}

impl Pty {
    fn is_enabled(&self) -> bool {
        matches!(self, Pty::Enabled { .. })
    }
}

/// Functionality for asynchronously reading from stdin, if enabled.
enum StdinReader {
    Enabled { fd: AsyncFd, buf: Vec<u8> },
    Closed,
    Disabled,
}

impl StdinReader {
    /// If reading from stdin is enabled, try to read into its buffer and return
    /// the bytes read along with a reference to the buffer. This allows us to
    /// conveniently do an optional read in tokio's select! macro.
    async fn maybe_read(&mut self) -> Option<(std::io::Result<usize>, &[u8])> {
        match self {
            StdinReader::Enabled { fd, buf } => Some((fd.read(buf).await, buf)),
            StdinReader::Disabled | StdinReader::Closed => None,
        }
    }
}

impl Session {
    async fn connect(
        privkey: PrivateKey,
        cid: u32,
        port: u32,
        timeout: Duration,
        allocate_tty: bool,
    ) -> Result<Self> {
        let config = russh::client::Config {
            keepalive_interval: Some(Duration::from_secs(5)),
            ..<_>::default()
        };

        let config = Arc::new(config);
        let sh = SshClient {};

        let vsock_addr = VsockAddr::new(cid, port);
        let now = Instant::now();
        println!("Connecting to SSH via vsock");
        let mut session = loop {
            tokio::time::sleep(Duration::from_millis(100)).await;

            // Establish vsock connection
            let stream = match VsockStream::connect(vsock_addr).await {
                Ok(stream) => stream,
                Err(ref e) if e.raw_os_error() == Some(19) => {
                    // "No such device"
                    if now.elapsed() > timeout {
                        eprintln!("Timeout trying to connect to VM via SSH");
                        return Err(SshError::Timeout);
                    }
                    continue;
                }
                Err(ref e) => match e.kind() {
                    ErrorKind::TimedOut
                    | ErrorKind::ConnectionRefused
                    | ErrorKind::ConnectionReset => {
                        if now.elapsed() > timeout {
                            eprintln!("Timeout trying to connect to VM via SSH");
                            return Err(SshError::Timeout);
                        }
                        continue;
                    }
                    _ => {
                        eprintln!("Unhandled vsock error: {e}");
                        return Err(SshError::Io(std::io::Error::new(e.kind(), e.to_string())));
                    }
                },
            };

            // Connect to SSH via vsock stream
            match russh::client::connect_stream(config.clone(), stream, sh.clone()).await {
                Ok(x) => break x,
                Err(russh::Error::IO(ref e)) => match e.kind() {
                    ErrorKind::ConnectionRefused | ErrorKind::ConnectionReset => {
                        if now.elapsed() > timeout {
                            eprintln!("Timeout trying to connect to VM via SSH");
                            return Err(SshError::Timeout);
                        }
                    }
                    _ => {
                        eprintln!("Unhandled SSH IO error: {e}");
                        return Err(SshError::Russh(russh::Error::IO(std::io::Error::new(
                            e.kind(),
                            e.to_string(),
                        ))));
                    }
                },
                Err(russh::Error::Disconnect) => {
                    if now.elapsed() > timeout {
                        eprintln!("Timeout trying to connect to VM via SSH");
                        return Err(SshError::Timeout);
                    }
                }
                Err(e) => {
                    eprintln!("Unhandled SSH error: {e}");
                    return Err(SshError::Russh(e));
                }
            }
        };
        println!("Authenticating via SSH");

        // use publickey authentication
        let auth_res = session
            .authenticate_publickey("root", PrivateKeyWithHashAlg::new(Arc::new(privkey), None))
            .await?;

        if !auth_res.success() {
            return Err(SshError::AuthFailed);
        }

        let tty_state = if allocate_tty {
            Pty::Enabled {
                host_terminal_size: termion::terminal_size()
                    .map_err(|_| SshError::TtyUnavailable)?,
            }
        } else {
            Pty::Disabled
        };

        Ok(Self { session, tty_state })
    }

    async fn call(
        &mut self,
        interactive: Interactive,
        // env: Vec<EnvVar>,
        command: &str,
    ) -> Result<u32> {
        let mut channel = self.session.channel_open_session().await?;

        if let Pty::Enabled { host_terminal_size } = &self.tty_state {
            // Request an interactive PTY from the server
            channel
                .request_pty(
                    true,
                    &env::var("TERM").unwrap_or("xterm-256color".into()),
                    host_terminal_size.0 as u32,
                    host_terminal_size.1 as u32,
                    0,
                    0,
                    &[], // ideally pass actual terminal modes here
                )
                .await?;
        }

        for e in env {
            channel.set_env(true, e.key, e.value).await?;
        }

        channel.exec(true, command).await?;

        let code;
        let mut stdin_reader = match interactive {
            Interactive::Always => {
                let buf = vec![0; 1024];
                let fd = tokio_fd::AsyncFd::try_from(libc::STDIN_FILENO)
                    .map_err(|e| SshError::StdinSetup(e.to_string()))?;
                StdinReader::Enabled { fd, buf }
            }
            Interactive::Never => StdinReader::Disabled,
            Interactive::Auto => {
                let fd = tokio_fd::AsyncFd::try_from(libc::STDIN_FILENO);
                if let Ok(fd) = fd {
                    let buf = vec![0; 1024];
                    StdinReader::Enabled { fd, buf }
                } else {
                    StdinReader::Disabled
                }
            }
        };
        let mut stdout = tokio::io::stdout();
        let mut stderr = tokio::io::stderr();

        loop {
            tokio::select! {
                // Handle terminal resize
                _ = tokio::time::sleep(Duration::from_millis(500)), if self.tty_state.is_enabled() => {
                    if let Pty::Enabled { host_terminal_size } = &self.tty_state {
                        let new_terminal_size = termion::terminal_size()?;
                        if host_terminal_size != &new_terminal_size {
                            println!("Terminal size change detected");
                            self.tty_state = Pty::Enabled { host_terminal_size: new_terminal_size };
                            channel.window_change(
                                new_terminal_size.0 as u32,
                                new_terminal_size.1 as u32,
                                0,
                                0
                            ).await?;
                        }
                    }
                },
                // There's terminal input available from the user
                Some((read_bytes, buf)) = stdin_reader.maybe_read() => {
                    match read_bytes {
                        Ok(0) => {
                            stdin_reader = StdinReader::Closed;
                            channel.eof().await?;
                        },
                        Ok(n) => channel.data(&buf[..n]).await?,
                        Err(e) => return Err(e.into()),
                    };
                },
                // There's an event available on the session channel
                Some(msg) = channel.wait() => {
                    match msg {
                        ChannelMsg::Data { ref data } => {
                            stdout.write_all(data).await?;
                            stdout.flush().await?;
                        }
                        ChannelMsg::ExtendedData { ref data, ext } => {
                            if ext == 1 {
                                stderr.write_all(data).await?;
                                stderr.flush().await?;
                            }
                        }
                        ChannelMsg::ExitStatus { exit_status } => {
                            code = exit_status;
                            match stdin_reader {
                                StdinReader::Enabled { .. } => channel.eof().await?,
                                StdinReader::Closed => {},
                                StdinReader::Disabled => channel.eof().await?,
                            };
                            break;
                        }
                        _ => {}
                    }
                },
            }
        }
        Ok(code)
    }

    async fn close(&mut self) -> Result<()> {
        self.session
            .disconnect(Disconnect::ByApplication, "", "English")
            .await?;
        Ok(())
    }
}

/// Connect SSH and run a user-provided command.
///
/// If requested, this will be an interactive session.
///
/// The `cancellation_tokens` are used to cancel a running QEMU task in case
/// there's a problem with the SSH connection or upon command completion. The
/// QEMU task can also use it to cancel the SSH task.
pub async fn connect_ssh_for_command(
    ssh_launch_opts: SshLaunchOpts,
    cancellation_tokens: Option<CancellationTokens>,
) -> Result<Option<u32>> {
    let privkey = PrivateKey::from_openssh(ssh_launch_opts.privkey)
        .map_err(|e| SshError::KeyEncoding(e.to_string()))?;

    // Session is a wrapper around a russh client.
    let mut ssh = Session::connect(
        privkey,
        ssh_launch_opts.cid,
        22,
        ssh_launch_opts.timeout,
        ssh_launch_opts.tty,
    )
    .await
    .inspect_err(|_| {
        if let Some(cancellation_tokens) = &cancellation_tokens {
            cancellation_tokens.qemu.cancel();
        }
    })?;
    println!("Connected via SSH");

    let exit_code = {
        let _raw_term = if ssh_launch_opts.tty {
            Some(std::io::stdout().into_raw_mode()?)
        } else {
            None
        };

        let escaped_args = &ssh_launch_opts
            .args
            .into_iter()
            .map(|x| shell_escape::escape(x.into()))
            .collect::<Vec<_>>()
            .join(" ");

        let cancel_future = if let Some(tokens) = &cancellation_tokens {
            tokens.ssh.cancelled()
        } else {
            pending()
        };

        let ssh_output = tokio::select! {
            _ = cancel_future => {
                println!("SSH task was cancelled");
                return Ok(None)
            }
            val = ssh.call(ssh_launch_opts.interactive, ssh_launch_opts.env_vars, escaped_args) => {
                val
            }
        };
        if let Some(cancellation_tokens) = &cancellation_tokens {
            cancellation_tokens.qemu.cancel();
        }
        ssh_output?
    };

    println!("Exit code: {:?}", exit_code);

    if let Some(cancellation_tokens) = &cancellation_tokens {
        cancellation_tokens.qemu.cancel();
    }

    ssh.close().await?;

    Ok(Some(exit_code))
}
