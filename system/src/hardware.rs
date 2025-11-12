use std::{fmt::Display, ops::Div};

use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CpuCount(u16);

impl CpuCount {
    pub fn new(count: u16) -> Self {
        Self(count)
    }
}

impl Display for CpuCount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct MemorySize(u64); // In bytes

impl MemorySize {
    pub fn new(size: u64) -> Self {
        Self(size)
    }
}

impl Display for MemorySize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Div<u64> for MemorySize {
    type Output = MemorySize;

    fn div(self, rhs: u64) -> Self::Output {
        Self(self.0 / rhs)
    }
}
