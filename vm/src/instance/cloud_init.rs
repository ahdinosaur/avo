use avo_machine::Machine;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::process::Command;

use crate::{
    context::Context,
    fs::{self, FsError},
    instance::get_machine_id,
    ssh::keypair::SshKeypair,
};

#[derive(Debug, Clone)]
pub struct VmInstanceCloudInit {
    pub cloud_init_image: PathBuf,
}

#[derive(Error, Debug)]
pub enum CloudInitError {
    #[error("failed to get output from `mkisofs ...`")]
    CommandOutput(#[from] tokio::io::Error),
    #[error("mkisofs failed")]
    CommandError { stderr: String },
    #[error(transparent)]
    Fs(#[from] FsError),
}

pub async fn setup_cloud_init(
    ctx: &mut Context,
    machine: &Machine,
    ssh_keypair: &SshKeypair,
) -> Result<VmInstanceCloudInit, CloudInitError> {
    let paths = ctx.paths();

    let machine_id = get_machine_id(machine);
    let hostname = machine.hostname.clone();

    let meta_data_path = paths.cloud_init_meta_data_file(machine_id);
    let user_data_path = paths.cloud_init_user_data_file(machine_id);
    let image_path = paths.cloud_init_image_file(machine_id);

    if !Path::new(&meta_data_path).exists() {
        fs::write_file(
            &meta_data_path,
            format!("instance-id: {machine_id}\nlocal-hostname: {hostname}\n").as_bytes(),
        )
        .await?;
    }

    if !Path::new(&user_data_path).exists() {
        let ssh_public_keys = [&ssh_keypair.public_key];
        let ssh_keys = format!(
            "\u{20}\u{20}\u{20}\u{20}ssh-authorized-keys:\n{}",
            ssh_public_keys
                .iter()
                .map(|key| format!("\u{20}\u{20}\u{20}\u{20}\u{20}\u{20}- {key}"))
                .collect::<Vec<_>>()
                .join("\n")
        );

        fs::write_file(
            &user_data_path,
            format!(
                "\
                #cloud-config\n\
                hostname: {hostname}
                {ssh_keys}\n\
                packages:\n\
                \u{20}\u{20}- openssh\n\
            "
            )
            .as_bytes(),
        )
        .await?;
    }

    if !Path::new(&image_path).exists() {
        let output = Command::new(ctx.executables().mkisofs())
            .arg("-RJ")
            .arg("-V")
            .arg("cidata")
            .arg("-o")
            .arg(&image_path)
            .arg("-graft-points")
            .arg(format!("/={}", user_data_path.to_string_lossy()))
            .arg(format!("/={}", meta_data_path.to_string_lossy()))
            .output()
            .await?;

        if !output.status.success() {
            return Err(CloudInitError::CommandError {
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }
    }

    Ok(VmInstanceCloudInit {
        cloud_init_image: image_path,
    })
}
