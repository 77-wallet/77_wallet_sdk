//! Device transforms and bindings to existing keystore I/O; no key provisioning.
//! Caller owns durable migration policy and atomic publication at the existing path.
use super::{DeviceProtection, ProtectionContext, ProtectionError};
use wallet_crypto::EncryptedJsonDecryptor;
use zeroize::Zeroizing;

const MAGIC: &[u8] = b"WDK";
const HEADER: &[u8] = b"WDK\x01";
const MAX_INNER: usize = 1024 * 1024;
const MAX_NATIVE: usize = MAX_INNER + 128;

/// Must come from trusted wallet state, never inferred from a failed decrypt.
#[derive(Clone, Copy)]
pub enum ReadPolicy {
    LegacyMigrationAllowed,
    DeviceRequired,
}

#[derive(Debug, PartialEq, Eq)]
pub enum KeystoreProtectionError {
    InvalidFormat,
    LegacyForbidden,
    PasswordDecryption,
    UnsupportedKdfCost,
    Protection(ProtectionError),
    Worker,
    Storage,
}
impl std::fmt::Display for KeystoreProtectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "keystore protection: {self:?}")
    }
}
impl std::error::Error for KeystoreProtectionError {}
impl From<ProtectionError> for KeystoreProtectionError {
    fn from(error: ProtectionError) -> Self {
        Self::Protection(error)
    }
}
impl From<wallet_tree::Error> for KeystoreProtectionError {
    fn from(error: wallet_tree::Error) -> Self {
        match error {
            wallet_tree::Error::KeystoreWorker => Self::Worker,
            wallet_tree::Error::InvalidKeyFormat => Self::InvalidFormat,
            _ => Self::Storage,
        }
    }
}

/// No Debug implementation: plaintext must not appear in logs.
/// A replacement must be atomically committed and migration state advanced by the
/// existing I/O flow before treating migration as complete. None means no rewrite.
pub struct OpenedKeystore {
    pub plaintext: Zeroizing<Vec<u8>>,
    pub replacement: Option<Vec<u8>>,
}

#[derive(Clone)]
pub struct KeystoreProtection {
    protection: DeviceProtection,
}
impl KeystoreProtection {
    pub fn new(protection: DeviceProtection) -> Self {
        Self { protection }
    }

    /// Common writer for any password-encrypted keystore content. Context is the
    /// expected stable record identity supplied by the caller, not derived from disk.
    /// Caller owns safe provisioning, authorization and concurrent-write ordering.
    pub async fn store_file(
        &self,
        path: &std::path::Path,
        context: &ProtectionContext,
        plaintext: Zeroizing<Vec<u8>>,
        password: &str,
        algorithm: wallet_crypto::KdfAlgorithm,
    ) -> Result<(), KeystoreProtectionError> {
        wallet_tree::file_ops::protected::store(
            path,
            plaintext,
            password,
            algorithm,
            |inner| async move { self.seal(context, &inner).await },
        )
        .await
    }

    /// Common read of an already device-protected file. No content schema is assumed.
    /// Legacy migration is a separate explicit operation; this never silently upgrades
    /// or drops an uncommitted replacement. Native/password errors remain distinct.
    pub async fn load_file(
        &self,
        path: &std::path::Path,
        context: &ProtectionContext,
        password: &str,
    ) -> Result<Zeroizing<Vec<u8>>, KeystoreProtectionError> {
        wallet_tree::file_ops::protected::load(path, |stored| async move {
            let opened = self.open(context, &stored, password, ReadPolicy::DeviceRequired).await?;
            Ok(opened.plaintext)
        })
        .await
    }

    /// Modern root write using the injected provider and existing root.keystore path.
    /// `wallet_id` must be the stable wallet identity, never a display name or path.
    /// Caller must provision safely and serialize authorized writes before entering.
    /// Do not activate until all root consumers understand the protected format.
    pub async fn store_root(
        &self,
        root_dir: &std::path::Path,
        wallet_id: &str,
        root: wallet_tree::file_ops::RootData,
        password: &str,
        algorithm: wallet_crypto::KdfAlgorithm,
    ) -> Result<(), KeystoreProtectionError> {
        let context = ProtectionContext::new(wallet_id, "root-keystore")?;
        let plaintext = Zeroizing::new(
            serde_json::to_vec(&root).map_err(|_| KeystoreProtectionError::InvalidFormat)?,
        );
        drop(root);
        self.store_file(&root_dir.join("root.keystore"), &context, plaintext, password, algorithm)
            .await
    }

    /// Read a modern root that is already device-protected. This deliberately rejects
    /// legacy files: migration must persist its replacement and durable policy first.
    /// Native errors are preserved rather than being reported as password errors.
    pub async fn load_root(
        &self,
        root_dir: &std::path::Path,
        wallet_id: &str,
        password: &str,
    ) -> Result<wallet_tree::file_ops::RootData, KeystoreProtectionError> {
        let context = ProtectionContext::new(wallet_id, "root-keystore")?;
        let plaintext = self.load_file(&root_dir.join("root.keystore"), &context, password).await?;
        serde_json::from_slice(&plaintext).map_err(|_| KeystoreProtectionError::InvalidFormat)
    }

    /// New writes use this directly. Only accepts existing password-encrypted JSON,
    /// preventing accidental double wrapping. Keeps that inner format unchanged.
    pub async fn seal(
        &self,
        context: &ProtectionContext,
        inner: &[u8],
    ) -> Result<Vec<u8>, KeystoreProtectionError> {
        validate_inner(inner)?;
        let native = self.protection.protect(context, inner).await?;
        if native.is_empty() || native.len() > MAX_NATIVE {
            return Err(KeystoreProtectionError::InvalidFormat);
        }
        // Verify native roundtrip before handing bytes to the existing writer.
        let verified = self.protection.unprotect(context, &native).await?;
        if verified != inner {
            return Err(ProtectionError::AuthenticationFailed.into());
        }
        let mut stored = Vec::with_capacity(HEADER.len() + native.len());
        stored.extend_from_slice(HEADER);
        stored.extend_from_slice(&native);
        Ok(stored)
    }

    /// Legacy upgrade is prepared only after the existing password decrypt succeeds.
    /// Native/version failures never retry the legacy parser.
    pub async fn open(
        &self,
        context: &ProtectionContext,
        stored: &[u8],
        password: &str,
        policy: ReadPolicy,
    ) -> Result<OpenedKeystore, KeystoreProtectionError> {
        if stored.len() > HEADER.len() + MAX_NATIVE {
            return Err(KeystoreProtectionError::InvalidFormat);
        }
        let (inner, legacy) = if stored.starts_with(MAGIC) {
            if !stored.starts_with(HEADER) || stored.len() == HEADER.len() {
                return Err(KeystoreProtectionError::InvalidFormat);
            }
            (self.protection.unprotect(context, &stored[HEADER.len()..]).await?, false)
        } else {
            if matches!(policy, ReadPolicy::DeviceRequired) {
                return Err(KeystoreProtectionError::LegacyForbidden);
            }
            (stored.to_vec(), true)
        };
        validate_inner(&inner)?;
        let password = Zeroizing::new(password.as_bytes().to_vec());
        let encrypted = inner.clone();
        let plaintext = tokio::task::spawn_blocking(move || {
            let json = std::str::from_utf8(&encrypted)
                .map_err(|_| KeystoreProtectionError::InvalidFormat)?;
            wallet_crypto::KeystoreJsonDecryptor
                .decrypt(&password, json)
                .map(Zeroizing::new)
                .map_err(|_| KeystoreProtectionError::PasswordDecryption)
        })
        .await
        .map_err(|_| KeystoreProtectionError::Worker)??;
        let replacement = if legacy { Some(self.seal(context, &inner).await?) } else { None };
        Ok(OpenedKeystore { plaintext, replacement })
    }
}

fn validate_inner(inner: &[u8]) -> Result<(), KeystoreProtectionError> {
    if inner.len() > MAX_INNER {
        return Err(KeystoreProtectionError::InvalidFormat);
    }
    let parsed = serde_json::from_slice::<wallet_crypto::KeystoreJson>(inner)
        .map_err(|_| KeystoreProtectionError::InvalidFormat)?;
    // Bound attacker-controlled work before invoking the existing password KDF.
    // Includes currently generated Argon2 (128 MiB/t5/p8), Scrypt and PBKDF2 profiles.
    // Out-of-budget imported files are rejected without modification, never downgraded.
    use wallet_crypto::kdf::KdfParams;
    let within_budget = match &parsed.crypto.kdfparams {
        KdfParams::Argon2id(p) => {
            p.dklen == 32
                && p.memory_cost <= 262144
                && p.time_cost <= 10
                && p.parallelism <= 16
                && u64::from(p.memory_cost) * u64::from(p.time_cost) <= 1048576
        }
        KdfParams::Scrypt(p) => {
            p.dklen == 32
                && p.n.is_power_of_two()
                && 128u128 * u128::from(p.n) * u128::from(p.r) <= 268435456
                && u128::from(p.n) * u128::from(p.r) * u128::from(p.p) <= 16777216
        }
        KdfParams::Pbkdf2(p) => p.dklen == 32 && p.c <= 2000000,
    };
    if !within_budget {
        return Err(KeystoreProtectionError::UnsupportedKdfCost);
    }
    Ok(())
}

#[cfg(test)]
#[path = "keystore_format_tests.rs"]
mod tests;
