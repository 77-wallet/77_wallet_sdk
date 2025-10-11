use std::str::FromStr;
use crate::{
    data::EncryptedData,
    encryption::{decrypt_with_shared_secret, encrypt_with_shared_secret},
    error::EncryptionError,
};
use k256::ecdh::SharedSecret;
use once_cell::sync::Lazy;
use std::sync::{Arc, RwLock};
use k256::ecdsa::Signature;
use k256::elliptic_curve::generic_array::GenericArray;
use k256::{ecdh, PublicKey, Secp256k1, SecretKey};
use uuid::Uuid;
use crate::sign::{sign_with_derived_ecdsa, verify_derived_ecdsa_signature};

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
    secret: SecretKey,
    shared_secret: RwLock<Option<SharedSecret>>,
}

impl ExKey {
    pub fn new() -> Self {
        let alice_secret_key_hex = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let alice_secret_key_bytes = hex::decode(alice_secret_key_hex).expect("Invalid hex string");
        let alice_secret_key_array = GenericArray::clone_from_slice(&alice_secret_key_bytes);
        let alice_secret = SecretKey::from_bytes(&alice_secret_key_array).unwrap();
        ExKey { sn: RwLock::new("".to_string()), secret: alice_secret, shared_secret: RwLock::new(None) }
    }

    pub fn set_sn(&self, sn : &str) {
        let mut w = self.sn.write().unwrap();
        *w = sn.to_string();
    }

    pub fn sn(&self) -> String {
        self.sn.read().unwrap().to_string()
    }

    pub fn secret_pub_key(&self) -> String {
        let pub_key = self.secret.public_key();
        pub_key.to_string()
    }

    pub fn set_shared_secret(&self, s: &str) ->Result<(), crate::error::EncryptionError> {
        // let pem_string = wallet_utils::base64_to_bytes(s)?;
        let bob_public = PublicKey::from_str(s).map_err(|_| EncryptionError::InvalidKey)?;
        let shared_key = ecdh::diffie_hellman(self.secret.to_nonzero_scalar(), bob_public.as_affine());
        tracing::info!("Got shared secret key: {:?}",hex::encode(shared_key.raw_secret_bytes()));
        let mut w = self.shared_secret.write().map_err(|_| EncryptionError::LockPoisoned)?;
        *w = Some(shared_key);
        Ok(())
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> Result<EncryptedData, EncryptionError> {
        let key = Uuid::new_v4().to_string();
        let r = self.shared_secret.read().map_err(|_| EncryptionError::LockPoisoned)?;;
        if let Some(shared_secret) = &*r {
            encrypt_with_shared_secret(plaintext, &shared_secret, key.as_bytes())
        } else {
            Err(EncryptionError::InvalidKey)
        }
    }

    pub fn decrypt(&self, plaintext: &[u8], key: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        let r = self.shared_secret.read().map_err(|_| EncryptionError::LockPoisoned)?;;
        if let Some(shared_secret) = &*r {
            decrypt_with_shared_secret(plaintext, shared_secret, key)
        } else {
            Err(EncryptionError::InvalidKey)
        }
    }

    pub fn sign(&self, plaintext: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        let key = &plaintext[0.. 32];
        tracing::info!("Got signing key: {:?}", hex::encode(key));
        let r = self.shared_secret.read().map_err(|_| EncryptionError::LockPoisoned)?;;
        if let Some(shared_secret) = &*r {
            let res  = sign_with_derived_ecdsa(plaintext, shared_secret, key)?;
            Ok(res.to_vec())
        } else {
            Err(EncryptionError::InvalidKey)
        }
    }

    pub fn verify(&self, plaintext: &[u8], sig: &[u8]) -> Result<bool, EncryptionError> {
        let key = &plaintext[0.. 32];
        let signature = Signature::from_slice(sig).map_err(|e| EncryptionError::SignatureError(e))?;
        let r = self.shared_secret.read().map_err(|_| EncryptionError::LockPoisoned)?;;
        if let Some(shared_secret) = &*r {
            let ok = verify_derived_ecdsa_signature(plaintext, &signature, shared_secret,key)?;
            Ok(ok)
        } else {
            Err(EncryptionError::InvalidKey)
        }
    }
}
