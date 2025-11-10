use std::{
    fmt::{Display, Write},
    net::Ipv4Addr,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use avo_machine::MachineVmOptions;
use avo_system::{CpuCount, MemorySize};
use base64ct::{Base64, Encoding};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{fs, process::Command};
use tracing::{debug, error, info};

use crate::{
    machines::VmMachineImage,
    paths::{ExecutablePaths, Paths},
    qemu::virtiofsd::launch_virtiofsd,
    run::CancellationTokens,
    utils::escape_path,
};

mod virtiofsd;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PublishPort {
    pub host_ip: Option<Ipv4Addr>,
    pub host_port: Option<u32>,
    pub vm_port: u32,
}

impl std::fmt::Display for PublishPort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut wrote_left = false;

        if let Some(ip) = self.host_ip {
            write!(f, "{}", ip)?;
            wrote_left = true;
        }

        if let Some(port) = self.host_port {
            if wrote_left {
                write!(f, ":")?;
            }
            write!(f, "{}", port)?;
            wrote_left = true;
        }

        if wrote_left {
            write!(f, "->")?;
        }

        write!(f, "{}/tcp", self.vm_port)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindMount {
    pub source: PathBuf,
    pub dest: PathBuf,
    pub read_only: bool,
}

impl BindMount {
    /// Safely printable/escaped path
    pub fn tag(&self) -> String {
        escape_path(&self.dest.to_string_lossy())
    }

    pub fn socket_name(&self) -> String {
        format!("{}.sock", self.tag())
    }
}

impl Display for BindMount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let source = self.source.to_string_lossy();
        let dest = self.dest.to_string_lossy();
        if self.read_only {
            write!(f, "{source}:{dest}:ro")
        } else {
            write!(f, "{source}:{dest}")
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QemuLaunchOpts {
    pub vm: MachineVmOptions,
    pub vm_image: VmMachineImage,
    pub cid: u32,
    pub volumes: Vec<BindMount>,
    pub published_ports: Vec<PublishPort>,
    pub show_vm_window: bool,
    pub ssh_pubkey: String,
    pub disable_kvm: bool,
}

#[derive(Error, Debug)]
pub enum QemuLaunchError {
    #[error("failed to launch virtiofsd for volume {volume}: {error}")]
    VirtiofsdLaunch { volume: BindMount, error: String },

    #[error("QEMU has no pid, maybe it exited early?")]
    QemuPidUnavailable,

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),

    #[error("QEMU failed: {stderr}")]
    QemuError { stderr: String },
}

pub async fn launch_qemu(
    paths: &Paths,
    executables: &ExecutablePaths,
    machine_id: &str,
    qemu_launch_opts: QemuLaunchOpts,
    cancellation_tokens: CancellationTokens,
    qemu_should_exit: Arc<AtomicBool>,
    run_dir: &Path,
) -> Result<(), QemuLaunchError> {
    debug!("called launch_qemu()");

    let vm_image = qemu_launch_opts.vm_image;

    #[allow(irrefutable_let_patterns)]
    let VmMachineImage::Linux {
        arch: _,
        linux: _,
        overlay_image_path,
        kernel_path,
        initrd_path,
        ovmf_vars_path,
    } = vm_image
    else {
        unimplemented!();
    };

    let overlay_image_path_str = overlay_image_path.to_string_lossy();
    let kernel_path_str = kernel_path.to_string_lossy();
    let initrd_path_str = initrd_path.map(|p| p.to_string_lossy().into_owned());
    let ovmf_vars_path_str = ovmf_vars_path.to_string_lossy();
    let ovmf_code_system_path_str = paths.ovmf_code_system_file().to_string_lossy();

    let vm = qemu_launch_opts.vm;
    let memory_size = vm
        .memory_size
        .unwrap_or_else(|| MemorySize::new(8 * 1024 * 1024 * 1024));
    let memory_size_in_gb = memory_size / 1024 / 1024 / 1024;
    let cpu_count = vm.cpu_count.unwrap_or_else(|| CpuCount::new(2));

    let ssh_pubkey_base64 = Base64::encode_string(qemu_launch_opts.ssh_pubkey.as_bytes());

    let cid = qemu_launch_opts.cid;

    let hostfwd: String =
        qemu_launch_opts
            .published_ports
            .iter()
            .fold(String::new(), |mut output, p| {
                let _ = write!(
                    output,
                    ",hostfwd=:{}:{}-:{}",
                    p.host_ip.unwrap_or(Ipv4Addr::UNSPECIFIED),
                    p.host_port.unwrap_or(p.vm_port),
                    p.vm_port
                );
                output
            });

    let qmp_socket_path = run_dir.join("qmp.sock,server,wait=off");
    let qmp_socket_path_str = qmp_socket_path.to_string_lossy();

    let mut qemu_cmd = Command::new(executables.qemu_x86_64());
    qemu_cmd
        // Decrease idle CPU usage
        .args(["-machine", "hpet=off"])
        .args(["-smp", &cpu_count.to_string()])
        // We extracted the kernel and initrd from this image earlier in order to boot it more
        // quickly.
        .args(["-kernel", &kernel_path_str])
        .args(["-append", "rw root=/dev/vda1"])
        // SSH port forwarding
        .args([
            "-device",
            &format!("vhost-vsock-pci,id=vhost-vsock-pci0,guest-cid={cid}"),
        ])
        // Network controller
        .args(["-nic", &format!("user,model=virtio{hostfwd}")])
        // Free Page Reporting allows the guest to signal to the host that memory can be reclaimed.
        .args(["-device", "virtio-balloon,free-page-reporting=on"])
        // Memory configuration
        .args(["-m", &format!("{memory_size_in_gb}G")])
        .args([
            "-object",
            &format!(
                "memory-backend-memfd,id=mem0,merge=on,share=on,size={memory_size_in_gb}G"
            ),
        ])
        .args(["-numa", "node,memdev=mem0"])
        // UEFI
        .args([
            "-drive",
            &format!("if=pflash,format=raw,unit=0,file={ovmf_code_system_path_str},readonly=on"),
        ])
        .args([
            "-drive",
            &format!("if=pflash,format=qcow2,unit=1,file={ovmf_vars_path_str}"),
        ])
        // Overlay image
        .args([
            "-drive",
            &format!("if=virtio,node-name=overlay-disk,file={overlay_image_path_str}"),
        ])
        // QMP API to expose QEMU command API
        .args(["-qmp", &format!("unix:{qmp_socket_path_str}")])
        // Here we inject the SSH using systemd.system-credentials, see:
        // https://www.freedesktop.org/software/systemd/man/latest/systemd.system-credentials.html
        .args([
            "-smbios",
            &format!(
                "type=11,value=io.systemd.credential.binary:ssh.authorized_keys.root={ssh_pubkey_base64}"
            ),
        ]);

    if !qemu_launch_opts.disable_kvm {
        qemu_cmd.args(["-accel", "kvm"]).args(["-cpu", "host"]);
    }

    if let Some(initrd_path_str) = initrd_path_str {
        qemu_cmd.args(["-initrd", &initrd_path_str]);
    }

    // It's important we keep `virtiofsd_handles` in scope here and that we don't drop them too
    // early as otherwise the process would exit and the VM would be unhappy.
    let mut virtiofsd_handles = vec![];

    // We need fstab entries for virtiofsd mounts and pmem devices.
    let mut fstab_entries = vec![];

    // Add virtiofsd-based directory shares
    for (i, vol) in qemu_launch_opts.volumes.iter().enumerate() {
        let virtiofsd_child = launch_virtiofsd(paths, executables, machine_id, vol)
            .await
            .map_err(|e| QemuLaunchError::VirtiofsdLaunch {
                volume: vol.clone(),
                error: e.to_string(),
            })?;
        virtiofsd_handles.push(virtiofsd_child);

        let socket_path = run_dir.join(vol.socket_name());
        let socket_path_str = socket_path.to_string_lossy();
        let tag = vol.tag();
        let dest_path = vol.dest.to_string_lossy();
        let read_only = if vol.read_only {
            String::from(",ro")
        } else {
            String::new()
        };
        let fstab_entry = format!("{tag} {dest_path} virtiofs defaults{read_only} 0 0");
        fstab_entries.push(fstab_entry);
        qemu_cmd
            .args([
                "-chardev",
                &format!("socket,id=char{i},path={socket_path_str}"),
            ])
            .args([
                "-device",
                &format!("vhost-user-fs-pci,chardev=char{i},tag={tag}"),
            ]);
    }

    if !fstab_entries.is_empty() {
        let fstab = fstab_entries.join("\n");
        let fstab_base64 = Base64::encode_string(fstab.as_bytes());
        qemu_cmd.args([
            "-smbios",
            &format!("type=11,value=io.systemd.credential.binary:fstab.extra={fstab_base64}"),
        ]);
    }

    if !qemu_launch_opts.show_vm_window {
        qemu_cmd.arg("-nographic");
    }

    info!("run qemu cmd: {:?}", qemu_cmd);

    let qemu_child = qemu_cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    // Write QEMU's pid into a `qemu-pid` file in the run dir. This allows a cleanup job to run and
    // some point and remove all the run dirs that have a dead QEMU (which can happen if vmexec is
    // cancelled at the wrong time).
    let qemu_pid_path = run_dir.join("qemu.pid");
    let qemu_pid = qemu_child
        .id()
        .ok_or(QemuLaunchError::QemuPidUnavailable)?
        .to_string();

    fs::write(&qemu_pid_path, &qemu_pid).await?;

    // trace!("Writing QEMU pid {qemu_pid} to {qemu_pid_path:?}");

    let qemu_output = tokio::select! {
        _ = cancellation_tokens.qemu.cancelled() => {
            // debug!("QEMU task was cancelled");
            return Ok(());
        }
        val = qemu_child.wait_with_output() => {
            if qemu_should_exit.load(Ordering::SeqCst) {
                // info!("QEMU has finished running");
                return Ok(());
            }
            error!("QEMU process exited early, that's usually a bad sign");
            val?
        }
    };

    if !qemu_output.status.success() {
        cancellation_tokens.ssh.cancel();

        return Err(QemuLaunchError::QemuError {
            stderr: String::from_utf8_lossy(&qemu_output.stderr).to_string(),
        });
    }

    Ok(())
}
