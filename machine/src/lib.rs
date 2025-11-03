use avo_system::{Arch, CpuCount, Hostname, MemorySize, Os};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Machine {
    hostname: Hostname,
    os: Os,
    arch: Arch,
    memory_size: MemorySize,
    cpu_count: CpuCount,
}
