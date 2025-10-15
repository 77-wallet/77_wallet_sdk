use crate::{data::EncryptedData, error::EncryptionError};
use aes_gcm::{
    AeadCore, AeadInPlace, Aes256Gcm, Key, KeyInit, Nonce,
    aead::{Aead, OsRng, generic_array::GenericArray},
};
use hkdf::Hkdf;
use k256::{ecdh::SharedSecret, sha2::Sha256};

/// 从共享密钥派生 AES 加密密钥
pub(crate) fn derive_aes_key_from_shared_secret(
    shared_secret: &SharedSecret,
    key: &[u8],
) -> Result<[u8; 32], EncryptionError> {
    let shared_bytes = shared_secret.raw_secret_bytes();
    let hkdf = Hkdf::<Sha256>::new(None, shared_bytes);

    let mut aes_key = [0u8; 32];
    hkdf.expand(key, &mut aes_key)
        .map_err(|e| EncryptionError::KeyDerivationFailed(e.to_string()))?;

    tracing::info!("Got aes secret key: {:?}, {:?}", hex::encode(aes_key), hex::encode(key));
    Ok(aes_key)
}

/// 使用共享密钥加密数据
pub(crate) fn encrypt_with_shared_secret(
    plaintext: &[u8],
    shared_secret: &SharedSecret,
    key: &[u8],
) -> Result<EncryptedData, EncryptionError> {
    // 1. 从共享密钥派生 AES 密钥
    let aes_key_bytes = derive_aes_key_from_shared_secret(shared_secret, key)?;
    let aes_key = Key::<Aes256Gcm>::from_slice(&aes_key_bytes);
    let cipher = Aes256Gcm::new(aes_key);
    tracing::info!("Encrypting with shared secret: {}", hex::encode(aes_key_bytes));

    // 2. 生成随机 nonce
    let head = &aes_key_bytes[0..4];
    let nonce_bytes = [aes_key_bytes.as_slice(), head].concat();
    let nonce_md5 = md5::compute(nonce_bytes).to_vec();
    let nonce_md5_head = &nonce_md5[0..12];
    let nonce = GenericArray::from_slice(nonce_md5_head);

    // 3. 加密数据
    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|e| EncryptionError::EncryptionFailed(e.to_string()))?;

    Ok(EncryptedData { key: key.to_vec(), nonce: nonce.to_vec(), ciphertext })
}

/// 使用共享密钥解密数据
pub(crate) fn decrypt_with_shared_secret(
    encrypted_data: &[u8],
    shared_secret: &SharedSecret,
    key: &[u8],
) -> Result<Vec<u8>, EncryptionError> {
    // 1. 从共享密钥派生 AES 密钥
    let aes_key_bytes = derive_aes_key_from_shared_secret(shared_secret, key)?;
    let key = Key::<Aes256Gcm>::from_slice(&aes_key_bytes);
    let cipher = Aes256Gcm::new(key);

    // 2. 从字节数组重建 nonce
    let head = &aes_key_bytes[0..4];
    let nonce_bytes = [aes_key_bytes.as_slice(), head].concat();
    let nonce_md5 = md5::compute(nonce_bytes).to_vec();
    let nonce_md5_head = &nonce_md5[0..12];
    let nonce = GenericArray::from_slice(nonce_md5_head);

    // 3. 解密数据
    let plaintext = cipher
        .decrypt(nonce, encrypted_data)
        .map_err(|e| EncryptionError::DecryptionFailed(e.to_string()))?;

    Ok(plaintext)
}

/// 带认证的加密（包含额外数据）
pub(crate) fn encrypt_with_aad(
    plaintext: &mut [u8],
    additional_data: &[u8],
    shared_secret: &SharedSecret,
    key: &[u8],
) -> Result<EncryptedData, EncryptionError> {
    // 1. 从共享密钥派生 AES 密钥
    let aes_key_bytes = derive_aes_key_from_shared_secret(shared_secret, key)?;
    let key = Key::<Aes256Gcm>::from_slice(&aes_key_bytes);
    let cipher = Aes256Gcm::new(key);

    // 2. 生成随机 nonce
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

    // 3. 加密数据（包含额外认证数据）
    let tag = cipher
        .encrypt_in_place_detached(&nonce, additional_data, plaintext)
        .map_err(|e| EncryptionError::EncryptionFailed(e.to_string()))?;

    // 4. 组合 nonce 和密文
    let mut combined_ciphertext = nonce.to_vec();
    combined_ciphertext.extend_from_slice(&tag);

    Ok(EncryptedData { key: key.to_vec(), nonce: nonce.to_vec(), ciphertext: combined_ciphertext })
}

/// 带认证的解密（包含额外数据）
pub(crate) fn decrypt_with_aad(
    encrypted_data: &mut EncryptedData,
    additional_data: &[u8],
    shared_secret: &SharedSecret,
    key: &[u8],
) -> Result<Vec<u8>, EncryptionError> {
    // 1. 从共享密钥派生 AES 密钥
    let aes_key_bytes = derive_aes_key_from_shared_secret(shared_secret, key)?;
    let key = Key::<Aes256Gcm>::from_slice(&aes_key_bytes);
    let cipher = Aes256Gcm::new(key);

    // 2. 从字节数组重建 nonce
    let nonce = Nonce::from_slice(&encrypted_data.nonce);

    // 3. 解密数据（包含额外认证数据）
    let plaintext = cipher
        .encrypt_in_place_detached(nonce, additional_data, &mut encrypted_data.ciphertext)
        .map_err(|e| EncryptionError::DecryptionFailed(e.to_string()))?;

    Ok(plaintext.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use k256::{PublicKey, SecretKey, ecdh, ecdh::EphemeralSecret};
    use rand_core::OsRng;

    #[test]
    fn test_basic_encryption_decryption() {
        // 生成 ECDH 共享密钥
        let alice_secret = EphemeralSecret::random(&mut OsRng);
        let bob_secret = EphemeralSecret::random(&mut OsRng);
        let shared_secret1 = alice_secret.diffie_hellman(&bob_secret.public_key());
        let shared_secret2 = bob_secret.diffie_hellman(&alice_secret.public_key());

        // 测试数据
        let plaintext = b"Hello, AES encryption!";
        let key = b"aes_encryption_key";

        // 加密
        let encrypted = encrypt_with_shared_secret(plaintext, &shared_secret1, key).unwrap();

        // 解密
        let decrypted =
            decrypt_with_shared_secret(&encrypted.ciphertext, &shared_secret2, key).unwrap();

        // 验证
        assert_eq!(plaintext, decrypted.as_slice());
        println!("基本加密解密测试通过！");
    }

    #[test]
    fn test_serialization_deserialization() {
        // 生成 ECDH 共享密钥
        let alice_secret = EphemeralSecret::random(&mut OsRng);
        let bob_secret = EphemeralSecret::random(&mut OsRng);
        let shared_secret1 = alice_secret.diffie_hellman(&bob_secret.public_key());
        let shared_secret2 = bob_secret.diffie_hellman(&alice_secret.public_key());

        // 测试数据
        let plaintext = b"Test serialization and deserialization";
        let key = b"aes_encryption_key";

        // 加密
        let encrypted = encrypt_with_shared_secret(plaintext, &shared_secret1, key).unwrap();

        // 序列化
        let serialized = encrypted.to_bytes();

        println!("len: {}", serialized.len());

        // 反序列化
        let deserialized = EncryptedData::from_bytes(&serialized).unwrap();

        // 解密
        let decrypted =
            decrypt_with_shared_secret(&deserialized.ciphertext, &shared_secret2, key).unwrap();

        // 验证
        assert_eq!(plaintext, decrypted.as_slice());
        println!("序列化反序列化测试通过！");
    }

    #[test]
    fn test_aad_encryption_decryption() {
        // 生成 ECDH 共享密钥
        let alice_secret = EphemeralSecret::random(&mut OsRng);
        let bob_secret = EphemeralSecret::random(&mut OsRng);
        let shared_secret1 = alice_secret.diffie_hellman(&bob_secret.public_key());
        let shared_secret2 = bob_secret.diffie_hellman(&alice_secret.public_key());

        // 测试数据
        let mut plaintext = b"Sensitive data".to_vec();
        let additional_data = b"Header information";
        let key = b"aes_encryption_key";

        // 加密（带额外认证数据）
        let mut encrypted =
            encrypt_with_aad(&mut plaintext, additional_data, &shared_secret1, key).unwrap();

        // 解密（带额外认证数据）
        let decrypted =
            decrypt_with_aad(&mut encrypted, additional_data, &shared_secret2, key).unwrap();

        // 验证
        assert_eq!(plaintext, decrypted.as_slice());
        println!("AAD 加密解密测试通过！");
    }
}
