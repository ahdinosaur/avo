use std::fmt::Debug;
use std::{env, io};

use async_promise::Promise;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite};
use tracing::info;

use crate::session::{AsyncChannel, AsyncSession, NoCheckHandler};
use crate::stream::ReadStream;
use crate::SshError;

/// Terminal execution specific errors.
#[derive(Error, Debug)]
pub enum SshTerminalError {
    #[error("failed to open SSH session channel: {0}")]
    ChannelOpen(#[source] russh::Error),

    #[error("SSH protocol error: {0}")]
    Russh(#[from] russh::Error),

    #[error("failed to request pty: {0}")]
    RequestPty(#[source] russh::Error),

    #[error("failed to get terminal size: {0}")]
    TerminalSize(#[source] io::Error),
}

/// A streaming handle to a running SSH terminal.
///
/// - stdout/stderr are AsyncBufRead (and AsyncRead) via ReadStream.
/// - stdin is available via stdin().
/// - exit code and other events exposed as Promises.
/// - call wait() to await completion and get the exit code.
pub struct SshTerminalHandle {
    pub stdout: ReadStream,
    pub stderr: ReadStream,
    pub channel: AsyncChannel,
}

impl SshTerminalHandle {
    /// Obtain a writer for the terminal's stdin.
    pub fn stdin(&self) -> impl AsyncWrite + use<> {
        self.channel.stdin()
    }

    /// Obtain a reader for the terminal's stdout.
    pub fn stdout(&mut self) -> &mut ReadStream {
        &mut self.stdout
    }

    /// Obtain a reader for the terminal's stderr.
    pub fn stderr(&mut self) -> &mut ReadStream {
        &mut self.stdout
    }

    /// Promise that resolves to the remote exit code when received.
    pub fn exit_code(&self) -> &Promise<u32> {
        self.channel.recv_exit_status()
    }

    /// Promise that resolves when EOF is received for stdout/stderr.
    pub fn eof(&self) -> &Promise<()> {
        self.channel.recv_eof()
    }

    /// Promise that resolves when the server replies Success/Failure to exec.
    pub fn success_failure(&self) -> &Promise<bool> {
        self.channel.recv_success_failure()
    }

    /// Close the channel cleanly and wait for it to be closed, returning exit
    /// code if received.
    #[tracing::instrument(skip(self))]
    pub async fn wait(mut self) -> Result<Option<u32>, SshError> {
        let exit_code = self.exit_code().wait().await.copied();

        if !self.channel.is_closed() {
            self.channel
                .close()
                .await
                .map_err(SshTerminalError::Russh)
                .map_err(SshError::Terminal)?;
            self.channel.wait_close().await;
        }

        info!(exit_code = exit_code, "Remote terminal completed");

        Ok(exit_code)
    }
}

/// Execute a remote terminal and return a streaming handle.
///
/// - stdout/stderr streams are created before exec to avoid missing data.
/// - exec requests a reply, so success_failure() will resolve.
#[tracing::instrument(skip(session), fields(terminal))]
pub(super) async fn ssh_terminal(
    session: &AsyncSession<NoCheckHandler>,
) -> Result<SshTerminalHandle, SshTerminalError> {
    let channel = session
        .open_channel()
        .await
        .map_err(SshTerminalError::ChannelOpen)?;

    let stdout = channel.stdout();
    let stderr = channel.stderr();

    let want_reply = true;
    let term = &env::var("TERM").unwrap_or("xterm-256color".into());
    let (col_width, row_height) =
        termion::terminal_size().map_err(SshTerminalError::TerminalSize)?;
    let pix_width = 0;
    let pix_height = 0;
    let terminal_modes = &[]; // ideally you want to pass the actual terminal modes here

    channel
        .request_pty(
            want_reply,
            term,
            col_width.into(),
            row_height.into(),
            pix_width,
            pix_height,
            terminal_modes,
        )
        .await
        .map_err(SshTerminalError::RequestPty);

    Ok(SshTerminalHandle {
        stdout,
        stderr,
        channel,
    })
}
