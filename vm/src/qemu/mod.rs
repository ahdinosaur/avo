mod virtiofsd;

use base64ct::{Base64, Encoding};
use std::fmt::{Debug, Write};
use std::{ffi::OsStr, net::Ipv4Addr, path::Path};
use thiserror::Error;
use tokio::process::{Child, Command};
use tracing::debug;

use self::virtiofsd::{launch_virtiofsd, LaunchVirtiofsdError};
use crate::{
    emulator::{VmPort, VmVolume},
    paths::ExecutablePaths,
};

#[derive(Error, Debug)]
pub enum QemuError {
    #[error("failed to launch virtiofsd for volume {volume}: {source}")]
    Virtiofsd {
        volume: VmVolume,
        #[source]
        source: LaunchVirtiofsdError,
    },

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub struct Qemu {
    command: Command,
    // Keep virtiofsd processes alive as long as Qemu is alive.
    virtiofsd_handles: Vec<Child>,
    // Fstab entries to be injected via SMBIOS credentials.
    fstab_entries: Vec<String>,
}

impl Debug for Qemu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.command.fmt(f)
    }
}

impl Qemu {
    /// Create a new emulator for a specific QEMU binary path.
    pub fn new<S: AsRef<OsStr>>(qemu_binary: S) -> Qemu {
        let command = Command::new(qemu_binary);
        Qemu {
            command,
            virtiofsd_handles: Vec::new(),
            fstab_entries: Vec::new(),
        }
    }

    pub fn easy(&mut self) -> &mut Self {
        // Disable HPET to decrease idle CPU usage: -machine hpet=off
        self.command.args(["-machine", "hpet=off"]);

        // Enable virtio balloon with free-page-reporting.
        self.command
            .args(["-device", "virtio-balloon,free-page-reporting=on"]);

        self
    }

    // Enable KVM accelerator.
    pub fn kvm(&mut self, enabled: bool) -> &mut Self {
        if enabled {
            self.command.args(["-accel", "kvm"]).args(["-cpu", "host"]);
        }
        self
    }

    /// Set env var for QEMU process.
    pub fn env(&mut self, k: &str, v: &str) -> &mut Self {
        self.command.env(k, v);
        self
    }

    /// Set CPU count: -smp <n>
    pub fn cpu_count(&mut self, cpus: impl ToString) -> &mut Self {
        self.command.args(["-smp", &cpus.to_string()]);
        self
    }

    /// Configure memory: -m <GB> and memfd NUMA backend for that size.
    pub fn memory(&mut self, memory_in_gb: u64) -> &mut Self {
        self.command
            .args(["-m", &format!("{memory_in_gb}G")])
            .args([
                "-object",
                &format!("memory-backend-memfd,id=mem0,merge=on,share=on,size={memory_in_gb}G"),
            ])
            .args(["-numa", "node,memdev=mem0"]);

        self
    }

    /// Kernel, append, and optional initrd.
    pub fn kernel(&mut self, kernel_path: &Path, kernel_args: Option<&str>) -> &mut Self {
        self.command
            .args(["-kernel", &kernel_path.to_string_lossy()]);
        if let Some(kernel_args) = kernel_args {
            self.command.args(["-append", kernel_args]);
        }

        self
    }

    pub fn initrd(&mut self, initrd_path: &Path) -> &mut Self {
        self.command
            .args(["-initrd", &initrd_path.to_string_lossy()]);

        self
    }

    /// Add a virtio drive with explicit node name, format and file path.
    pub fn virtio_drive(&mut self, node_name: &str, format: &str, file: &Path) -> &mut Self {
        let file = file.display();
        self.command.args([
            "-drive",
            &format!("if=virtio,node-name={node_name},format={format},file={file}"),
        ]);

        self
    }

    /// Add UEFI pflash code and vars drives.
    pub fn plash_drives(&mut self, code_path: &Path, vars_path: &Path) -> &mut Self {
        let code_path = code_path.display();
        let vars_path = vars_path.display();
        self.command
            .args([
                "-drive",
                &format!("if=pflash,format=raw,unit=0,file={code_path},readonly=on",),
            ])
            .args([
                "-drive",
                &format!("if=pflash,format=qcow2,unit=1,file={vars_path}"),
            ]);

        self
    }

    /// QMP over UNIX socket.
    pub fn qmp_socket(&mut self, qmp_socket_path: &Path) -> &mut Self {
        let qmp_socket_path = qmp_socket_path.display();
        self.command
            .args(["-qmp", &format!("unix:{qmp_socket_path},server,wait=off")]);
        self
    }

    /// Add user-mode NIC with model 'virtio' and hostfwd rules based on VmPort.
    pub fn ports(&mut self, ports: &[VmPort]) -> &mut Self {
        let hostfwd: String = ports.iter().fold(String::new(), |mut s, p| {
            let _ = write!(
                s,
                ",hostfwd=:{}:{}-:{}",
                p.host_ip.unwrap_or(Ipv4Addr::UNSPECIFIED),
                p.host_port.unwrap_or(p.vm_port),
                p.vm_port
            );
            s
        });
        self.command
            .args(["-nic", &format!("user,model=virtio{hostfwd}")]);

        self
    }

    pub fn pid_file<P: AsRef<Path>>(&mut self, path: P) -> &mut Self {
        self.command.arg("-pidfile").arg(path.as_ref());
        self
    }

    /// Use -nographic to disable the GUI window.
    pub fn nographic(&mut self, enabled: bool) -> &mut Self {
        if enabled {
            self.command.arg("-nographic");
        }

        self
    }

    /// Add a virtiofsd-backed volume:
    /// - launches virtiofsd and retains its Child handle
    /// - adds the vhost-user-fs-pci device and chardev
    /// - registers an fstab entry to later inject via SMBIOS
    pub async fn volume(
        &mut self,
        executables: &ExecutablePaths,
        instance_dir: &Path,
        volume: &VmVolume,
    ) -> Result<&mut Self, QemuError> {
        debug!("Launching virtiofsd for volume: {}", volume);

        let child = launch_virtiofsd(executables, instance_dir, volume)
            .await
            .map_err(|error| QemuError::Virtiofsd {
                volume: volume.clone(),
                source: error,
            })?;

        self.virtiofsd_handles.push(child);

        let socket_path = instance_dir.join(volume.socket_name());
        let socket_path_str = socket_path.to_string_lossy();
        let tag = volume.tag();

        // Make the mount read-only if requested.
        let dest_path = volume.dest.to_string_lossy();
        let read_only = if volume.read_only { ",ro" } else { "" };
        let fstab_entry = format!("{tag} {dest_path} virtiofs defaults{read_only} 0 0");
        self.fstab_entries.push(fstab_entry);

        // Use a sequential chardev id.
        let idx = self.virtiofsd_handles.len() - 1;
        self.command
            .args([
                "-chardev",
                &format!("socket,id=char{idx},path={socket_path_str}"),
            ])
            .args([
                "-device",
                &format!("vhost-user-fs-pci,chardev=char{idx},tag={tag}"),
            ]);

        Ok(self)
    }

    /// Inject fstab entries via SMBIOS type 11 credentials as a base64 blob.
    pub fn inject_fstab_smbios(&mut self) -> &mut Self {
        if !self.fstab_entries.is_empty() {
            let fstab = self.fstab_entries.join("\n");
            let fstab_base64 = Base64::encode_string(fstab.as_bytes());
            self.command.args([
                "-smbios",
                &format!("type=11,value=io.systemd.credential.binary:fstab.extra={fstab_base64}"),
            ]);
        }

        self
    }

    pub async fn spawn(self) -> Result<Child, QemuError> {
        let mut command = self.command;

        command.arg("-daemonize");

        let child = command.spawn()?;

        Ok(child)
    }
}
