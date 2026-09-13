//! Shared byte-level keystore I/O. All record types use the same protection boundary.
use std::{
    future::Future,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

use secrecy::zeroize::Zeroizing;
use wallet_crypto::{EncryptedJsonGenerator, KdfAlgorithm, KeystoreJsonGenerator};

use crate::Error;

const MAX_INNER: usize = 1024 * 1024;
const MAX_STORED: usize = MAX_INNER + 128 + 4;

/// Password-encrypt opaque keystore contents, then await outer protection and
/// atomically publish at the existing path. Does not inspect root/derived/verify data.
/// The callback must bind the ciphertext to the expected record and verify protection.
/// Caller owns provisioning, authorization and serialization of competing writes.
pub async fn store<F, Fut, E>(
    path: &Path,
    plain: Zeroizing<Vec<u8>>,
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
    let path = path.to_owned();
    let parent = path.parent().ok_or_else(|| E::from(Error::InvalidKeyFormat))?;
    let parent = if parent.as_os_str().is_empty() { Path::new(".") } else { parent }.to_owned();
    tokio::task::spawn_blocking(move || -> Result<(), Error> {
        // Only the outer ciphertext reaches disk, including this temporary file.
        let mut pending = tempfile::NamedTempFile::new_in(&parent)?;
        pending.write_all(&stored)?;
        pending.as_file().sync_all()?;
        pending.seek(SeekFrom::Start(0))?;
        let mut verified = Vec::new();
        pending.read_to_end(&mut verified)?;
        if verified != stored {
            return Err(Error::InvalidKeyFormat);
        }
        pending.persist(&path).map_err(|error| Error::KeystoreIo(error.error))?;
        // A failure here is an uncertain durability outcome, not a rollback.
        #[cfg(unix)]
        std::fs::File::open(&parent)?.sync_all()?;
        Ok(())
    })
    .await
    .map_err(|_| E::from(Error::KeystoreWorker))?
    .map_err(E::from)
}

/// Bounded binary read followed by the caller's asynchronous decoder. The decoder
/// owns device/version/password policy and the result type; no automatic migration.
pub async fn load<T, F, Fut, E>(path: &Path, decode: F) -> Result<T, E>
where
    F: FnOnce(Vec<u8>) -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: From<Error>,
{
    let path = path.to_owned();
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

#[cfg(test)]
#[path = "protected_tests.rs"]
mod tests;
