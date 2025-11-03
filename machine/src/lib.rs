use serde::{Deserialize, Serialize};

mod arch;
mod hardware;
mod os;

pub use arch::*;
pub use hardware::*;
pub use os::*;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Machine {
    hostname: String,
    os: Os,
    arch: Arch,
    memory_size: MemorySize,
    cpu_count: CpuCount,
}
