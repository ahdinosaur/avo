#![allow(dead_code)]

pub enum Os {
    Linux(Linux),
}

pub enum Linux {
    Ubuntu,
    Debian,
    Arch,
}

pub struct OsVersion(String);

pub enum Bitness {
    X64,
}

pub enum Arch {
    X86_64,
    Aarch64,
}

pub struct System {
    id: String,
    os: Os,
    os_version: OsVersion,
    bitness: Bitness,
    arch: Arch,
    hostname: String,
    domain: String,
}

// The part of the System that can be expressed as part of the node metadata.
pub struct UpdateSystem {
    hostname: String,
    domain: String,
}
