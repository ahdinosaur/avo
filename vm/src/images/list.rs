use avo_system::{Arch, Os};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageIndex {
    pub arch: Arch,
    pub os: Os,
    pub image: ImageRef,
    pub hash: ImageHashRef,
}

impl ImageIndex {
    pub fn to_image_file_name(&self) -> String {
        let arch = &self.arch;
        let os = &self.os;
        let ext = self.image.to_extension();
        format!("{arch}_{os}.{ext}")
    }
    pub fn to_hash_file_name(&self) -> String {
        let arch = &self.arch;
        let os = &self.os;
        let ext = self.hash.to_extension();
        format!("{arch}_{os}.{ext}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ImageRef {
    #[serde(rename = "qcow2")]
    Qcow2 { url: String },
}

impl ImageRef {
    pub fn to_url(&self) -> &str {
        match self {
            ImageRef::Qcow2 { url } => url,
        }
    }
    fn to_extension(&self) -> &str {
        match self {
            ImageRef::Qcow2 { url: _ } => "qcow2",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ImageHashRef {
    #[serde(rename = "sha512sums")]
    Sha512Sums { url: String },
}

impl ImageHashRef {
    pub fn to_url(&self) -> &str {
        match self {
            ImageHashRef::Sha512Sums { url } => url,
        }
    }
    fn to_extension(&self) -> &str {
        match self {
            ImageHashRef::Sha512Sums { url: _ } => "sha512sums",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImagesList(HashMap<String, ImageIndex>);

impl ImagesList {
    pub fn into_values(self) -> impl Iterator<Item = ImageIndex> {
        self.0.into_values()
    }
}
