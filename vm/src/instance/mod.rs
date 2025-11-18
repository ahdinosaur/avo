mod exec;
mod paths;
mod setup;
mod start;

pub use self::exec::*;
pub use self::paths::*;
pub use self::setup::*;
pub use self::start::*;

use avo_system::Arch;
use avo_system::CpuCount;
use avo_system::Linux;
use avo_system::MemorySize;
use nix::{
    sys::signal::{kill, Signal},
    unistd::Pid,
};
use serde::{Deserialize, Serialize};
use std::num::ParseIntError;
use std::time::Duration;
use std::{fmt::Display, net::Ipv4Addr, path::PathBuf, str::FromStr};
use thiserror::Error;

use crate::context::Context;
use crate::fs::{self, FsError};
use crate::instance::exec::InstanceExecError;
use crate::ssh::{SshError, SshKeypair};
use crate::utils::is_tcp_port_open;

#[derive(Error, Debug)]
pub enum InstanceError {
    #[error(transparent)]
    Setup(#[from] InstanceSetupError),

    #[error(transparent)]
    Start(#[from] InstanceStartError),

    #[error(transparent)]
    Exec(#[from] InstanceExecError),

    #[error("failed to check whether instance dir exists")]
    DirExists(#[source] fs::FsError),

    #[error("failed to serialize or deserialize state")]
    StateSerde(#[source] serde_json::Error),

    #[error("failed to read state")]
    StateRead(#[source] fs::FsError),

    #[error("failed to write state")]
    StateWrite(#[source] fs::FsError),

    #[error("failed to remove instance dir")]
    RemoveDir(#[source] fs::FsError),

    #[error("failed to read pid")]
    ReadPid(#[source] FsError),

    #[error("failed to parse pid")]
    ParsePid(#[source] ParseIntError),

    #[error("failed to kill pid")]
    KillPid(#[source] nix::errno::Errno),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    pub id: String,
    pub dir: PathBuf,
    pub arch: Arch,
    pub linux: Linux,
    pub kernel_root: String,
    pub user: String,
    pub has_initrd: bool,
    pub ssh_port: u16,
    pub memory_size: Option<MemorySize>,
    pub cpu_count: Option<CpuCount>,
    pub volumes: Vec<VmVolume>,
    pub ports: Vec<VmPort>,
    pub graphics: Option<bool>,
    pub kvm: Option<bool>,
}

impl Instance {
    pub fn paths(&self) -> InstancePaths<'_> {
        InstancePaths::new(&self.dir)
    }

    pub async fn setup(
        ctx: &mut Context,
        options: InstanceSetupOptions<'_>,
    ) -> Result<Self, InstanceError> {
        Ok(setup_instance(ctx, options).await?)
    }

    pub async fn exists(ctx: &mut Context, instance_id: &str) -> Result<bool, InstanceError> {
        let instance_dir = ctx.paths().instance_dir(instance_id);
        let exists = fs::path_exists(instance_dir)
            .await
            .map_err(InstanceError::DirExists)?;
        Ok(exists)
    }

    pub async fn load(ctx: &mut Context, instance_id: &str) -> Result<Self, InstanceError> {
        let instance_dir = ctx.paths().instance_dir(instance_id);
        let paths = InstancePaths::new(&instance_dir);
        let state_path = paths.state();
        let state_str = fs::read_file_to_string(state_path)
            .await
            .map_err(InstanceError::StateRead)?;
        let instance = serde_json::from_str(&state_str).map_err(InstanceError::StateSerde)?;
        Ok(instance)
    }

    pub async fn save(&self) -> Result<(), InstanceError> {
        let state_path = self.paths().state();
        let state = serde_json::to_string_pretty(self).map_err(InstanceError::StateSerde)?;
        fs::write_file(state_path, state.as_bytes())
            .await
            .map_err(InstanceError::StateWrite)?;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn remove(self) -> Result<(), InstanceError> {
        fs::remove_dir(self.dir)
            .await
            .map_err(InstanceError::RemoveDir)?;
        Ok(())
    }

    pub async fn start(&self, ctx: &mut Context) -> Result<(), InstanceError> {
        Ok(instance_start(ctx.executables(), self).await?)
    }

    pub async fn is_qemu_running(&self) -> Result<bool, InstanceError> {
        let pid_exists = fs::path_exists(&self.paths().qemu_pid_path())
            .await
            .map_err(InstanceError::ReadPid)?;
        Ok(pid_exists)
    }

    pub fn is_ssh_open(&self) -> bool {
        is_tcp_port_open(self.ssh_port)
    }

    #[allow(dead_code)]
    pub async fn stop(&self) -> Result<(), InstanceError> {
        let pid_str = fs::read_file_to_string(&self.paths().qemu_pid_path())
            .await
            .map_err(InstanceError::ReadPid)?;
        let pid_int: i32 = FromStr::from_str(&pid_str).map_err(InstanceError::ParsePid)?;
        let pid = Pid::from_raw(pid_int);
        kill(pid, Some(Signal::SIGKILL)).map_err(InstanceError::KillPid)?;
        Ok(())
    }

    pub async fn ssh_keypair(&self) -> Result<SshKeypair, SshError> {
        SshKeypair::load_or_create(&self.dir).await
    }

    pub async fn exec(&self, command: &str, timeout: Duration) -> Result<u32, InstanceError> {
        Ok(instance_exec(self, command, timeout).await?)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VmPort {
    pub host_ip: Option<Ipv4Addr>,
    pub host_port: Option<u16>,
    pub vm_port: u16,
}

impl Display for VmPort {
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
pub struct VmVolume {
    pub source: PathBuf,
    pub dest: PathBuf,
    pub read_only: bool,
}

impl Display for VmVolume {
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
