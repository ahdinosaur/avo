use avo_system::{Arch, CpuCount, Hostname, MemorySize, Os};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Machine<Options> {
    pub hostname: Hostname,
    pub arch: Arch,
    pub os: Os,
    pub options: Options,
}
