use russh::client::{Handle, Handler};
use russh_sftp::{
    client::{error::Error as SftpError, SftpSession},
    protocol::OpenFlags,
};
use std::path::{Path, PathBuf};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
};
use tracing::{debug, error, info, instrument, warn};

use super::{SshClientHandle, SshError};
use crate::VmVolume;

/// Entry point: open SFTP, sync the volume, and disconnect.
#[instrument(skip(handle, volume))]
pub(super) async fn ssh_sync(
    handle: &mut SshClientHandle,
    volume: VmVolume,
) -> Result<(), SshError> {
    info!(
        source = %volume.source.display(),
        dest = %volume.dest,
        "Starting SSH SFTP sync"
    );

    let mut sftp = open_sftp(handle).await?;
    info!("SFTP subsystem opened");

    sftp_upload_volume(&mut sftp, &volume).await?;
    info!("Volume sync completed");

    Ok(())
}

/// Opens an SFTP client off a new "sftp" subsystem channel.
#[instrument(skip(handle))]
async fn open_sftp<H>(handle: &Handle<H>) -> Result<SftpSession, SshError>
where
    H: Handler<Error = russh::Error> + Clone + Send + 'static,
{
    debug!("Opening SSH session channel for SFTP");
    let ch = handle.channel_open_session().await?;

    debug!("Requesting SFTP subsystem");
    ch.request_subsystem(true, "sftp").await?;

    debug!("Creating SFTP session");
    let sftp = SftpSession::new(ch.into_stream()).await?;
    Ok(sftp)
}

/// Upload an entire volume (file or directory).
#[instrument(skip(sftp, volume))]
async fn sftp_upload_volume(sftp: &mut SftpSession, volume: &VmVolume) -> Result<(), SshError> {
    // Determine what the source is. Use async metadata to avoid blocking.
    let md = fs::symlink_metadata(&volume.source).await?;
    let ft = md.file_type();

    if ft.is_file() {
        info!(
            local = %volume.source.display(),
            remote = %volume.dest,
            "Uploading single file"
        );
        sftp_upload_file(sftp, &volume.source, &volume.dest).await
    } else if ft.is_dir() {
        info!(
            local = %volume.source.display(),
            remote = %volume.dest,
            "Uploading directory recursively"
        );
        sftp_upload_dir(sftp, &volume.source, &volume.dest).await
    } else if ft.is_symlink() {
        // Avoid surprising behavior; top-level symlink is ambiguous.
        let err = std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Top-level source is a symlink; refusing to upload",
        );
        Err(SshError::Io(err))
    } else {
        let err = std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Unsupported source type (not file/dir)",
        );
        Err(SshError::Io(err))
    }
}

/// Recursively traverse local directory and upload everything.
/// - Skips symlinks and special files for safety.
/// - Creates remote directories as needed (mkdir -p).
#[instrument(skip(sftp, src_root), fields(dst_root = %dst_root))]
async fn sftp_upload_dir(
    sftp: &mut SftpSession,
    src_root: &Path,
    dst_root: &str,
) -> Result<(), SshError> {
    if !src_root.is_dir() {
        return Err(SshError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "src must be a directory",
        )));
    }

    debug!(remote = %dst_root, "Ensuring remote destination root exists");
    sftp_mkdirs(sftp, dst_root).await?;

    let mut stack: Vec<PathBuf> = vec![src_root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let rel = dir.strip_prefix(src_root).unwrap_or(Path::new(""));
        let remote_dir = remote_join(dst_root, rel);

        debug!(local = %dir.display(), remote = %remote_dir, "Ensuring remote directory exists");
        sftp_mkdirs(sftp, &remote_dir).await?;

        let mut rd = fs::read_dir(&dir).await?;
        while let Some(entry) = rd.next_entry().await? {
            let path = entry.path();
            let md = fs::symlink_metadata(&path).await?;

            // Skip symlinks for safety to avoid unexpected traversal.
            if md.file_type().is_symlink() {
                warn!(path = %path.display(), "Skipping symlink");
                continue;
            }

            if md.is_dir() {
                stack.push(path);
            } else if md.is_file() {
                let rel = path.strip_prefix(src_root).unwrap_or(Path::new(""));
                let remote_file = remote_join(dst_root, rel);
                sftp_upload_file(sftp, &path, &remote_file).await?;
            } else {
                warn!(
                    path = %path.display(),
                    "Skipping special/unsupported file type"
                );
                continue;
            }
        }
    }

    Ok(())
}

/// Upload a single file by overwriting it (create + truncate).
#[instrument(skip(sftp, local), fields(remote = %remote))]
async fn sftp_upload_file(
    sftp: &mut SftpSession,
    local: &Path,
    remote: &str,
) -> Result<(), SshError> {
    // Ensure remote parent directory exists (POSIX path parsing).
    if let Some((parent, _)) = remote.rsplit_once('/') {
        if !parent.is_empty() {
            debug!(parent, "Ensuring remote parent directory exists");
            sftp_mkdirs(sftp, parent).await?;
        }
    }

    // Open local file
    let mut lf = fs::File::open(local).await?;
    let size = match lf.metadata().await {
        Ok(m) => m.len(),
        Err(_) => 0,
    };
    debug!(local = %local.display(), size_bytes = size, "Opened local file");

    // Open remote file with overwrite semantics
    let flags = OpenFlags::CREATE
        .union(OpenFlags::TRUNCATE)
        .union(OpenFlags::WRITE);

    let mut rf = sftp.open_with_flags(remote, flags).await?;
    debug!("Opened remote file for writing (create+truncate)");

    // Write contents in chunks
    let mut buf = vec![0u8; 128 * 1024];
    let mut written: u64 = 0;
    loop {
        let n = lf.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        rf.write_all(&buf[..n]).await?;
        written += n as u64;
    }

    // Flush and close remote handle
    rf.flush().await?;
    rf.shutdown().await?;

    info!(
        local = %local.display(),
        remote,
        bytes = written,
        "File upload completed"
    );

    Ok(())
}

/// Create remote directories recursively (mkdir -p).
#[instrument(skip(sftp), fields(remote_dir = %remote_dir))]
async fn sftp_mkdirs(sftp: &mut SftpSession, remote_dir: &str) -> Result<(), SshError> {
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
                debug!(path = %accum, "Remote directory already exists");
            } else {
                debug!(path = %accum, "Remote path exists, but is not directory");
                // TODO return error
            }
            continue;
        }

        match sftp.create_dir(&accum).await {
            Ok(_) => debug!(path = %accum, "Created remote directory"),
            Err(e) => {
                error!(
                    path = %accum,
                    error = %e,
                    "Failed to create remote directory"
                );
                return Err(SshError::from(e));
            }
        }
    }

    Ok(())
}

/// Join a remote POSIX path base with a relative path. Normalizes '.' and '..'.
fn remote_join(base: &str, rel: &Path) -> String {
    if rel.as_os_str().is_empty() {
        return base.to_string();
    }

    // Start with base without trailing slash.
    let mut out = base.trim_end_matches('/').to_string();

    // Append normalized segments from rel.
    for c in rel.components() {
        use std::path::Component;
        match c {
            Component::Normal(seg) => {
                out.push('/');
                out.push_str(&seg.to_string_lossy());
            }
            Component::CurDir => {
                // Skip
            }
            Component::ParentDir => {
                // For safety, do not traverse upwards in remote path.
                // Intentionally ignore ".." to stay under base.
            }
            _ => {
                // Ignore prefixes/roots on a relative path.
            }
        }
    }

    if out.is_empty() {
        // If base was "/", ensure we produce an absolute path.
        "/".to_string()
    } else {
        out
    }
}
