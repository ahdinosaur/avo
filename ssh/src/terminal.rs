use futures_util::StreamExt;
use signal_hook::consts::SIGWINCH;
use signal_hook_tokio::Signals;
use std::fmt::Debug;
use std::{env, io};
use termion::raw::IntoRawMode;
use thiserror::Error;
use tokio::io::{copy, stderr, stdout};
use tracing::info;

use crate::session::{AsyncSession, NoCheckHandler};

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

    #[error("failed to create signals stream")]
    Signals(#[source] io::Error),

    #[error("failed to put stdout into raw mode: {0}")]
    StdoutRawMode(#[source] io::Error),
}

/// Execute a remote terminal and return a streaming handle.
///
/// - stdout/stderr streams are created before exec to avoid missing data.
/// - exec requests a reply, so success_failure() will resolve.
#[tracing::instrument(skip(session), fields(terminal))]
pub(super) async fn ssh_terminal(
    session: &AsyncSession<NoCheckHandler>,
) -> Result<Option<u32>, SshTerminalError> {
    let mut channel = session
        .open_channel()
        .await
        .map_err(SshTerminalError::ChannelOpen)?;

    let mut signals = Signals::new(&[SIGWINCH]).map_err(SshTerminalError::Signals)?;

    // We're using `termion` to put the terminal into raw mode, so that we can
    // display the output of interactive applications correctly.
    let _raw_term = std::io::stdout()
        .into_raw_mode()
        .map_err(SshTerminalError::StdoutRawMode)?;

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

    copy(&mut channel.stdout(), &mut stdout());
    copy(&mut channel.stderr(), &mut stderr());

    let signals_handle = signals.handle();

    tokio::spawn(async move {
        while let Some(signal) = signals.next().await {
            match signal {
                SIGWINCH => {
                    let (col_width, row_height) =
                        termion::terminal_size().map_err(SshTerminalError::TerminalSize)?;
                    channel.window_change(col_width.into(), row_height.into(), 0, 0);
                }
                _ => unreachable!(),
            }
        }
    });

    let exit_code = channel.recv_exit_status().wait().await.copied();
    if !channel.is_closed() {
        channel.close().await;
        channel.wait_close().await;
    }

    info!(exit_code = exit_code, "Remote terminal completed");

    Ok(exit_code)
}
