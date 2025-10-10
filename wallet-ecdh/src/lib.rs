use crate::{
    data::EncryptedData,
    encryption::{decrypt_with_shared_secret, encrypt_with_shared_secret},
    error::EncryptionError,
};
use k256::ecdh::SharedSecret;
use once_cell::sync::Lazy;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

pub mod data;
pub mod encryption;
pub mod error;
pub mod sign;

pub static GLOBAL_KEY: Lazy<Arc<ExKey>> = Lazy::new(|| {
    let cache = Arc::new(ExKey::new());
    cache
});

pub struct ExKey {
    sn: RwLock<String>,
    shared_secret: Option<SharedSecret>,
}

impl ExKey {
    pub fn new() -> Self {
        ExKey { sn: RwLock::new("".to_string()), shared_secret: None }
    }

    pub fn set_fn(&self, sn : &str) {
        let mut w = self.sn.write().unwrap();
        *w = sn.to_string();
    }

    pub fn sn(&self) -> String {
        self.sn.read().unwrap().to_string()
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> Result<EncryptedData, EncryptionError> {
        let key = Uuid::new_v4().to_string();
        if let Some(shared_secret) = &self.shared_secret {
            encrypt_with_shared_secret(plaintext, shared_secret, key.as_bytes())
        } else {
            Err(EncryptionError::InvalidKey)
        }
    }

    pub fn decrypt(&self, plaintext: &[u8], key: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        if let Some(shared_secret) = &self.shared_secret {
            decrypt_with_shared_secret(plaintext, shared_secret, key)
        } else {
            Err(EncryptionError::InvalidKey)
        }
    }
}
