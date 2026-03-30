use aes_gcm::{
    Aes256Gcm, Key, KeyInit, Nonce,
    aead::{Aead, AeadCore},
};
use argon2::{Algorithm, Argon2, Params, Version};
use hkdf::Hkdf;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use zeroize::{Zeroize, Zeroizing};

use crate::error::service::ServiceError;

pub(crate) use aes_gcm::aead::OsRng;

/// 版本化的 seed 封装格式。
///
/// 这层只负责“把 seed 从可直接解读的单层密文，升级成可轮换的封装结构”。
/// 真正的明文密码不应该进入这层的持久化格式里。
pub(crate) const SEED_ENVELOPE_VERSION_V1: u8 = 1;
pub(crate) const SEED_ENVELOPE_SALT_BYTES: usize = 16;
pub(crate) const SEED_ENVELOPE_KEY_BYTES: usize = 32;
pub(crate) const SEED_ENVELOPE_NONCE_BYTES: usize = 12;
pub(crate) const SEED_ENVELOPE_ROTATION_COUNTER: u64 = 0;
const SEED_ENVELOPE_HKDF_INFO: &[u8] = b"wallet-api-seed-envelope-v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SeedEnvelopeV1 {
    pub(crate) version: u8,
    pub(crate) salt: Vec<u8>,
    pub(crate) rotation_counter: u64,
    pub(crate) session_nonce: Vec<u8>,
    pub(crate) seed_nonce: Vec<u8>,
    pub(crate) wrapped_dek: Vec<u8>,
    pub(crate) seed_cipher: Vec<u8>,
}

/// 解锁材料只保存“可以重新解开 seed 的中间材料”，而不是明文密码。
/// 这里的 material 只是当前会话里可复用的中间态，不是长期缓存的最终秘密。
#[derive(Debug, Clone)]
pub(crate) struct WalletUnlockMaterial {
    smk: Vec<u8>,
}

impl WalletUnlockMaterial {
    pub(crate) fn new(smk: Vec<u8>) -> Self {
        Self { smk }
    }

    pub(crate) fn smk(&self) -> &[u8] {
        &self.smk
    }
}

impl Drop for WalletUnlockMaterial {
    fn drop(&mut self) {
        self.smk.zeroize();
    }
}

/// 钱包解锁会话是一个短期的“能力句柄”：
/// - session_token 用来表示当前会话已解锁
/// - wallet_materials 为每个钱包保存对应的解锁材料
/// - 过期后整个状态会被丢弃
#[derive(Debug, Clone)]
pub(crate) struct WalletUnlockSession {
    session_token: String,
    expires_at: Instant,
    wallet_materials: HashMap<String, WalletUnlockMaterial>,
}

impl WalletUnlockSession {
    pub(crate) fn new(
        session_token: String,
        expires_at: Instant,
        wallet_materials: HashMap<String, WalletUnlockMaterial>,
    ) -> Self {
        Self { session_token, expires_at, wallet_materials }
    }

    pub(crate) fn session_token(&self) -> &str {
        &self.session_token
    }

    pub(crate) fn is_expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }

    pub(crate) fn wallet_material(&self, wallet_address: &str) -> Option<&WalletUnlockMaterial> {
        self.wallet_materials.get(wallet_address)
    }
}

pub(crate) struct WalletUnlockSessionCodec;

impl WalletUnlockSessionCodec {
    pub(crate) fn unlock_session_ttl() -> Duration {
        #[cfg(test)]
        {
            Duration::from_secs(1)
        }

        #[cfg(not(test))]
        {
            Duration::from_secs(3 * 60)
        }
    }

    pub(crate) fn generate_unlock_token() -> String {
        let mut token_bytes = vec![0u8; 32];
        let mut rng = rand::rngs::OsRng;
        rng.fill_bytes(&mut token_bytes);
        hex::encode(token_bytes)
    }

    pub(crate) async fn derive_smk(
        password: &str,
        salt: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, ServiceError> {
        let mut smk = vec![0u8; SEED_ENVELOPE_KEY_BYTES];
        let params = Params::default();
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        argon2.hash_password_into(password.as_bytes(), salt, &mut smk).map_err(|err| {
            crate::error::service::ServiceError::System(
                crate::error::system::SystemError::Internal(err.to_string()),
            )
        })?;

        Ok(Zeroizing::new(smk))
    }

    async fn derive_session_key(
        smk: &[u8],
        rotation_counter: u64,
    ) -> Result<Zeroizing<Vec<u8>>, ServiceError> {
        let hkdf = Hkdf::<sha2::Sha256>::new(None, smk);
        let mut session_key = vec![0u8; SEED_ENVELOPE_KEY_BYTES];
        let info = [SEED_ENVELOPE_HKDF_INFO, &rotation_counter.to_le_bytes()].concat();
        hkdf.expand(&info, &mut session_key).map_err(|err| {
            crate::error::service::ServiceError::System(
                crate::error::system::SystemError::Internal(err.to_string()),
            )
        })?;

        Ok(Zeroizing::new(session_key))
    }

    async fn unwrap_dek(
        session_key: &[u8],
        wrapped_dek: &[u8],
        session_nonce: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, ServiceError> {
        if session_nonce.len() != SEED_ENVELOPE_NONCE_BYTES {
            return Err(crate::error::service::ServiceError::System(
                crate::error::system::SystemError::Internal(format!(
                    "invalid session nonce length: {}",
                    session_nonce.len()
                )),
            ));
        }

        let key = Key::<Aes256Gcm>::from_slice(session_key);
        let cipher = Aes256Gcm::new(key);
        let nonce = Nonce::from_slice(session_nonce);
        let dek = cipher.decrypt(nonce, wrapped_dek).map_err(|err| {
            crate::error::service::ServiceError::System(
                crate::error::system::SystemError::Internal(err.to_string()),
            )
        })?;

        Ok(Zeroizing::new(dek))
    }
}

pub(crate) struct SeedEnvelopeCodec;

impl SeedEnvelopeCodec {
    pub(crate) async fn encrypt_seed_bundle(
        password: &str,
        seed: &[u8],
    ) -> Result<String, ServiceError> {
        let mut salt = vec![0u8; SEED_ENVELOPE_SALT_BYTES];
        OsRng.fill_bytes(&mut salt);

        let smk = WalletUnlockSessionCodec::derive_smk(password, &salt).await?;
        Self::encrypt_seed_bundle_with_smk(&smk, &salt, seed, SEED_ENVELOPE_ROTATION_COUNTER).await
    }

    async fn encrypt_seed_bundle_with_smk(
        smk: &[u8],
        salt: &[u8],
        seed: &[u8],
        rotation_counter: u64,
    ) -> Result<String, ServiceError> {
        let session_key =
            WalletUnlockSessionCodec::derive_session_key(smk, rotation_counter).await?;

        let mut dek = vec![0u8; SEED_ENVELOPE_KEY_BYTES];
        let mut rng = rand::rngs::OsRng;
        rng.fill_bytes(&mut dek);

        let session_cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&session_key));
        let session_nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let wrapped_dek =
            session_cipher.encrypt(&session_nonce, dek.as_slice()).map_err(|err| {
                crate::error::service::ServiceError::System(
                    crate::error::system::SystemError::Internal(err.to_string()),
                )
            })?;

        let seed_cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&dek));
        let seed_nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let encrypted_seed = seed_cipher.encrypt(&seed_nonce, seed).map_err(|err| {
            crate::error::service::ServiceError::System(
                crate::error::system::SystemError::Internal(err.to_string()),
            )
        })?;

        let envelope = SeedEnvelopeV1 {
            version: SEED_ENVELOPE_VERSION_V1,
            salt: salt.to_vec(),
            rotation_counter,
            session_nonce: session_nonce.to_vec(),
            seed_nonce: seed_nonce.to_vec(),
            wrapped_dek,
            seed_cipher: encrypted_seed,
        };

        serde_json::to_string(&envelope).map_err(|err| {
            crate::error::service::ServiceError::System(
                crate::error::system::SystemError::Internal(err.to_string()),
            )
        })
    }

    pub(crate) async fn encrypt_seed_bundle_with_state(
        unlock_material: &WalletUnlockMaterial,
        salt: &[u8],
        seed: &[u8],
        rotation_counter: u64,
    ) -> Result<String, ServiceError> {
        Self::encrypt_seed_bundle_with_smk(unlock_material.smk(), salt, seed, rotation_counter)
            .await
    }

    pub(crate) async fn decrypt_seed_bundle(
        password: &str,
        envelope: &SeedEnvelopeV1,
    ) -> Result<Vec<u8>, ServiceError> {
        let smk = WalletUnlockSessionCodec::derive_smk(password, &envelope.salt).await?;
        Self::decrypt_seed_bundle_with_smk(&smk, envelope).await
    }

    async fn decrypt_seed_bundle_with_smk(
        smk: &[u8],
        envelope: &SeedEnvelopeV1,
    ) -> Result<Vec<u8>, ServiceError> {
        let session_key =
            WalletUnlockSessionCodec::derive_session_key(smk, envelope.rotation_counter).await?;
        let dek = WalletUnlockSessionCodec::unwrap_dek(
            &session_key,
            &envelope.wrapped_dek,
            &envelope.session_nonce,
        )
        .await?;

        if envelope.seed_nonce.len() != SEED_ENVELOPE_NONCE_BYTES {
            return Err(crate::error::service::ServiceError::System(
                crate::error::system::SystemError::Internal(format!(
                    "invalid seed nonce length: {}",
                    envelope.seed_nonce.len()
                )),
            ));
        }

        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&dek));
        let nonce = Nonce::from_slice(&envelope.seed_nonce);
        let seed = cipher.decrypt(nonce, envelope.seed_cipher.as_ref()).map_err(|err| {
            crate::error::service::ServiceError::System(
                crate::error::system::SystemError::Internal(err.to_string()),
            )
        })?;

        Ok(seed)
    }

    pub(crate) async fn decrypt_seed_bundle_with_state(
        unlock_material: &WalletUnlockMaterial,
        envelope: &SeedEnvelopeV1,
    ) -> Result<Vec<u8>, ServiceError> {
        Self::decrypt_seed_bundle_with_smk(unlock_material.smk(), envelope).await
    }

    pub(crate) fn parse_seed_envelope(seed: &str) -> Result<Option<SeedEnvelopeV1>, ServiceError> {
        match serde_json::from_str::<SeedEnvelopeV1>(seed) {
            Ok(envelope) if envelope.version == SEED_ENVELOPE_VERSION_V1 => Ok(Some(envelope)),
            Ok(envelope) => Err(crate::error::service::ServiceError::System(
                crate::error::system::SystemError::Internal(format!(
                    "unsupported seed envelope version: {}",
                    envelope.version
                )),
            )),
            Err(_) => Ok(None),
        }
    }
}
