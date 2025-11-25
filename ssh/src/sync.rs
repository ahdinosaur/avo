use russh::client::{Handle, Handler};
use russh_sftp::{
    client::{error::Error as SftpError, SftpSession},
    protocol::OpenFlags,
};
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::{
    fs as tfs,
    io::{AsyncReadExt, AsyncWriteExt},
};
use tracing::{debug, info, instrument, trace, warn};

use super::SshClientHandle;
use lusid_fs::{self as fs, FsError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SshVolume {
    DirPath { local: PathBuf, remote: String },
    FilePath { local: PathBuf, remote: String },
    FileBytes { local: Vec<u8>, remote: String },
}

#[derive(Error, Debug)]
pub enum SshSyncError {
    #[error("filesystem error: {0}")]
    Fs(#[from] FsError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("SSH protocol error: {0}")]
    Russh(#[from] russh::Error),

    #[error("SFTP error: {0}")]
    RusshSftp(#[from] SftpError),

    #[error("refusing to upload: top-level source is a symlink")]
    TopLevelSymlink,

    #[error("unsupported source type (must be file or directory)")]
    UnsupportedSource,

    #[error("source path must be a directory")]
    SourceMustBeDirectory,
}

// Entry point: open SFTP, sync the volume.
#[instrument(skip(handle))]
pub(super) async fn ssh_sync(
    handle: &mut SshClientHandle,
    volume: SshVolume,
) -> Result<(), SshSyncError> {
    info!("Starting SSH volume sync");
    let mut sftp = open_sftp(handle).await?;
    sftp_upload_volume(&mut sftp, &volume).await?;
    info!("Volume sync completed");
    Ok(())
}

// Opens an SFTP client off a new "sftp" subsystem channel.
#[instrument(skip_all)]
async fn open_sftp<H>(handle: &Handle<H>) -> Result<SftpSession, SshSyncError>
where
    H: Handler<Error = russh::Error> + Clone + Send + 'static,
{
    let ch = handle.channel_open_session().await?;
    ch.request_subsystem(true, "sftp").await?;
    let sftp = SftpSession::new(ch.into_stream()).await?;
    Ok(sftp)
}

// Upload an entire volume (file or directory).
async fn sftp_upload_volume(
    sftp: &mut SftpSession,
    volume: &SshVolume,
) -> Result<(), SshSyncError> {
    match volume {
        SshVolume::DirPath { local, remote } => sftp_upload_dir(sftp, local, remote).await,
        SshVolume::FilePath { local, remote } => sftp_upload_file(sftp, local, remote).await,
        SshVolume::FileBytes { local, remote } => sftp_upload_file_bytes(sftp, local, remote).await,
    }
}

// Recursively traverse local directory and upload everything.
// - Skips symlinks and special files for safety.
// - Creates remote directories as needed (mkdir -p).
#[instrument(skip(sftp))]
async fn sftp_upload_dir(
    sftp: &mut SftpSession,
    local_root: &Path,
    remote_root: &str,
) -> Result<(), SshSyncError> {
    if !local_root.is_dir() {
        return Err(SshSyncError::SourceMustBeDirectory);
    }

    trace!(remote = %remote_root, "Ensuring remote destination root exists");
    sftp_mkdirs(sftp, remote_root).await?;

    let mut stack: Vec<PathBuf> = vec![local_root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let rel = dir.strip_prefix(local_root).unwrap_or(Path::new(""));
        let remote_dir = remote_join(remote_root, rel);

        trace!(local = %dir.display(), remote = %remote_dir, "Ensuring remote directory exists");
        sftp_mkdirs(sftp, &remote_dir).await?;

        let entries = fs::read_dir(&dir).await?;
        for path in entries {
            let md = tfs::symlink_metadata(&path).await?;
            if md.file_type().is_symlink() {
                warn!(path = %path.display(), "Skipping symlink");
                continue;
            }

            if md.is_dir() {
                stack.push(path);
            } else if md.is_file() {
                let rel = path.strip_prefix(local_root).unwrap_or(Path::new(""));
                let remote_file = remote_join(remote_root, rel);
                sftp_upload_file(sftp, &path, &remote_file).await?;
            } else {
                warn!(path = %path.display(), "Skipping special/unsupported file type");
                continue;
            }
        }
    }

    debug!("Directory upload completed");
    Ok(())
}

// Upload a single file by overwriting it
#[instrument(skip(sftp))]
async fn sftp_upload_file(
    sftp: &mut SftpSession,
    local: &Path,
    remote: &str,
) -> Result<(), SshSyncError> {
    #[allow(clippy::collapsible_if)]
    if let Some((parent, _)) = remote.rsplit_once('/') {
        if !parent.is_empty() {
            trace!(parent, "Ensuring remote parent directory exists");
            sftp_mkdirs(sftp, parent).await?;
        }
    }

    let mut lf = fs::open_file(local).await?;
    let size = match lf.metadata().await {
        Ok(m) => m.len(),
        Err(_) => 0,
    };
    trace!(local = %local.display(), size_bytes = size, "Opened local file");

    let flags = OpenFlags::CREATE
        .union(OpenFlags::TRUNCATE)
        .union(OpenFlags::WRITE);
    let mut rf = sftp.open_with_flags(remote, flags).await?;
    trace!("Opened remote file for writing");

    let mut buf = vec![0u8; 128 * 1024];
    loop {
        let n = lf.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        rf.write_all(&buf[..n]).await?;
    }
    rf.flush().await?;
    rf.shutdown().await?;

    debug!("File upload completed");
    Ok(())
}

#[instrument(skip(sftp))]
async fn sftp_upload_file_bytes(
    sftp: &mut SftpSession,
    local: &[u8],
    remote: &str,
) -> Result<(), SshSyncError> {
    #[allow(clippy::collapsible_if)]
    if let Some((parent, _)) = remote.rsplit_once('/') {
        if !parent.is_empty() {
            trace!(parent, "Ensuring remote parent directory exists");
            sftp_mkdirs(sftp, parent).await?;
        }
    }

    let flags = OpenFlags::CREATE
        .union(OpenFlags::TRUNCATE)
        .union(OpenFlags::WRITE);
    let mut rf = sftp.open_with_flags(remote, flags).await?;
    trace!("Opened remote file for writing");

    rf.write_all(local).await?;
    rf.flush().await?;
    rf.shutdown().await?;

    debug!("File upload completed");
    Ok(())
}

// Create remote directories recursively (mkdir -p).
#[instrument(skip(sftp))]
async fn sftp_mkdirs(sftp: &mut SftpSession, remote_dir: &str) -> Result<(), SshSyncError> {
    let remote_dir = remote_dir.trim();
    if remote_dir.is_empty() || remote_dir == "." {
        return Ok(());
    }

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

        if sftp.try_exists(&accum).await? {
            let metadata = sftp.metadata(&accum).await?;
            if metadata.is_dir() {
                trace!(path = %accum, "Remote directory already exists");
            } else {
                warn!(
                    path = %accum,
                    "Remote path exists but is not a directory; continuing"
                );
            }
            continue;
        }

        match sftp.create_dir(&accum).await {
            Ok(_) => trace!(path = %accum, "Created remote directory"),
            Err(e) => {
                tracing::error!(path = %accum, error = %e, "Failed to create remote directory");
                return Err(SshSyncError::from(e));
            }
        }
    }

    Ok(())
}

// Join a remote POSIX path base with a relative path. Normalizes '.' and '..'.
fn remote_join(base: &str, rel: &Path) -> String {
    if rel.as_os_str().is_empty() {
        return base.to_string();
    }

    let mut out = base.trim_end_matches('/').to_string();

    for c in rel.components() {
        use std::path::Component;
        match c {
            Component::Normal(seg) => {
                out.push('/');
                out.push_str(&seg.to_string_lossy());
            }
            Component::CurDir => {}
            Component::ParentDir => {}
            _ => {}
        }
    }

    if out.is_empty() {
        "/".to_string()
    } else {
        out
    }
}
