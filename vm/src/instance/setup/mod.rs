mod cloud_init;
mod kernel;
mod overlay;
mod ovmf;

use avo_machine::{Machine, MachineVmOptions};
use thiserror::Error;

use crate::{
    context::Context,
    fs::{self, FsError},
    image::{get_image, VmImage, VmImageError},
    instance::{
        setup::{
            cloud_init::{setup_cloud_init, CloudInitError},
            kernel::{setup_kernel, ExtractKernelError, VmInstanceKernelDetails},
            overlay::{setup_overlay, CreateOverlayImageError},
            ovmf::{setup_ovmf_uefi_variables, ConvertOvmfVarsError},
        },
        Instance, InstancePaths, VmPort, VmVolume,
    },
    ssh::{error::SshError, keypair::SshKeypair},
    utils::get_free_tcp_port,
};

pub struct InstanceSetupOptions<'a> {
    instance_id: &'a str,
    machine: &'a Machine,
    ports: Vec<VmPort>,
    volumes: Vec<VmVolume>,
}

#[derive(Error, Debug)]
pub enum InstanceSetupError {
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

    #[error("no open ports available")]
    NoOpenPortsAvailable,
}

pub async fn setup_instance(
    ctx: &mut Context,
    options: InstanceSetupOptions<'_>,
) -> Result<Instance, InstanceSetupError> {
    let InstanceSetupOptions {
        instance_id,
        machine,
        ports,
        volumes,
    } = options;

    let source_image = get_image(ctx, machine).await?;

    let MachineVmOptions {
        memory_size,
        cpu_count,
        graphics,
    } = machine.vm.clone().unwrap_or_default();

    let VmImage {
        arch,
        linux,
        image_path: source_image_path,
        kernel_root,
        user,
    } = source_image;

    let instance_dir = ctx.paths().instance_dir(instance_id);
    fs::setup_directory_access(&instance_dir).await?;

    let executables = ctx.executables();
    let instance_paths = InstancePaths::new(&instance_dir);

    setup_overlay(&instance_paths, &source_image_path).await?;
    setup_ovmf_uefi_variables(executables, &instance_paths).await?;

    let VmInstanceKernelDetails { has_initrd } =
        setup_kernel(executables, &instance_paths, &source_image_path).await?;

    let ssh_keypair = SshKeypair::load_or_create(&instance_dir).await?;
    let ssh_port = get_free_tcp_port().ok_or(InstanceSetupError::NoOpenPortsAvailable)?;

    setup_cloud_init(
        executables,
        &instance_paths,
        instance_id,
        &machine.hostname,
        &ssh_keypair.public_key,
    )
    .await?;

    Ok(Instance {
        id: instance_id.to_owned(),
        dir: instance_dir,
        arch,
        linux,
        kernel_root,
        user,
        has_initrd,
        ssh_port,
        memory_size,
        cpu_count,
        volumes,
        ports,
        graphics,
        // TODO set via global avo config
        kvm: None,
    })
}
