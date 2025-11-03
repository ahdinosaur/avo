use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Hostname(String);

impl AsRef<str> for Hostname {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
