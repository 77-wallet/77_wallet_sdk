//! Platform-neutral contract for an additional device-bound encryption layer.
//!
//! The plugin supplies an iOS/Android provider before protected storage is used.
//! This module does not select a platform, store keys, or perform file/SQLite I/O.
//! Merely configuring it does not protect existing storage: callers must route
//! every sensitive read/write through it in a subsequent integration step.
use std::{fmt, future::Future, pin::Pin, sync::Arc};

pub type ProtectionFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<u8>, ProtectionError>> + Send + 'a>>;

/// Stable categories suitable for mapping across the native bridge.
/// Never put ciphertext, passwords, key material or raw platform errors here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtectionError {
    NotConfigured,
    InvalidContext,
    Unavailable,
    Locked,
    Cancelled,
    KeyMissing,
    KeyInvalidated,
    AuthenticationFailed,
    MalformedCiphertext,
    Timeout,
    PlatformFailure,
}
impl fmt::Display for ProtectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "device protection: {self:?}")
    }
}
impl std::error::Error for ProtectionError {}

/// Non-secret, stable record identity and purpose. Do not use a renameable label
/// or filesystem location. For device-level records use a stable device scope.
/// Both encrypt and decrypt must derive context from the expected record, not
/// trust context read from an unauthenticated ciphertext header.
#[derive(Clone, PartialEq, Eq)]
pub struct ProtectionContext {
    scope: String,
    purpose: String,
}
impl ProtectionContext {
    pub fn new(
        scope: impl Into<String>,
        purpose: impl Into<String>,
    ) -> Result<Self, ProtectionError> {
        let scope = scope.into();
        let purpose = purpose.into();
        if scope.is_empty()
            || purpose.is_empty()
            || scope.len() > u32::MAX as usize
            || purpose.len() > u32::MAX as usize
        {
            return Err(ProtectionError::InvalidContext);
        }
        Ok(Self { scope, purpose })
    }
    /// Versioned length-prefixed UTF-8; native adapters must use these exact
    /// bytes as AEAD authenticated data. This is binding, not a secret/key.
    pub fn authenticated_data(&self) -> Vec<u8> {
        let mut aad = b"wallet-device-protection-context-v1\0".to_vec();
        for field in [&self.scope, &self.purpose] {
            aad.extend_from_slice(&(field.len() as u32).to_be_bytes());
            aad.extend_from_slice(field.as_bytes());
        }
        aad
    }
}

/// Implemented by the native plugin, not Flutter UI business logic.
///
/// Providers must enforce hardware-backed key use, bind the context as AAD,
/// use fresh encryption randomness and return a versioned opaque envelope.
/// They must fail on missing/invalidated keys during decryption, never create
/// replacement keys to recover existing ciphertext, and never silently fall
/// back to software or plaintext. Explicit provisioning/deletion lifecycle is
/// owned by the adapter; this interface grants no reset/delete capability.
///
/// Futures must not block an executor thread while awaiting native callbacks.
/// The adapter owns deadlines/cancellation and thread-safe native request state.
/// Error categories describe outcomes; this wrapper does not enforce a timeout.
pub trait DeviceProtectionProvider: Send + Sync {
    /// Input is already password-encrypted data, not raw wallet secrets.
    fn protect<'a>(
        &'a self,
        context: &'a ProtectionContext,
        data: &'a [u8],
    ) -> ProtectionFuture<'a>;
    /// Output is the original password-encrypted data; SDK decrypts its inner layer.
    fn unprotect<'a>(
        &'a self,
        context: &'a ProtectionContext,
        blob: &'a [u8],
    ) -> ProtectionFuture<'a>;
}

/// Instance-scoped dependency; no global mutable registry or default adapter.
/// Missing configuration is an error, never permission to persist input as-is.
#[derive(Clone, Default)]
pub struct DeviceProtection {
    provider: Option<Arc<dyn DeviceProtectionProvider>>,
}
impl DeviceProtection {
    /// Reinitialization may reuse a clone, but must not replace/drop a provider.
    pub(crate) fn same_provider(&self, other: &Self) -> bool {
        match (&self.provider, &other.provider) {
            (None, None) => true,
            (Some(left), Some(right)) => Arc::ptr_eq(left, right),
            _ => false,
        }
    }

    pub fn new(provider: Arc<dyn DeviceProtectionProvider>) -> Self {
        Self { provider: Some(provider) }
    }
    pub async fn protect(
        &self,
        context: &ProtectionContext,
        data: &[u8],
    ) -> Result<Vec<u8>, ProtectionError> {
        self.provider.as_ref().ok_or(ProtectionError::NotConfigured)?.protect(context, data).await
    }
    pub async fn unprotect(
        &self,
        context: &ProtectionContext,
        blob: &[u8],
    ) -> Result<Vec<u8>, ProtectionError> {
        self.provider.as_ref().ok_or(ProtectionError::NotConfigured)?.unprotect(context, blob).await
    }
}

#[cfg(test)]
#[path = "device_protection/contract_tests.rs"]
mod contract_tests;

pub mod keystore_format;
