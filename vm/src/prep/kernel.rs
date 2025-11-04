
/// Extract the kernel and initrd from a given image
///
/// It will extract it into the same dir of the `image_path`.
///
/// Source: https://gitlab.archlinux.org/archlinux/vmexec/-/blob/03b649bdbcdc64d30b2943f61b51165f390b920d/src/qemu.rs#L48-91
pub async fn extract_kernel(paths: &Paths, vm_image: &VmImage) -> Result<()> {
    let kernel_path = image_path.join("vmlinuz-linux");
    let initrd_path = if matches!(linux, Linux::Arch) {
        None
    } else {
        Some(image_path.join("initramfs-linux,img"))
    };

    let dest_dir = vm_image
        .image_path
        .parent()
        .ok_or_eyre("Image {image_path:?} doesn't have a parent")?;
    let mut virt_copy_out_cmd = Command::new(virt_copy_out_path);

    let files_to_extract = if let Some(initrd) = &vm_image.initrd_path {
        vec![
            format!("/boot/{}", initrd.file_name().unwrap().to_string_lossy()),
            format!(
                "/boot/{}",
                vm_image.kernel_path.file_name().unwrap().to_string_lossy()
            ),
        ]
    } else {
        vec![format!(
            "/boot/{}",
            vm_image.kernel_path.file_name().unwrap().to_string_lossy()
        )]
    };

    virt_copy_out_cmd
        .args(["-a", &vm_image.image_path.to_string_lossy()])
        .args(files_to_extract)
        .arg(dest_dir);

    let virt_copy_out_cmd_str = command_as_string(&virt_copy_out_cmd);
    info!("Extracting kernel from {:?}", vm_image.image_path);
    debug!("{virt_copy_out_cmd_str}");

    let virt_copy_out_output = virt_copy_out_cmd.output().await?;
    if !virt_copy_out_output.status.success() {
        bail!(
            "virt_copy_out failed: {}",
            String::from_utf8_lossy(&virt_copy_out_output.stderr)
        );
    }

    Ok(())
}
