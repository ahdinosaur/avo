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

use crate::VmVolume;

use super::{SshClientHandle, SshError};

// Best-effort detection for "already exists" on mkdir.
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
    let mut lf = fs::File::open(local).await?;

    let flags = OpenFlags::READ
        .union(OpenFlags::WRITE)
        .union(OpenFlags::APPEND);

    let mut rf = sftp.open_with_flags(remote, flags).await?;

    // Write contents in chunks
    let mut buf = vec![0u8; 128 * 1024];
    loop {
        let n = lf.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        rf.write_all(&buf[..n]).await?;
    }

    rf.flush().await?;

    Ok(())
}

async fn sftp_upload_volume(sftp: &mut SftpSession, volume: &VmVolume) -> Result<(), SshError> {
    if volume.source.is_file() {
        sftp_upload_file(sftp, &volume.source, &volume.dest).await
    } else if volume.source.is_dir() {
        sftp_upload_dir(sftp, &volume.source, &volume.dest).await
    } else {
        // TODO make into error type
        panic!("Unexpected volume! {:?}", volume)
    }
}

// Recursively traverse local directory and upload everything.
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

    // Ensure the destination root exists
    sftp_mkdirs(sftp, dst_root).await?;

    let mut stack: Vec<PathBuf> = vec![src_root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        // Ensure the remote directory exists
        let rel = dir.strip_prefix(src_root).unwrap();
        let remote_dir = remote_join(dst_root, rel);
        sftp_mkdirs(sftp, &remote_dir).await?;

        let mut rd = fs::read_dir(&dir).await?;
        while let Some(entry) = rd.next_entry().await? {
            let path = entry.path();

            // Follow symlinks
            let ft = entry.metadata().await?.file_type();

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

pub(super) async fn ssh_sync(
    handle: &mut SshClientHandle,
    volume: VmVolume,
) -> Result<(), SshError> {
    // Open SFTP subsystem and perform upload
    let mut sftp = open_sftp(handle).await?;
    sftp_upload_volume(&mut sftp, &volume).await?;

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
