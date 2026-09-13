use std::{
    future::Future,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

use secrecy::zeroize::Zeroizing;
use wallet_crypto::{EncryptedJsonGenerator, KdfAlgorithm, KeystoreJsonGenerator};

use super::ModernIo;
use crate::{Error, file_ops::RootData};

const MAX_INNER: usize = 1024 * 1024;
const MAX_STORED: usize = MAX_INNER + 128 + 4;

impl ModernIo {
    /// Writes the existing root.keystore path after asynchronous outer protection.
    /// The callback must authenticate/bind the password ciphertext to this wallet.
    /// Callers must serialize competing writes and provide a provisioned provider.
    /// No fallback or provisioning takes place here.
    pub async fn store_root_with_protection<F, Fut, E>(
        &self,
        root_dir: &Path,
        root: RootData,
        password: &str,
        algorithm: KdfAlgorithm,
        protect: F,
    ) -> Result<(), E>
    where
        F: FnOnce(Vec<u8>) -> Fut,
        Fut: Future<Output = Result<Vec<u8>, E>>,
        E: From<Error>,
    {
        let password = Zeroizing::new(password.as_bytes().to_vec());
        let inner = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, Error> {
            let plain = Zeroizing::new(wallet_utils::serde_func::serde_to_vec(&root)?);
            if plain.len() > MAX_INNER {
                return Err(Error::InvalidKeyFormat);
            }
            // Same generator as KeystoreBuilder::save; only the disk write is delayed.
            let encrypted = KeystoreJsonGenerator::new(rand::thread_rng(), algorithm)
                .generate(&password, &plain)?;
            let inner = wallet_utils::serde_func::serde_to_vec(&encrypted)?;
            if inner.len() > MAX_INNER {
                return Err(Error::InvalidKeyFormat);
            }
            Ok(inner)
        })
        .await
        .map_err(|_| E::from(Error::KeystoreWorker))?
        .map_err(E::from)?;

        let stored = protect(inner).await?;
        if stored.is_empty() || stored.len() > MAX_STORED {
            return Err(Error::InvalidKeyFormat.into());
        }
        let root_dir = root_dir.to_owned();
        tokio::task::spawn_blocking(move || -> Result<(), Error> {
            // Only the outer ciphertext reaches disk, including this temporary file.
            let mut pending = tempfile::NamedTempFile::new_in(&root_dir)?;
            pending.write_all(&stored)?;
            pending.as_file().sync_all()?;
            pending.seek(SeekFrom::Start(0))?;
            let mut verified = Vec::new();
            pending.read_to_end(&mut verified)?;
            if verified != stored {
                return Err(Error::InvalidKeyFormat);
            }
            pending
                .persist(root_dir.join("root.keystore"))
                .map_err(|error| Error::KeystoreIo(error.error))?;
            // A failure here is an uncertain durability outcome, not a rollback.
            #[cfg(unix)]
            std::fs::File::open(&root_dir)?.sync_all()?;
            Ok(())
        })
        .await
        .map_err(|_| E::from(Error::KeystoreWorker))?
        .map_err(E::from)
    }

    /// Reads binary bytes at the existing path and awaits the caller's decoder.
    /// The decoder must enforce device/version/password policy; errors propagate.
    /// This is read-only: legacy migration must be explicitly orchestrated by caller.
    pub async fn load_root_with_protection<F, Fut, E>(
        &self,
        root_dir: &Path,
        decode: F,
    ) -> Result<RootData, E>
    where
        F: FnOnce(Vec<u8>) -> Fut,
        Fut: Future<Output = Result<RootData, E>>,
        E: From<Error>,
    {
        let path = root_dir.join("root.keystore");
        let stored = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, Error> {
            let file = std::fs::File::open(path)?;
            let mut stored = Vec::new();
            file.take(MAX_STORED as u64 + 1).read_to_end(&mut stored)?;
            if stored.is_empty() || stored.len() > MAX_STORED {
                return Err(Error::InvalidKeyFormat);
            }
            Ok(stored)
        })
        .await
        .map_err(|_| E::from(Error::KeystoreWorker))?
        .map_err(E::from)?;
        decode(stored).await
    }
}

#[cfg(test)]
mod tests;
