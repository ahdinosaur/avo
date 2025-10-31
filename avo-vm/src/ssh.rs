use std::fmt::Debug;
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{env, os::unix::fs::PermissionsExt, path::Path, sync::Arc, time::Duration};

use base64ct::LineEnding;
use color_eyre::eyre::{Context, Result, bail};
use russh::keys::ssh_key::{private::Ed25519Keypair, rand_core::OsRng};
use russh::keys::{PrivateKey, PublicKey};
use russh::{ChannelMsg, Disconnect, keys::key::PrivateKeyWithHashAlg};
use serde::{Deserialize, Serialize};
use termion::raw::IntoRawMode;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::Instant;
use tokio_fd::AsyncFd;
use tokio_vsock::{VsockAddr, VsockStream};
use tracing::{debug, error, info, instrument};

use crate::cli::{self, EnvVar};
use crate::runner::CancellationTokens;

#[derive(Clone, Debug)]
pub struct PersistedSshKeypair {
    pub pubkey_str: String,
    pub _pubkey_path: PathBuf,
    pub privkey_str: String,
    pub privkey_path: PathBuf,
}

impl PersistedSshKeypair {
    // Try to load a keypair from `dir`
    pub fn from_dir(dir: &Path) -> Result<Self> {
        let privkey_path = dir.join("id_ed25519");
        let pubkey_path = privkey_path.with_extension("pub");
        let privkey_str = fs::read_to_string(&privkey_path)?;
        let pubkey_str = fs::read_to_string(&pubkey_path)?;

        Ok(Self {
            pubkey_str,
            _pubkey_path: pubkey_path,
            privkey_str,
            privkey_path,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SshLaunchOpts {
    #[serde(skip)]
    pub privkey: String,
    pub tty: bool,
    pub interactive: cli::Interactive,
    pub timeout: Duration,
    pub env_vars: Vec<EnvVar>,
    pub args: Vec<String>,
    pub cid: u32,
}

/// Retrieve or create SSH keypair from `path` to be used with the virtual machine
#[instrument]
pub fn ensure_ssh_key(dir: &Path) -> Result<PersistedSshKeypair> {
    // First try reading an existing keypair from disk.
    // If that fails we'll just create a new one.
    if let Ok(existing_keypair) = PersistedSshKeypair::from_dir(dir) {
        return Ok(existing_keypair);
    }

    let privkey_path = dir.join("id_ed25519");
    let pubkey_path = privkey_path.with_extension("pub");

    let ed25519_keypair = Ed25519Keypair::random(&mut OsRng);

    let pubkey_openssh = PublicKey::from(ed25519_keypair.public).to_openssh()?;
    debug!("Writing SSH public key to {pubkey_path:?}");
    fs::write(&pubkey_path, &pubkey_openssh)?;

    let privkey_openssh = PrivateKey::from(ed25519_keypair)
        .to_openssh(LineEnding::default())?
        .to_string();
    debug!("Writing SSH private key to {privkey_path:?}");

    fs::write(&privkey_path, &privkey_openssh)?;
    let mut perms = fs::metadata(&privkey_path)?.permissions();
    perms.set_mode(0o600);
    fs::set_permissions(&privkey_path, perms)?;

    let keypair = PersistedSshKeypair {
        pubkey_str: pubkey_openssh,
        _pubkey_path: pubkey_path,
        privkey_str: privkey_openssh,
        privkey_path,
    };
    Ok(keypair)
}

#[derive(Debug, Clone)]
struct SshClient {}

// More SSH event handlers can be defined in this trait
//
// In this example, we're only using Channel, so these aren't needed.
impl russh::client::Handler for SshClient {
    type Error = russh::Error;

    #[instrument]
    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

/// This struct is a convenience wrapper around a russh client that handles the input/output event
/// loop
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
        match self {
            Pty::Enabled { .. } => true,
            Pty::Disabled => false,
        }
    }
}

/// Functionality for asynchronously reading from stdin, if enabled.
enum StdinReader {
    Enabled { fd: AsyncFd, buf: Vec<u8> },
    Closed,
    Disabled,
}

impl StdinReader {
    /// If reading from stdin is enabled, try to read into its buffer and return the bytes read along with a reference to the buffer.
    /// This allows us to conveniently do an optional read in tokio's select! macro.
    async fn maybe_read(&mut self) -> Option<(std::io::Result<usize>, &[u8])> {
        match self {
            StdinReader::Enabled { fd, buf } => Some((fd.read(buf).await, buf)),
            StdinReader::Disabled => None,
            StdinReader::Closed => None,
        }
    }
}

impl Session {
    #[instrument(skip(privkey))]
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
        debug!("Connecting to SSH via vsock");
        let mut session = loop {
            tokio::time::sleep(Duration::from_millis(100)).await;

            // I would like to apologize for the error handling below.

            // Establish vsock connection
            let stream = match VsockStream::connect(vsock_addr).await {
                Ok(stream) => stream,
                Err(ref e) if e.raw_os_error() == Some(19) => {
                    // This is "No such device" but for some reason Rust doesn't have an IO
                    // ErrorKind for it. Meh.
                    if now.elapsed() > timeout {
                        error!(
                            "Reached timeout trying to connect to virtual machine via SSH, aborting"
                        );
                        bail!("Timeout");
                    }
                    continue;
                }
                Err(ref e) => match e.kind() {
                    ErrorKind::TimedOut
                    | ErrorKind::ConnectionRefused
                    | ErrorKind::ConnectionReset => {
                        if now.elapsed() > timeout {
                            error!(
                                "Reached timeout trying to connect to virtual machine via SSH, aborting"
                            );
                            bail!("Timeout");
                        }
                        continue;
                    }
                    e => {
                        error!("Unhandled error occured: {e}");
                        bail!("Unknown error");
                    }
                },
            };

            // Connect to SSH via vsock stream
            match russh::client::connect_stream(config.clone(), stream, sh.clone()).await {
                Ok(x) => break x,
                Err(russh::Error::IO(ref e)) => {
                    match e.kind() {
                        // The VM is still booting at this point so we're just ignoring these errors
                        // for some time.
                        ErrorKind::ConnectionRefused | ErrorKind::ConnectionReset => {
                            if now.elapsed() > timeout {
                                error!(
                                    "Reached timeout trying to connect to virtual machine via SSH, aborting"
                                );
                                bail!("Timeout");
                            }
                        }
                        e => {
                            error!("Unhandled error occured: {e}");
                            bail!("Unknown error");
                        }
                    }
                }
                Err(russh::Error::Disconnect) => {
                    if now.elapsed() > timeout {
                        error!(
                            "Reached timeout trying to connect to virtual machine via SSH, aborting"
                        );
                        bail!("Timeout");
                    }
                }
                Err(e) => {
                    error!("Unhandled error occured: {e}");
                    bail!("Unknown error");
                }
            }
        };
        debug!("Authenticating via SSH");

        // use publickey authentication
        let auth_res = session
            .authenticate_publickey("root", PrivateKeyWithHashAlg::new(Arc::new(privkey), None))
            .await?;

        if !auth_res.success() {
            bail!("Authentication (with publickey) failed");
        }

        let tty_state = if allocate_tty {
            Pty::Enabled {
                host_terminal_size: termion::terminal_size().wrap_err("Requested a TTY inside the VM, but vmexec doesn't seem to be running in a terminal")?,
            }
        } else {
            Pty::Disabled
        };

        Ok(Self { session, tty_state })
    }

    #[instrument(skip(self))]
    async fn call(
        &mut self,
        interactive: cli::Interactive,
        env: Vec<EnvVar>,
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
                    &[], // ideally you want to pass the actual terminal modes here
                )
                .await?;
        }

        for e in env {
            channel.set_env(true, e.key, e.value).await?;
        }

        //channel.request_shell(true).await?;
        channel.exec(true, command).await?;

        let code;
        let mut stdin_reader = match interactive {
            cli::Interactive::Always => {
                let buf = vec![0; 1024];
                let fd = tokio_fd::AsyncFd::try_from(libc::STDIN_FILENO).wrap_err("Requested stdin to be piped to the process inside the VM, but failed to open stdin.")?;
                StdinReader::Enabled { fd, buf }
            }
            cli::Interactive::Never => StdinReader::Disabled,
            cli::Interactive::Auto => {
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

        // TODO maybe have entirely separate code paths for interactive vs non-interactive?
        // We don't need to handle terminal resizing and stdin at all if we have no tty and are not
        // interactive.

        loop {
            // Handle one of the possible events:
            tokio::select! {
                // Handle terminal resize
                _ = tokio::time::sleep(Duration::from_millis(500)), if self.tty_state.is_enabled() => {
                    if let Pty::Enabled{host_terminal_size} = &self.tty_state {
                        let new_terminal_size = termion::terminal_size()?;
                        if host_terminal_size != &new_terminal_size {
                            debug!("Terminal size change detected");
                            self.tty_state = Pty::Enabled { host_terminal_size: new_terminal_size };
                            channel.window_change(new_terminal_size.0 as u32, new_terminal_size.1 as u32, 0, 0).await?;
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
                        // Send it to the server
                        Ok(n) => channel.data(&buf[..n]).await?,
                        Err(e) => return Err(e.into()),
                    };
                },
                // There's an event available on the session channel
                Some(msg) = channel.wait() => {
                    match msg {
                        // Write data to the terminal
                        ChannelMsg::Data { ref data } => {
                            stdout.write_all(data).await?;
                            stdout.flush().await?;
                        }
                        ChannelMsg::ExtendedData { ref data, ext } => {
                            // ext == 1 means it's stderr content
                            // https://github.com/Eugeny/russh/discussions/258
                            if ext == 1 {
                                stderr.write_all(data).await?;
                                stderr.flush().await?;
                            }
                        }
                        // The command has returned an exit code
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

    #[instrument(skip(self))]
    async fn close(&mut self) -> Result<()> {
        self.session
            .disconnect(Disconnect::ByApplication, "", "English")
            .await?;
        Ok(())
    }
}

/// Connect SSH and run a command that checks whether the system is ready for operation and then
/// shuts down.
///
/// The `qemu_should_exit` is used to tell the QEMU process to wait for process completion once
/// we've told the VM to shutdown. This is not the same as the cancellation token! We use the
/// cancellation token to actively cancel QEMU. In contrast, we use `qemu_should_exit` to signal
/// that QEMU is expected to exit on its own. Usually, we asssume QEMU exitting on its own is a
/// sign that something is wrong which is why we need a signal in cases where we expect it to exit.
#[instrument(skip(ssh_launch_opts))]
pub async fn connect_ssh_for_warmup(
    qemu_should_exit: Arc<AtomicBool>,
    ssh_launch_opts: SshLaunchOpts,
) -> Result<()> {
    let privkey = PrivateKey::from_openssh(ssh_launch_opts.privkey)?;

    // Session is a wrapper around a russh client, defined down below.
    let mut ssh = Session::connect(
        privkey,
        ssh_launch_opts.cid,
        22,
        ssh_launch_opts.timeout,
        ssh_launch_opts.tty,
    )
    .await?;
    info!("Connected");

    // First we'll wait until the system has fully booted up.
    let is_running_exitcode = ssh
        .call(
            cli::Interactive::Never,
            vec![],
            "systemctl is-system-running --wait --quiet",
        )
        .await?;
    debug!("systemctl is-system-running --wait exit code {is_running_exitcode}");

    // TODO: Here we'll add a stupid hack to deal with
    // https://github.com/linux-pam/linux-pam/issues/885 for the time being.
    ssh.call(
        cli::Interactive::Never,
        vec![],
        "echo 127.0.0.1 unknown >> /etc/hosts",
    )
    .await?;

    // Allow the --env option to work by allowing SSH to accept all sent environment variables.
    ssh.call(
        cli::Interactive::Never,
        vec![],
        "echo AcceptEnv * >> /etc/ssh/sshd_config",
    )
    .await?;

    // Then shut the system down.
    ssh.call(cli::Interactive::Never, vec![], "systemctl poweroff")
        .await?;
    debug!("Shutting down system");

    // Tell the QEMU handler it's now fine to wait for exit.
    qemu_should_exit.store(true, Ordering::SeqCst);

    // Ignore whatever error we might get from this as we want to close the connection at this
    // point anyway.
    let _ = ssh.close().await;
    Ok(())
}

/// Connect SSH and run a user-provided command.
///
/// If requested, this will be an interactive session.
///
/// The `cancellation_tokens` are used to cancel a running QEMU task in case there's a problem with
/// the SSH connection or upon command completion. The QEMU task can also use it to cancel the SSH
/// task.
#[instrument(skip(cancellation_tokens, ssh_launch_opts))]
pub async fn connect_ssh_for_command_cancellable(
    cancellation_tokens: CancellationTokens,
    ssh_launch_opts: SshLaunchOpts,
) -> Result<Option<u32>> {
    let privkey = PrivateKey::from_openssh(ssh_launch_opts.privkey)?;

    // Session is a wrapper around a russh client, defined down below
    let mut ssh = Session::connect(
        privkey,
        ssh_launch_opts.cid,
        22,
        ssh_launch_opts.timeout,
        ssh_launch_opts.tty,
    )
    .await
    .inspect_err(|_| {
        cancellation_tokens.qemu.cancel();
    })?;
    info!("Connected via SSH");

    let exit_code = {
        // We're using `termion` to put the terminal into raw mode, so that we can
        // display the output of interactive applications correctly.
        let _raw_term = if ssh_launch_opts.tty {
            Some(std::io::stdout().into_raw_mode()?)
        } else {
            None
        };

        let escaped_args = &ssh_launch_opts
            .args
            .into_iter()
            // arguments are escaped manually since the SSH protocol doesn't support quoting
            .map(|x| shell_escape::escape(x.into()))
            .collect::<Vec<_>>()
            .join(" ");
        let ssh_output = tokio::select! {
            _ = cancellation_tokens.ssh.cancelled() => {
                debug!("SSH task was cancelled");
                return Ok(None)
            }
            val = ssh.call(ssh_launch_opts.interactive, ssh_launch_opts.env_vars, escaped_args) => {
                val
            }
        };
        cancellation_tokens.qemu.cancel();
        ssh_output?
    };

    info!("Exit code: {:?}", exit_code);
    cancellation_tokens.qemu.cancel();
    ssh.close().await?;
    Ok(Some(exit_code))
}

/// Connect SSH and run a user-provided command.
///
/// If requested, this will be an interactive session.
#[instrument(skip(ssh_launch_opts))]
pub async fn connect_ssh_for_command(ssh_launch_opts: SshLaunchOpts) -> Result<Option<u32>> {
    let privkey = PrivateKey::from_openssh(ssh_launch_opts.privkey)?;

    // Session is a wrapper around a russh client, defined down below
    let mut ssh = Session::connect(
        privkey,
        ssh_launch_opts.cid,
        22,
        ssh_launch_opts.timeout,
        ssh_launch_opts.tty,
    )
    .await?;
    info!("Connected via SSH");

    let exit_code = {
        // We're using `termion` to put the terminal into raw mode, so that we can
        // display the output of interactive applications correctly.
        let _raw_term = if ssh_launch_opts.tty {
            Some(std::io::stdout().into_raw_mode()?)
        } else {
            None
        };

        let escaped_args = &ssh_launch_opts
            .args
            .into_iter()
            // arguments are escaped manually since the SSH protocol doesn't support quoting
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

    info!("Exit code: {:?}", exit_code);
    ssh.close().await?;
    Ok(Some(exit_code))
}
