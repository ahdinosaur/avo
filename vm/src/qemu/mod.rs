use std::{fmt::Display, net::Ipv4Addr, path::PathBuf, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::machines::VmMachineImage;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PublishPort {
    pub host_ip: Ipv4Addr,
    pub host_port: u32,
    pub vm_port: u32,
}

impl Display for PublishPort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}->{}/tcp",
            self.host_ip, self.host_port, self.vm_port
        )
    }
}

impl FromStr for PublishPort {
    type Err = String;

    fn from_str(src: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = src.split(':').collect();

        if parts[0].is_empty() {
            return Err("Expected format: [[hostip:][hostport]:]vmport".to_string());
        }

        let (host_ip, host_port, vm_port) = match parts.len() {
            // If there's only a single part, it has to be the `vm_port`.
            1 => {
                let host_ip = Ipv4Addr::UNSPECIFIED;
                let host_port = parts[0]
                    .parse()
                    .map_err(|_| format!("'{}' is not a valid port", parts[0]))?;
                let vm_port = parts[0]
                    .parse()
                    .map_err(|_| format!("'{}' is not a valid port", parts[0]))?;

                (host_ip, host_port, vm_port)
            }
            2 => {
                let host_ip = Ipv4Addr::UNSPECIFIED;
                let host_port = parts[0]
                    .parse()
                    .map_err(|_| format!("'{}' is not a valid port", parts[0]))?;
                let vm_port = parts[1]
                    .parse()
                    .map_err(|_| format!("'{}' is not a valid port", parts[1]))?;
                (host_ip, host_port, vm_port)
            }
            3 => {
                let host_ip = parts[0]
                    .parse()
                    .map_err(|_| format!("'{}' is not a valid IPv4", parts[0]))?;
                let vm_port = parts[2]
                    .parse()
                    .map_err(|_| format!("'{}' is not a valid port", parts[2]))?;
                let host_port = if !parts[1].is_empty() {
                    parts[1]
                        .parse()
                        .map_err(|_| format!("'{}' is not a valid port", parts[1]))?
                } else {
                    vm_port
                };
                (host_ip, host_port, vm_port)
            }
            _ => return Err("Expected format: [[hostip:][hostport]:]vmport".to_string()),
        };

        Ok(Self {
            host_ip,
            host_port,
            vm_port,
        })
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

/// Parse a string the format `source:dest`
impl FromStr for BindMount {
    type Err = String;

    fn from_str(src: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = src.split(':').collect();
        if parts.len() != 2 && parts.len() != 3 {
            return Err("Expected format: source:dest[:ro]".to_string());
        }

        let source = PathBuf::from(parts[0]);
        if !source.is_absolute() {
            return Err("source must be an absolute path".to_string());
        }
        if !source.is_dir() {
            return Err("source doesn't exist or isn't a directory".to_string());
        }

        let dest = PathBuf::from(parts[1]);
        if !dest.is_absolute() {
            return Err("dest must be an absolute path".to_string());
        }

        // Last part (ro) is optional so we have to check for that.
        if parts.len() == 3 {
            let options = parts[2];
            if options == "ro" {
                return Ok(BindMount {
                    source,
                    dest,
                    read_only: true,
                });
            } else {
                return Err("Expected format: source:dest[:ro]".to_string());
            }
        }

        Ok(BindMount {
            source,
            dest,
            read_only: false,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PmemMount {
    pub dest: PathBuf,
    pub size: u64,
}

impl FromStr for PmemMount {
    type Err = String;

    fn from_str(src: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = src.split(':').collect();
        if parts.len() != 2 {
            return Err("Expected format: dest:<size>".to_string());
        }

        let dest = PathBuf::from(parts[0]);
        if !dest.is_absolute() {
            return Err("dest must be an absolute path".to_string());
        }

        let size = if let Ok(size) = parts[1].parse() {
            size
        } else {
            return Err("Couldn't parse size as integer".to_string());
        };

        Ok(PmemMount { dest, size })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QemuLaunchOpts {
    pub id: String,
    pub volumes: Vec<BindMount>,
    pub pmems: Vec<PmemMount>,
    pub published_ports: Vec<PublishPort>,
    pub vm_image: VmMachineImage,
    pub ovmf_uefi_vars_path: PathBuf,
    pub show_vm_window: bool,
    pub pubkey: String,
    pub cid: u32,
    pub is_warmup: bool,
    pub disable_kvm: bool,
}
