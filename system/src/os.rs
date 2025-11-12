use std::fmt::{self, Display, Formatter};

use serde::{de, Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
#[non_exhaustive]
pub enum Os {
    Linux(Linux),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "linux")]
#[non_exhaustive]
pub enum Linux {
    Ubuntu {
        #[serde(deserialize_with = "validate_ubuntu_version")]
        #[serde(rename = "ubuntu")]
        version: String,
    },
    Debian {
        #[serde(rename = "debian")]
        version: u8,
    },
    Arch, // no version
}

impl Display for Linux {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Linux::Ubuntu { version } => write!(f, "ubuntu-{}", version),
            Linux::Debian { version } => write!(f, "debian-{}", version),
            Linux::Arch => write!(f, "arch"),
        }
    }
}

impl Display for Os {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Os::Linux(l) => write!(f, "linux-{}", l),
        }
    }
}

fn validate_ubuntu_version<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: de::Deserializer<'de>,
{
    // Manual validation for "YY.MM"
    let s = String::deserialize(deserializer)?;

    // Must be exactly two digits, a dot, then two digits
    let mut parts = s.split('.');
    let (Some(yy), Some(mm), None) = (parts.next(), parts.next(), parts.next()) else {
        return Err(de::Error::custom("Ubuntu: expected YY.MM"));
    };

    let year_ok = yy.len() == 2 && yy.chars().all(|c| c.is_ascii_digit());
    let month_ok = mm.len() == 2
        && mm.chars().all(|c| c.is_ascii_digit())
        && matches!(mm.parse::<u8>(), Ok(1..=12));

    if year_ok && month_ok {
        Ok(s)
    } else {
        Err(de::Error::custom(
            "invalid Ubuntu version (expected YY.MM, with 01-12 for MM)",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::from_str;

    #[test]
    fn ubuntu_valid() {
        let j = r#"{
            "type": "Linux",
            "linux": "Ubuntu",
            "ubuntu": "22.04"
        }"#;
        let os: Os = from_str(j).unwrap();
        assert_eq!(os.to_string(), "linux-ubuntu-22.04");
    }

    #[test]
    fn ubuntu_invalid_month() {
        let j = r#"{
            "type": "Linux",
            "linux": "Ubuntu",
            "ubuntu": "22.15"
        }"#;
        let err = serde_json::from_str::<Os>(j).unwrap_err();
        assert!(err.to_string().contains("invalid Ubuntu version"));
    }

    #[test]
    fn debian_u8() {
        let j = r#"{
            "type": "Linux",
            "linux": "Debian",
            "debian": 12
        }"#;
        let os: Os = from_str(j).unwrap();
        assert_eq!(os.to_string(), "linux-debian-12");
    }

    #[test]
    fn arch_unit_variant() {
        let j = r#"{
            "type": "Linux",
            "linux": "Arch"
        }"#;
        let os: Os = from_str(j).unwrap();
        assert_eq!(os.to_string(), "linux-arch");
    }
}
