use avo_system::{Arch, CpuCount, Hostname, MemorySize, Os};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Machine {
    pub hostname: Hostname,
    pub arch: Arch,
    pub os: Os,
    pub vm: MachineVmOptions,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct MachineVmOptions {
    pub memory_size: Option<MemorySize>,
    pub cpu_count: Option<CpuCount>,
}
