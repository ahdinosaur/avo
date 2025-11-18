use base64ct::LineEnding;
use russh::keys::ssh_key::{private::Ed25519Keypair, rand_core::OsRng};
use russh::keys::{PrivateKey, PublicKey};
use std::path::Path;

use crate::fs;

use super::SshError;

#[derive(Clone, Debug)]
pub struct SshKeypair {
    pub public_key: PublicKey,
    pub private_key: PrivateKey,
}

const PRIVATE_KEY_FILE: &str = "id_ed25519";
const PUBLIC_KEY_FILE: &str = "id_ed25519.pub";

impl SshKeypair {
    pub async fn load_or_create(directory: &Path) -> Result<Self, SshError> {
        if Self::exists(directory).await? {
            return Self::load(directory).await;
        }

        let keypair = Self::create()?;

        keypair.save(directory).await?;

        Ok(keypair)
    }

    pub fn create() -> Result<Self, SshError> {
        let ed25519 = Ed25519Keypair::random(&mut OsRng);

        let public_key = PublicKey::from(ed25519.public);
        let private_key = PrivateKey::from(ed25519);

        Ok(Self {
            public_key,
            private_key,
        })
    }

    pub async fn save(&self, directory: &Path) -> Result<(), SshError> {
        fs::setup_directory_access(directory).await?;

        let public_key_path = directory.join(PUBLIC_KEY_FILE);
        let private_key_path = directory.join(PRIVATE_KEY_FILE);

        let public_key_string = self.public_key.to_openssh()?;
        let private_key_string = self.private_key.to_openssh(LineEnding::default())?;

        fs::write_file(&public_key_path, public_key_string.as_bytes()).await?;
        fs::write_file(&private_key_path, private_key_string.as_bytes()).await?;

        // Restrict permissions on the private key to 0600.
        fs::set_file_mode(&private_key_path, 0o600).await?;

        Ok(())
    }

    pub async fn exists(directory: &Path) -> Result<bool, SshError> {
        let public_key_path = directory.join(PUBLIC_KEY_FILE);
        let private_key_path = directory.join(PRIVATE_KEY_FILE);

        let public_key_exists = fs::path_exists(&public_key_path).await?;
        let private_key_exists = fs::path_exists(&private_key_path).await?;

        Ok(public_key_exists && private_key_exists)
    }

    pub async fn load(directory: &Path) -> Result<Self, SshError> {
        let public_key_path = directory.join(PUBLIC_KEY_FILE);
        let private_key_path = directory.join(PRIVATE_KEY_FILE);

        let public_key_string = fs::read_file_to_string(&public_key_path).await?;
        let private_key_string = fs::read_file_to_string(&private_key_path).await?;

        let public_key = PublicKey::from_openssh(&public_key_string)?;
        let private_key = PrivateKey::from_openssh(&private_key_string)?;

        Ok(Self {
            public_key,
            private_key,
        })
    }
}
