use std::fmt::Debug;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::{env, os::unix::fs::PermissionsExt, sync::Arc, time::Duration};

use base64ct::LineEnding;
use russh::keys::ssh_key::{private::Ed25519Keypair, rand_core::OsRng};
use russh::keys::{PrivateKey, PublicKey};
use russh::{keys::key::PrivateKeyWithHashAlg, ChannelMsg, Disconnect};
use serde::{Deserialize, Serialize};
use termion::raw::IntoRawMode;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::Instant;
use tokio_fd::AsyncFd;
use tokio_vsock::{VsockAddr, VsockStream};

use crate::run::CancellationTokens;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Interactive {
    Always,
    Never,
    Auto,
}

#[derive(Error, Debug)]
pub enum SshError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("SSH error: {0}")]
    Russh(#[from] russh::Error),

    #[error("Timed out connecting to virtual machine via SSH")]
    Timeout,

    #[error("SSH authentication (public key) failed")]
    AuthFailed,

    #[error("TTY requested but no terminal is available")]
    TtyUnavailable,

    #[error("Failed to set up stdin for interactive mode: {0}")]
    StdinSetup(String),

    #[error("SSH key encoding error: {0}")]
    KeyEncoding(String),
}

type Result<T> = std::result::Result<T, SshError>;

#[derive(Clone, Debug)]
pub struct GeneratedSshKeypair {
    pub pubkey_str: String,
    pub pubkey_path: PathBuf,
    pub privkey_str: String,
    pub privkey_path: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SshLaunchOpts {
    #[serde(skip)]
    pub privkey: String,
    pub tty: bool,
    pub interactive: Interactive,
    pub timeout: Duration,
    // pub env_vars: Vec<EnvVar>,
    pub args: Vec<String>,
    pub cid: u32,
}

/// Always create a fresh SSH keypair in `dir` and return both the strings and
/// the file paths. This never reuses any system SSH keys.
pub fn create_ssh_keypair(dir: &Path) -> Result<GeneratedSshKeypair> {
    fs::create_dir_all(dir)?;

    let privkey_path = dir.join("id_ed25519");
    let pubkey_path = privkey_path.with_extension("pub");

    // Always generate a new keypair.
    let ed25519_keypair = Ed25519Keypair::random(&mut OsRng);

    let pubkey_openssh = PublicKey::from(ed25519_keypair.public)
        .to_openssh()
        .map_err(|e| SshError::KeyEncoding(e.to_string()))?;
    println!("Writing SSH public key to {:?}", pubkey_path);
    fs::write(&pubkey_path, &pubkey_openssh)?;

    let privkey_openssh = PrivateKey::from(ed25519_keypair)
        .to_openssh(LineEnding::default())
        .map_err(|e| SshError::KeyEncoding(e.to_string()))?
        .to_string();
    println!("Writing SSH private key to {:?}", privkey_path);
    fs::write(&privkey_path, &privkey_openssh)?;

    let mut perms = fs::metadata(&privkey_path)?.permissions();
    perms.set_mode(0o600);
    fs::set_permissions(&privkey_path, perms)?;

    Ok(GeneratedSshKeypair {
        pubkey_str: pubkey_openssh,
        pubkey_path,
        privkey_str: privkey_openssh,
        privkey_path,
    })
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
pub async fn connect_ssh_for_command_cancellable(
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
        if let Some(cancellation_tokens) = cancellation_tokens {
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
        let ssh_output = tokio::select! {
            _ = cancellation_tokens.ssh.cancelled() => {
                println!("SSH task was cancelled");
                return Ok(None)
            }
            val = ssh.call(ssh_launch_opts.interactive, ssh_launch_opts.env_vars, escaped_args) => {
                val
            }
        };
        cancellation_tokens.qemu.cancel();
        ssh_output?
    };

    println!("Exit code: {:?}", exit_code);
    cancellation_tokens.qemu.cancel();
    ssh.close().await?;
    Ok(Some(exit_code))
}

/// Connect SSH and run a user-provided command.
///
/// If requested, this will be an interactive session.
pub async fn connect_ssh_for_command(ssh_launch_opts: SshLaunchOpts) -> Result<Option<u32>> {
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
    .await?;
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
        let ssh_output = tokio::select! {
            val = ssh.call(ssh_launch_opts.interactive, ssh_launch_opts.env_vars, escaped_args) => {
                val
            }
        };
        ssh_output?
    };

    println!("Exit code: {:?}", exit_code);
    ssh.close().await?;
    Ok(Some(exit_code))
}
