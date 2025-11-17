mod virtiofsd;

use base64ct::{Base64, Encoding};
use std::fmt::Write;
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

#[derive(Debug)]
pub struct Qemu {
    command: Command,
    // Keep virtiofsd processes alive as long as Qemu is alive.
    virtiofsd_handles: Vec<Child>,
    // Fstab entries to be injected via SMBIOS credentials.
    fstab_entries: Vec<String>,
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

    /// Returns a debug string for the underlying command.
    pub fn debug_command(&self) -> String {
        format!("{:?}", &self.command)
    }

    /// Set env var for QEMU process.
    pub fn env(mut self, k: &str, v: &str) -> Self {
        self.command.env(k, v);
        self
    }

    /// Disable HPET to decrease idle CPU usage: -machine hpet=off
    pub fn machine_hpet_off(mut self) -> Self {
        self.command.args(["-machine", "hpet=off"]);
        self
    }

    /// Set CPU count: -smp <n>
    pub fn smp(mut self, cpus: impl ToString) -> Self {
        self.command.args(["-smp", &cpus.to_string()]);
        self
    }

    /// Kernel, append, and optional initrd.
    pub fn kernel(mut self, kernel_path: &str) -> Self {
        self.command.args(["-kernel", kernel_path]);
        self
    }

    pub fn kernel_append(mut self, append: &str) -> Self {
        self.command.args(["-append", append]);
        self
    }

    pub fn initrd(mut self, initrd_path: &str) -> Self {
        self.command.args(["-initrd", initrd_path]);
        self
    }

    /// Add a virtio drive with explicit node name, format and file path.
    pub fn add_virtio_drive(mut self, node_name: &str, format: &str, file: &str) -> Self {
        self.command.args([
            "-drive",
            &format!("if=virtio,node-name={node_name},format={format},file={file}"),
        ]);
        self
    }

    /// Add UEFI pflash code and vars drives.
    pub fn uefi_pflash(mut self, code_path: &str, vars_path: &str) -> Self {
        self.command
            .args([
                "-drive",
                &format!("if=pflash,format=raw,unit=0,file={code_path},readonly=on"),
            ])
            .args([
                "-drive",
                &format!("if=pflash,format=qcow2,unit=1,file={vars_path}"),
            ]);
        self
    }

    /// QMP over UNIX socket: -qmp unix:<path_with_opts>
    /// Example path_with_opts: "/path/qmp.sock,server,wait=off"
    pub fn qmp_unix(mut self, path_with_opts: &str) -> Self {
        self.command
            .args(["-qmp", &format!("unix:{path_with_opts}")]);
        self
    }

    /// Add user-mode NIC with model 'virtio' and hostfwd rules based on VmPort.
    pub fn user_nic_model_virtio_hostfwd(mut self, ports: &[VmPort]) -> Self {
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

    /// Enable virtio balloon with free-page-reporting.
    pub fn virtio_balloon_free_page_reporting(mut self) -> Self {
        self.command
            .args(["-device", "virtio-balloon,free-page-reporting=on"]);
        self
    }

    /// Configure memory: -m <GB> and memfd NUMA backend for that size.
    pub fn memory_memfd_numa_gb(mut self, memory_gb: u64) -> Self {
        self.command
            .args(["-m", &format!("{memory_gb}G")])
            .args([
                "-object",
                &format!("memory-backend-memfd,id=mem0,merge=on,share=on,size={memory_gb}G"),
            ])
            .args(["-numa", "node,memdev=mem0"]);
        self
    }

    /// Enable KVM accelerator and use host CPU.
    pub fn accel_kvm_cpu_host(mut self, enabled: bool) -> Self {
        if enabled {
            self.command.args(["-accel", "kvm"]).args(["-cpu", "host"]);
        }
        self
    }

    /// Use -nographic to disable the GUI window.
    pub fn nographic(mut self, enabled: bool) -> Self {
        if enabled {
            self.command.arg("-nographic");
        }
        self
    }

    /// Add a virtiofsd-backed volume:
    /// - launches virtiofsd and retains its Child handle
    /// - adds the vhost-user-fs-pci device and chardev
    /// - registers an fstab entry to later inject via SMBIOS
    pub async fn add_virtiofsd_volume(
        &mut self,
        executables: &ExecutablePaths,
        instance_dir: &Path,
        vol: &VmVolume,
    ) -> Result<(), QemuError> {
        debug!("Launching virtiofsd for volume: {}", vol);

        let child = launch_virtiofsd(executables, instance_dir, vol)
            .await
            .map_err(|error| QemuError::Virtiofsd {
                volume: vol.clone(),
                source: error,
            })?;

        self.virtiofsd_handles.push(child);

        let socket_path = instance_dir.join(vol.socket_name());
        let socket_path_str = socket_path.to_string_lossy();
        let tag = vol.tag();

        // Make the mount read-only if requested.
        let dest_path = vol.dest.to_string_lossy();
        let read_only = if vol.read_only { ",ro" } else { "" };
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

        Ok(())
    }

    /// Inject fstab entries via SMBIOS type 11 credentials as a base64 blob.
    pub fn inject_fstab_smbios(mut self) -> Self {
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

    /// Finalize stdio+spawn. Keeps kill_on_drop true; caller should keep
    /// Qemu alive as long as QEMU (and virtiofsd) must run.
    pub async fn spawn(self) -> Result<Child, std::io::Error> {
        let mut command = self.command;

        // Ensure we inject pending SMBIOS creds if needed.
        if !self.fstab_entries.is_empty() {
            let fstab = self.fstab_entries.join("\n");
            let fstab_base64 = Base64::encode_string(fstab.as_bytes());
            command.args([
                "-smbios",
                &format!("type=11,value=io.systemd.credential.binary:fstab.extra={fstab_base64}"),
            ]);
        }

        command
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
    }
}

pub struct QemuHandle {
    pid: u32,
}
