use std::path::Path;

use base64ct::LineEnding;
use russh::keys::ssh_key::{private::Ed25519Keypair, rand_core::OsRng};
use russh::keys::{PrivateKey, PublicKey};

use crate::fs;
use crate::ssh::error::SshError;

#[derive(Clone, Debug)]
pub struct SshKeypair {
    pub public_key: String,
    pub private_key: String,
}

const PRIVKEY_FILE: &str = "id_ed25519";
const PUBKEY_FILE: &str = "id_ed25519.pub";

pub async fn ensure_keypair(directory: &Path) -> Result<SshKeypair, SshError> {
    if has_keypair(directory).await? {
        return load_keypair(directory).await;
    }

    let keypair = generate_keypair()?;

    save_keypair(&keypair, directory).await?;

    Ok(keypair)
}

pub fn generate_keypair() -> Result<SshKeypair, SshError> {
    let ed25519 = Ed25519Keypair::random(&mut OsRng);

    let public_key = PublicKey::from(ed25519.public)
        .to_openssh()
        .map_err(|e| SshError::KeyEncoding(e.to_string()))?;

    let private_key = PrivateKey::from(ed25519)
        .to_openssh(LineEnding::default())
        .map_err(|e| SshError::KeyEncoding(e.to_string()))?
        .to_string();

    Ok(SshKeypair {
        public_key,
        private_key,
    })
}

pub async fn save_keypair(keypair: &SshKeypair, directory: &Path) -> Result<(), SshError> {
    fs::create_dir(directory).await?;

    let privkey_path = directory.join(PRIVKEY_FILE);
    let pubkey_path = directory.join(PUBKEY_FILE);

    fs::write_file(&pubkey_path, keypair.public_key.as_bytes()).await?;
    fs::write_file(&privkey_path, keypair.private_key.as_bytes()).await?;

    // Restrict permissions on the private key to 0600.
    fs::set_file_mode(&privkey_path, 0o600).await?;

    Ok(())
}

pub async fn has_keypair(directory: &Path) -> Result<bool, SshError> {
    let privkey_path = directory.join(PRIVKEY_FILE);
    let pubkey_path = directory.join(PUBKEY_FILE);

    let public_key_exists = fs::path_exists(&pubkey_path).await?;
    let private_key_exists = fs::path_exists(&privkey_path).await?;

    Ok(public_key_exists && private_key_exists)
}

pub async fn load_keypair(directory: &Path) -> Result<SshKeypair, SshError> {
    let privkey_path = directory.join(PRIVKEY_FILE);
    let pubkey_path = directory.join(PUBKEY_FILE);

    let public_key = fs::read_file_to_string(&pubkey_path).await?;
    let private_key = fs::read_file_to_string(&privkey_path).await?;

    Ok(SshKeypair {
        public_key,
        private_key,
    })
}
