use std::{path::Path, process::Stdio, time::Duration};
use thiserror::Error;
use tokio::{
    io,
    process::{Child, Command},
};

use crate::{paths::ExecutablePaths, qemu::BindMount};

#[derive(Error, Debug)]
pub enum LaunchVirtiofsdError {
    #[error("failed to spawn virtiofsd")]
    Spawn(#[from] io::Error),
    #[error("virtiofsd failed: {stderr}")]
    CommandError { stderr: String },
}

/// Launch an instance of virtiofsd for a particular volume
pub async fn launch_virtiofsd(
    executables: &ExecutablePaths,
    instance_dir: &Path,
    volume: &BindMount,
) -> Result<Child, LaunchVirtiofsdError> {
    let socket_path = instance_dir.join(volume.socket_name());

    let mut virtiofsd_cmd = Command::new(executables.unshare());
    virtiofsd_cmd
        .arg("-r")
        .arg("--map-auto")
        .arg("--")
        .arg(executables.virtiofsd())
        .args(["--shared-dir", &volume.source.to_string_lossy()])
        .args(["--socket-path", &socket_path.to_string_lossy()])
        // This seems to allow us to skip past the guest's page cache which is very desireable
        // since we want to ensure that the memory usage inside the guest stays minimal. If we
        // instead hit the host directly then the host can manage the cache for us.
        .args(["--cache", "never"])
        // Like the above, we want to skip the caches as much as possible. so allowing direct IO
        // seems prudent.
        .arg("--allow-direct-io")
        // It seems like a good idea to allow mmap to work so that users are not suprised by weird
        // kernel errors.
        .arg("--allow-mmap")
        // Create a thread pool with 8 threads. We have yet to test whether this does anything in
        // terms of performance.
        .args(["--thread-pool-size", "8"])
        .args(["--sandbox", "chroot"]);

    if volume.read_only {
        virtiofsd_cmd.arg("--readonly");
    }

    let mut virtiofsd_child = virtiofsd_cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    tokio::select! {
        // I tried very hard to find a reasonable way to check properly for connectivity but there
        // doesn't seem to be a good way as the server quits after the first connection, see also:
        // https://gitlab.com/virtio-fs/virtiofsd/-/issues/62
        // As such, we're going to use a timing based approach for the time being.
        _ = tokio::time::sleep(Duration::from_millis(250)) => {},
        _ = virtiofsd_child.wait() => {
            eprintln!("virtiofsd process exited early, that's usually a bad sign");

            let virtiofsd_output = virtiofsd_child.wait_with_output().await?;
            return Err(LaunchVirtiofsdError::CommandError { stderr: String::from_utf8_lossy(&virtiofsd_output.stderr).to_string() });
        }
    }

    Ok(virtiofsd_child)
}
