use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Os {
    Linux(Linux),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "linux")]
pub enum Linux {
    Ubuntu { version: String },
    Debian { version: String },
    Arch { version: String },
}
