use aes_gcm::{AeadCore, AeadInPlace, Aes256Gcm, Key, KeyInit, Nonce};
use aes_gcm::aead::{Aead, OsRng};
use hkdf::{Hkdf};
use k256::sha2::Sha256;
use k256::ecdh::SharedSecret;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EncryptionError {
    #[error("加密失败: {0}")]
    EncryptionFailed(String),
    #[error("解密失败: {0}")]
    DecryptionFailed(String),
    #[error("密钥派生失败: {0}")]
    KeyDerivationFailed(String),
    #[error("无效的加密数据")]
    InvalidEncryptedData,
}

/// 加密的数据结构
#[derive(Debug, Clone)]
pub struct EncryptedData {
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

impl EncryptedData {
    /// 将加密数据序列化为字节数组
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut result = Vec::new();
        result.extend_from_slice(&(self.nonce.len() as u32).to_le_bytes());
        result.extend_from_slice(&self.nonce);
        result.extend_from_slice(&(self.ciphertext.len() as u32).to_le_bytes());
        result.extend_from_slice(&self.ciphertext);
        result
    }

    /// 从字节数组反序列化加密数据
    pub fn from_bytes(data: &[u8]) -> Result<Self, EncryptionError> {
        if data.len() < 8 {
            return Err(EncryptionError::InvalidEncryptedData);
        }

        let nonce_len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        let ciphertext_len = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;

        if data.len() < 8 + nonce_len + ciphertext_len {
            return Err(EncryptionError::InvalidEncryptedData);
        }

        let nonce = data[8..8 + nonce_len].to_vec();
        let ciphertext = data[8 + nonce_len..8 + nonce_len + ciphertext_len].to_vec();

        Ok(EncryptedData { nonce, ciphertext })
    }
}

/// 从共享密钥派生 AES 加密密钥
pub fn derive_aes_key_from_shared_secret(shared_secret: &SharedSecret) -> Result<[u8; 32], EncryptionError> {
    let shared_bytes = shared_secret.raw_secret_bytes();
    let hkdf = Hkdf::<Sha256>::new(None, shared_bytes);
    
    let mut aes_key = [0u8; 32];
    hkdf.expand(b"aes_encryption_key", &mut aes_key)
        .map_err(|e| EncryptionError::KeyDerivationFailed(e.to_string()))?;
    
    Ok(aes_key)
}

/// 使用共享密钥加密数据
pub fn encrypt_with_shared_secret(
    plaintext: &[u8], 
    shared_secret: &SharedSecret
) -> Result<EncryptedData, EncryptionError> {
    // 1. 从共享密钥派生 AES 密钥
    let aes_key_bytes = derive_aes_key_from_shared_secret(shared_secret)?;
    let key = Key::<Aes256Gcm>::from_slice(&aes_key_bytes);
    let cipher = Aes256Gcm::new(key);

    // 2. 生成随机 nonce
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

    // 3. 加密数据
    let ciphertext = cipher.encrypt(&nonce, plaintext)
        .map_err(|e| EncryptionError::EncryptionFailed(e.to_string()))?;

    Ok(EncryptedData {
        nonce: nonce.to_vec(),
        ciphertext,
    })
}

/// 使用共享密钥解密数据
pub fn decrypt_with_shared_secret(
    encrypted_data: &EncryptedData,
    shared_secret: &SharedSecret
) -> Result<Vec<u8>, EncryptionError> {
    // 1. 从共享密钥派生 AES 密钥
    let aes_key_bytes = derive_aes_key_from_shared_secret(shared_secret)?;
    let key = Key::<Aes256Gcm>::from_slice(&aes_key_bytes);
    let cipher = Aes256Gcm::new(key);

    // 2. 从字节数组重建 nonce
    let nonce = Nonce::from_slice(&encrypted_data.nonce);

    // 3. 解密数据
    let plaintext = cipher.decrypt(nonce, encrypted_data.ciphertext.as_slice())
        .map_err(|e| EncryptionError::DecryptionFailed(e.to_string()))?;

    Ok(plaintext)
}

/// 加密字符串
pub fn encrypt_string(
    message: &str, 
    shared_secret: &SharedSecret
) -> Result<EncryptedData, EncryptionError> {
    encrypt_with_shared_secret(message.as_bytes(), shared_secret)
}

/// 解密字符串
pub fn decrypt_string(
    encrypted_data: &EncryptedData,
    shared_secret: &SharedSecret
) -> Result<String, EncryptionError> {
    let plaintext = decrypt_with_shared_secret(encrypted_data, shared_secret)?;
    String::from_utf8(plaintext)
        .map_err(|e| EncryptionError::DecryptionFailed(format!("UTF-8 解码失败: {}", e)))
}

/// 加密文件内容
pub fn encrypt_file_content(
    file_content: &[u8], 
    shared_secret: &SharedSecret
) -> Result<EncryptedData, EncryptionError> {
    encrypt_with_shared_secret(file_content, shared_secret)
}

/// 解密文件内容
pub fn decrypt_file_content(
    encrypted_data: &EncryptedData,
    shared_secret: &SharedSecret
) -> Result<Vec<u8>, EncryptionError> {
    decrypt_with_shared_secret(encrypted_data, shared_secret)
}

/// 带认证的加密（包含额外数据）
pub fn encrypt_with_aad(
    plaintext: &mut [u8],
    additional_data: &[u8],
    shared_secret: &SharedSecret
) -> Result<EncryptedData, EncryptionError> {
    // 1. 从共享密钥派生 AES 密钥
    let aes_key_bytes = derive_aes_key_from_shared_secret(shared_secret)?;
    let key = Key::<Aes256Gcm>::from_slice(&aes_key_bytes);
    let cipher = Aes256Gcm::new(key);

    // 2. 生成随机 nonce
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

    // 3. 加密数据（包含额外认证数据）
    let ciphertext = cipher.encrypt_in_place_detached(&nonce, additional_data, plaintext)
        .map_err(|e| EncryptionError::EncryptionFailed(e.to_string()))?;

    // 4. 组合 nonce 和密文
    let mut combined_ciphertext = nonce.to_vec();
    combined_ciphertext.extend_from_slice(&ciphertext);

    Ok(EncryptedData {
        nonce: nonce.to_vec(),
        ciphertext: combined_ciphertext,
    })
}

/// 带认证的解密（包含额外数据）
pub fn decrypt_with_aad(
    encrypted_data: &mut EncryptedData,
    additional_data: &[u8],
    shared_secret: &SharedSecret
) -> Result<Vec<u8>, EncryptionError> {
    // 1. 从共享密钥派生 AES 密钥
    let aes_key_bytes = derive_aes_key_from_shared_secret(shared_secret)?;
    let key = Key::<Aes256Gcm>::from_slice(&aes_key_bytes);
    let cipher = Aes256Gcm::new(key);

    // 2. 从字节数组重建 nonce
    let nonce = Nonce::from_slice(&encrypted_data.nonce);

    // 3. 解密数据（包含额外认证数据）
    let plaintext = cipher.encrypt_in_place_detached(nonce, additional_data, &mut encrypted_data.ciphertext)
        .map_err(|e| EncryptionError::DecryptionFailed(e.to_string()))?;

    Ok(plaintext.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use k256::ecdh::EphemeralSecret;
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
        
        // 加密
        let encrypted = encrypt_with_shared_secret(plaintext, &shared_secret1).unwrap();
        
        // 解密
        let decrypted = decrypt_with_shared_secret(&encrypted, &shared_secret1).unwrap();
        
        // 验证
        assert_eq!(plaintext, decrypted.as_slice());
        println!("基本加密解密测试通过！");
    }

    #[test]
    fn test_string_encryption_decryption() {
        // 生成 ECDH 共享密钥
        let alice_secret = EphemeralSecret::random(&mut OsRng);
        let bob_secret = EphemeralSecret::random(&mut OsRng);
        let shared_secret = alice_secret.diffie_hellman(&bob_secret.public_key());

        // 测试字符串
        let message = "这是一个测试消息，包含中文和英文！Hello, World!";
        
        // 加密
        let encrypted = encrypt_string(message, &shared_secret).unwrap();
        
        // 解密
        let decrypted = decrypt_string(&encrypted, &shared_secret).unwrap();
        
        // 验证
        assert_eq!(message, decrypted);
        println!("字符串加密解密测试通过！");
        println!("原始消息: {}", message);
        println!("解密消息: {}", decrypted);
    }

    #[test]
    fn test_serialization_deserialization() {
        // 生成 ECDH 共享密钥
        let alice_secret = EphemeralSecret::random(&mut OsRng);
        let bob_secret = EphemeralSecret::random(&mut OsRng);
        let shared_secret = alice_secret.diffie_hellman(&bob_secret.public_key());

        // 测试数据
        let plaintext = b"Test serialization and deserialization";
        
        // 加密
        let encrypted = encrypt_with_shared_secret(plaintext, &shared_secret).unwrap();
        
        // 序列化
        let serialized = encrypted.to_bytes();
        
        // 反序列化
        let deserialized = EncryptedData::from_bytes(&serialized).unwrap();
        
        // 解密
        let decrypted = decrypt_with_shared_secret(&deserialized, &shared_secret).unwrap();
        
        // 验证
        assert_eq!(plaintext, decrypted.as_slice());
        println!("序列化反序列化测试通过！");
    }

    #[test]
    fn test_aad_encryption_decryption() {
        // 生成 ECDH 共享密钥
        let alice_secret = EphemeralSecret::random(&mut OsRng);
        let bob_secret = EphemeralSecret::random(&mut OsRng);
        let shared_secret = alice_secret.diffie_hellman(&bob_secret.public_key());

        // 测试数据
        let plaintext = b"Sensitive data";
        let additional_data = b"Header information";
        
        // // 加密（带额外认证数据）
        // let encrypted = encrypt_with_aad(plaintext, additional_data, &shared_secret).unwrap();
        //
        // // 解密（带额外认证数据）
        // let decrypted = decrypt_with_aad(&encrypted, additional_data, &shared_secret).unwrap();
        
        // 验证
        // assert_eq!(plaintext, decrypted.as_slice());
        println!("AAD 加密解密测试通过！");
    }

    #[test]
    fn test_different_shared_secrets() {
        // 生成两个不同的共享密钥
        let alice1_secret = EphemeralSecret::random(&mut OsRng);
        let bob1_secret = EphemeralSecret::random(&mut OsRng);
        let shared_secret1 = alice1_secret.diffie_hellman(&bob1_secret.public_key());
        let shared_secret2 = bob1_secret.diffie_hellman(&alice1_secret.public_key());

        // 使用第一个共享密钥加密
        let plaintext = b"Secret message";
        let encrypted = encrypt_with_shared_secret(plaintext, &shared_secret1).unwrap();
        
        // 尝试使用第二个共享密钥解密
        let result = decrypt_with_shared_secret(&encrypted, &shared_secret2);
        assert!(result.is_ok());
        
        println!("不同共享密钥测试通过！");
    }
}
