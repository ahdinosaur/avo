mod cloud_init;
mod kernel;
mod overlay;
mod ovmf;

use avo_machine::Machine;
use avo_system::{Arch, Linux};
use std::path::PathBuf;
use thiserror::Error;

use crate::{
    context::Context,
    fs::{self, FsError},
    image::{get_image, VmImage, VmImageError},
    instance::{
        cloud_init::{setup_cloud_init, CloudInitError, VmInstanceCloudInit},
        kernel::{extract_kernel, ExtractKernelError, VmInstanceKernelDetails},
        overlay::{create_overlay_image, CreateOverlayImageError},
        ovmf::{convert_ovmf_uefi_variables, ConvertOvmfVarsError},
    },
    ssh::{error::SshError, keypair::SshKeypair, port::SshPort},
};

#[derive(Error, Debug)]
pub enum SetupInstanceError {
    #[error(transparent)]
    Image(#[from] VmImageError),

    #[error(transparent)]
    ConvertOvmfVars(#[from] ConvertOvmfVarsError),

    #[error(transparent)]
    ExtractKernel(#[from] ExtractKernelError),

    #[error(transparent)]
    CreateOverlayImage(#[from] CreateOverlayImageError),

    #[error(transparent)]
    CloudInit(#[from] CloudInitError),

    #[error(transparent)]
    Fs(#[from] FsError),

    #[error(transparent)]
    Ssh(#[from] SshError),
}

#[derive(Debug, Clone)]
pub struct VmInstance {
    pub id: String,
    pub dir: PathBuf,
    pub arch: Arch,
    pub linux: Linux,
    pub kernel_root: String,
    pub user: String,
    pub overlay_image_path: PathBuf,
    pub ovmf_vars_path: PathBuf,
    pub kernel_path: PathBuf,
    pub initrd_path: Option<PathBuf>,
    pub ssh_keypair: SshKeypair,
    pub ssh_port: SshPort,
    pub cloud_init_image: PathBuf,
}

pub async fn setup_instance(
    ctx: &mut Context,
    instance_id: &str,
    machine: &Machine,
) -> Result<VmInstance, VmInstanceError> {
    let source_image = get_image(ctx, machine).await?;

    let VmImage {
        arch,
        linux,
        image_path,
        kernel_root,
        user,
    } = source_image;

    let instance_dir = ctx.paths().instance_dir(instance_id);
    fs::setup_directory_access(&instance_dir).await?;

    let overlay_image_path = create_overlay_image(ctx.paths(), instance_id, &image_path).await?;
    let ovmf_vars_path = convert_ovmf_uefi_variables(ctx.paths(), instance_id).await?;
    let VmInstanceKernelDetails {
        kernel_path,
        initrd_path,
    } = extract_kernel(ctx, instance_id, &image_path).await?;

    let ssh_keypair = SshKeypair::load_or_create(&instance_dir).await?;
    let ssh_port = SshPort::load_or_create(&instance_dir).await?;

    let VmInstanceCloudInit { cloud_init_image } =
        setup_cloud_init(ctx, instance_id, machine, &ssh_keypair).await?;

    Ok(VmInstance {
        id: instance_id.to_owned(),
        dir: instance_dir,
        arch,
        linux,
        kernel_root,
        user,
        overlay_image_path,
        ovmf_vars_path,
        kernel_path,
        initrd_path,
        ssh_keypair,
        ssh_port,
        cloud_init_image,
    })
}
