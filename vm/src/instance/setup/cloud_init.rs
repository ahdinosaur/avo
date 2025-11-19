use ludis_system::Hostname;
use russh::keys::PublicKey;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    cmd::{Command, CommandError},
    fs::{self, FsError},
    instance::InstancePaths,
    paths::ExecutablePaths,
};

#[derive(Error, Debug)]
pub enum CloudInitError {
    #[error(transparent)]
    Fs(#[from] FsError),

    #[error(transparent)]
    Yaml(#[from] serde_saphyr::ser_error::Error),

    #[error(transparent)]
    SshKey(#[from] russh::keys::ssh_key::Error),

    #[error(transparent)]
    Command(#[from] CommandError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CloudInitMetaData {
    instance_id: String,
    local_hostname: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CloudInitUserData {
    hostname: String,
    ssh_authorized_keys: Vec<String>,
    packages: Vec<String>,
}

pub(super) async fn setup_cloud_init(
    executables: &ExecutablePaths,
    paths: &InstancePaths<'_>,
    instance_id: &str,
    hostname: &Hostname,
    ssh_public_key: &PublicKey,
) -> Result<(), CloudInitError> {
    let meta_data_path = paths.cloud_init_meta_data_path();
    let user_data_path = paths.cloud_init_user_data_path();
    let image_path = paths.cloud_init_image_path();

    if !fs::path_exists(&meta_data_path).await? {
        let meta_data = CloudInitMetaData {
            instance_id: instance_id.to_owned(),
            local_hostname: hostname.to_string(),
        };
        fs::write_file(
            &meta_data_path,
            serde_saphyr::to_string(&meta_data)?.as_bytes(),
        )
        .await?;
    }

    if !fs::path_exists(&user_data_path).await? {
        let user_data = CloudInitUserData {
            hostname: hostname.to_string(),
            ssh_authorized_keys: vec![ssh_public_key.to_openssh()?],
            packages: vec!["openssh".to_owned()],
        };
        fs::write_file(
            &user_data_path,
            format!("#cloud-config\n{}", serde_saphyr::to_string(&user_data)?).as_bytes(),
        )
        .await?;
    }

    if !fs::path_exists(&image_path).await? {
        Command::new(executables.mkisofs())
            .arg("-RJ")
            .arg("-V")
            .arg("cidata")
            .arg("-o")
            .arg(&image_path)
            .arg("-graft-points")
            .arg(format!("/meta-data={}", meta_data_path.to_string_lossy()))
            .arg(format!("/user-data={}", user_data_path.to_string_lossy()))
            .run()
            .await?;
    }

    Ok(())
}
