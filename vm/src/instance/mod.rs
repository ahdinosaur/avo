mod run;
mod setup;

pub use self::run::*;
pub use self::setup::*;

use serde::{Deserialize, Serialize};
use std::{fmt::Display, net::Ipv4Addr, path::PathBuf};

use crate::{instance::VmInstance, utils::escape_path};

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

impl VmVolume {
    pub fn tag(&self) -> String {
        escape_path(&self.dest.to_string_lossy())
    }

    pub fn socket_name(&self) -> String {
        format!("{}.sock", self.tag())
    }
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
