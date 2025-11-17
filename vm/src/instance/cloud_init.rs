use avo_machine::Machine;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;
use tokio::process::Command;

use crate::{
    context::Context,
    fs::{self, FsError},
    ssh::keypair::SshKeypair,
};

#[derive(Debug, Clone)]
pub struct VmInstanceCloudInit {
    pub cloud_init_image: PathBuf,
}

#[derive(Error, Debug)]
pub enum CloudInitError {
    #[error(transparent)]
    Fs(#[from] FsError),

    #[error(transparent)]
    Yaml(#[from] serde_yml::Error),

    #[error(transparent)]
    SshKey(#[from] russh::keys::ssh_key::Error),

    #[error("failed to get output from `mkisofs ...`")]
    CommandOutput(#[from] tokio::io::Error),

    #[error("mkisofs failed")]
    CommandError { stderr: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudInitMetaData {
    instance_id: String,
    local_hostname: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudInitUserData {
    hostname: String,
    ssh_authorized_keys: Vec<String>,
    packages: Vec<String>,
}

pub async fn setup_cloud_init(
    ctx: &mut Context,
    instance_id: &str,
    machine: &Machine,
    ssh_keypair: &SshKeypair,
) -> Result<VmInstanceCloudInit, CloudInitError> {
    let paths = ctx.paths();

    let hostname = machine.hostname.clone();

    let meta_data_path = paths.cloud_init_meta_data_file(instance_id);
    let user_data_path = paths.cloud_init_user_data_file(instance_id);
    let image_path = paths.cloud_init_image_file(instance_id);

    if !fs::path_exists(&meta_data_path).await? {
        let meta_data = CloudInitMetaData {
            instance_id: instance_id.to_owned(),
            local_hostname: hostname.to_string(),
        };
        fs::write_file(
            &meta_data_path,
            serde_yml::to_string(&meta_data)?.as_bytes(),
        )
        .await?;
    }

    if !fs::path_exists(&user_data_path).await? {
        let user_data = CloudInitUserData {
            hostname: hostname.to_string(),
            ssh_authorized_keys: vec![ssh_keypair.public_key.to_openssh()?],
            packages: vec!["openssh".to_owned()],
        };
        fs::write_file(
            &user_data_path,
            format!("#cloud-config\n{}", serde_yml::to_string(&user_data)?).as_bytes(),
        )
        .await?;
    }

    if !fs::path_exists(&image_path).await? {
        let output = Command::new(ctx.executables().mkisofs())
            .arg("-RJ")
            .arg("-V")
            .arg("cidata")
            .arg("-o")
            .arg(&image_path)
            .arg("-graft-points")
            .arg(format!("/meta-data={}", meta_data_path.to_string_lossy()))
            .arg(format!("/user-data={}", user_data_path.to_string_lossy()))
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
