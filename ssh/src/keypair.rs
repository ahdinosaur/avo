use base64ct::LineEnding;
use russh::keys::ssh_key::{private::Ed25519Keypair, rand_core::OsRng};
use russh::keys::{PrivateKey, PublicKey};
use std::path::Path;
use thiserror::Error;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::debug;

#[derive(Error, Debug)]
pub enum SshKeypairError {
    #[error("SSH key encode/decode error: {0}")]
    RusshKey(#[from] russh::keys::ssh_key::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Clone, Debug)]
pub struct SshKeypair {
    pub public_key: PublicKey,
    pub private_key: PrivateKey,
}

const PRIVATE_KEY_FILE: &str = "id_ed25519";
const PUBLIC_KEY_FILE: &str = "id_ed25519.pub";

impl SshKeypair {
    // Load keys from directory if present; otherwise create and save new keys.
    #[tracing::instrument(skip_all)]
    pub async fn load_or_create(directory: &Path) -> Result<Self, SshKeypairError> {
        if Self::exists(directory).await? {
            debug!("SSH keypair exists; loading");
            return Self::load(directory).await;
        }

        debug!("SSH keypair doesn't exist, creating");
        let keypair = Self::create()?;
        keypair.save(directory).await?;
        Ok(keypair)
    }

    // Create a fresh ed25519 keypair.
    #[tracing::instrument(skip_all)]
    pub fn create() -> Result<Self, SshKeypairError> {
        let ed25519 = Ed25519Keypair::random(&mut OsRng);
        let public_key = PublicKey::from(ed25519.public);
        let private_key = PrivateKey::from(ed25519);
        debug!("Created new SSH keypair");
        Ok(Self {
            public_key,
            private_key,
        })
    }

    // Persist keypair in OpenSSH format, setting private key permissions to 0600.
    #[tracing::instrument(skip_all)]
    pub async fn save(&self, directory: &Path) -> Result<(), SshKeypairError> {
        fs::create_dir_all(directory).await?;

        let public_key_path = directory.join(PUBLIC_KEY_FILE);
        let private_key_path = directory.join(PRIVATE_KEY_FILE);

        let public_key_string = self.public_key.to_openssh()?;
        let private_key_string = self.private_key.to_openssh(LineEnding::default())?;

        write_all(&public_key_path, public_key_string.as_bytes()).await?;
        write_all(&private_key_path, private_key_string.as_bytes()).await?;

        set_private_mode_0600(&private_key_path).await?;

        debug!(
            public_key = %public_key_path.display(),
            private_key = %private_key_path.display(),
            "Saved SSH keypair"
        );
        Ok(())
    }

    // Check if both key files exist.
    #[tracing::instrument(skip_all)]
    pub async fn exists(directory: &Path) -> Result<bool, SshKeypairError> {
        let public_key_path = directory.join(PUBLIC_KEY_FILE);
        let private_key_path = directory.join(PRIVATE_KEY_FILE);

        let public_key_exists = fs::try_exists(&public_key_path).await?;
        let private_key_exists = fs::try_exists(&private_key_path).await?;
        Ok(public_key_exists && private_key_exists)
    }

    // Load keypair from disk, expecting OpenSSH format.
    #[tracing::instrument(skip_all)]
    pub async fn load(directory: &Path) -> Result<Self, SshKeypairError> {
        let public_key_path = directory.join(PUBLIC_KEY_FILE);
        let private_key_path = directory.join(PRIVATE_KEY_FILE);

        let public_key_string = read_to_string(&public_key_path).await?;
        let private_key_string = read_to_string(&private_key_path).await?;

        let public_key = PublicKey::from_openssh(&public_key_string)?;
        let private_key = PrivateKey::from_openssh(&private_key_string)?;

        debug!(
            public_key = %public_key_path.display(),
            private_key = %private_key_path.display(),
            "Loaded SSH keypair"
        );

        Ok(Self {
            public_key,
            private_key,
        })
    }
}

async fn write_all(path: &Path, data: &[u8]) -> Result<(), std::io::Error> {
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .await?;
    f.write_all(data).await?;
    Ok(())
}

async fn read_to_string(path: &Path) -> Result<String, std::io::Error> {
    let mut file = fs::File::open(path).await?;
    let mut string = String::new();
    file.read_to_string(&mut string).await?;
    Ok(string)
}

#[cfg(unix)]
async fn set_private_mode_0600(path: &Path) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt;
    let mut perm = fs::metadata(path).await?.permissions();
    perm.set_mode(0o600);
    fs::set_permissions(path, perm).await
}

#[cfg(not(unix))]
async fn set_private_mode_0600(_path: &Path) -> Result<(), std::io::Error> {
    unimplemented!()
}
