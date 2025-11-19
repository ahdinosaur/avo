use russh::client::{Handle, Handler};
use russh_sftp::{
    client::{error::Error as SftpError, SftpSession},
    protocol::OpenFlags,
};
use std::path::{Path, PathBuf};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
    net::ToSocketAddrs,
};

use crate::{ssh::SshConnectOptions, VmVolume};

use super::{connect::connect_with_retry, SshError};

#[derive(Debug)]
pub struct SshSyncOptions<Addrs>
where
    Addrs: ToSocketAddrs + Clone + Send,
{
    pub connect: SshConnectOptions<Addrs>,

    pub volume: VmVolume,

    pub follow_symlinks: bool,
}

// Best-effort detection for "already exists" on mkdir.
// If your russh_sftp version exposes a specific status code,
// match on that instead of string matching.
fn sftp_err_is_already_exists(err: &SftpError) -> bool {
    let s = err.to_string().to_ascii_lowercase();
    s.contains("already exists") || s.contains("exist")
}

// Join a remote POSIX path base with a relative path.
fn remote_join(base: &str, rel: &Path) -> String {
    if rel.as_os_str().is_empty() {
        return base.to_string();
    }
    let mut out = base.trim_end_matches('/').to_string();
    for c in rel.components() {
        let seg = c.as_os_str().to_string_lossy();
        if !seg.is_empty() {
            out.push('/');
            out.push_str(&seg);
        }
    }
    out
}

// Create remote directories recursively (mkdir -p).
async fn sftp_mkdirs(sftp: &mut SftpSession, remote_dir: &str) -> Result<(), SshError> {
    let mut accum = String::new();
    if remote_dir.starts_with('/') {
        accum.push('/');
    }

    for seg in remote_dir.split('/').filter(|s| !s.is_empty()) {
        if accum.is_empty() || accum == "/" {
            accum.push_str(seg);
        } else {
            accum.push('/');
            accum.push_str(seg);
        }

        if let Err(e) = sftp.create_dir(&accum).await {
            if !sftp_err_is_already_exists(&e) {
                return Err(SshError::from(e));
            }
        }
    }

    Ok(())
}

// Upload a single file by overwriting it.
async fn sftp_upload_file(
    sftp: &mut SftpSession,
    local: &Path,
    remote: &str,
) -> Result<(), SshError> {
    // Ensure remote parent directory exists
    if let Some(parent) = Path::new(remote).parent() {
        let parent_str = parent.to_string_lossy();
        sftp_mkdirs(sftp, &parent_str).await?;
    }

    // Open local file
    let mut lf = fs::File::open(local).await.map_err(SshError::Io)?;

    let flags = OpenFlags::READ
        .union(OpenFlags::WRITE)
        .union(OpenFlags::APPEND);

    let mut rf = sftp.open_with_flags(remote, flags).await?;

    // Write contents in chunks
    // Note: Depending on russh_sftp version, rf may implement futures::io
    // AsyncWrite. If so, use futures::io::AsyncWriteExt's write_all instead of
    // tokio::io::AsyncWriteExt. Below we call a method often available on the
    // SFTP handle; if your version differs, adapt accordingly.
    let mut buf = vec![0u8; 128 * 1024];
    loop {
        let n = lf.read(&mut buf).await.map_err(SshError::Io)?;
        if n == 0 {
            break;
        }
        // If your SftpFile type doesn't have write_all, use write in a loop
        // or bring `use futures::io::AsyncWriteExt as FuturesWriteExt;`
        // and call `FuturesWriteExt::write_all(&mut rf, &buf[..n]).await`.
        rf.write_all(&buf[..n]).await?;
    }

    // Some versions have explicit close/flush; if so, call them here.
    Ok(())
}

async fn sftp_upload_volume(
    sftp: &mut SftpSession,
    volume: &VmVolume,
    follow_symlinks: bool,
) -> Result<(), SshError> {
    if volume.source.is_file() {
        sftp_upload_file(sftp, &volume.source, &volume.dest).await
    } else if volume.source.is_dir() {
        sftp_upload_dir(sftp, &volume.source, &volume.dest, follow_symlinks).await
    } else {
        // TODO make into error type
        panic!("Unexpected volume! {:?}", volume)
    }
}

// Recursively traverse local directory and upload everything.
// Skips symlinks unless follow_symlinks is true, in which case the regular
// file contents of the symlink target are uploaded.
async fn sftp_upload_dir(
    sftp: &mut SftpSession,
    src_root: &Path,
    dst_root: &str,
    follow_symlinks: bool,
) -> Result<(), SshError> {
    if !src_root.is_dir() {
        return Err(SshError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "src must be a directory",
        )));
    }

    // Ensure the destination root exists
    sftp_mkdirs(sftp, dst_root).await?;

    let mut stack: Vec<PathBuf> = vec![src_root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        // Ensure the remote directory exists
        let rel = dir.strip_prefix(src_root).unwrap();
        let remote_dir = remote_join(dst_root, rel);
        sftp_mkdirs(sftp, &remote_dir).await?;

        let mut rd = fs::read_dir(&dir).await.map_err(SshError::Io)?;
        while let Some(entry) = rd.next_entry().await.map_err(SshError::Io)? {
            let path = entry.path();

            // Decide how to treat symlinks
            let ft = if follow_symlinks {
                entry.metadata().await.map_err(SshError::Io)?.file_type()
            } else {
                entry.file_type().await.map_err(SshError::Io)?
            };

            if ft.is_dir() {
                stack.push(path);
            } else if ft.is_file() {
                let rel = path.strip_prefix(src_root).unwrap();
                let remote_file = remote_join(dst_root, rel);
                sftp_upload_file(sftp, &path, &remote_file).await?;
            } else {
                // Skip symlinks and other special files by default
                continue;
            }
        }
    }

    Ok(())
}

pub async fn ssh_sync<Addrs: ToSocketAddrs + Clone + Send + Sync + 'static>(
    options: SshSyncOptions<Addrs>,
) -> Result<(), SshError> {
    let SshSyncOptions {
        connect,
        volume,
        follow_symlinks,
    } = options;

    let handle = connect_with_retry(connect).await?;

    // Open SFTP subsystem and perform upload
    let mut sftp = open_sftp(&handle).await?;
    sftp_upload_volume(&mut sftp, &volume, follow_symlinks).await?;

    // Graceful close
    let _ = handle
        .disconnect(russh::Disconnect::ByApplication, "", "English")
        .await;

    Ok(())
}

// Opens an SFTP client off a new "sftp" subsystem channel.
async fn open_sftp<H>(handle: &Handle<H>) -> Result<SftpSession, SshError>
where
    H: Handler<Error = russh::Error> + Clone + Send + 'static,
{
    let ch = handle.channel_open_session().await?;
    ch.request_subsystem(true, "sftp").await?;
    let sftp = SftpSession::new(ch.into_stream()).await?;
    Ok(sftp)
}
