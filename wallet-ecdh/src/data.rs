use crate::error::EncryptionError;

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
        result.extend_from_slice(&(self.ciphertext.len() as u32).to_le_bytes());
        result.extend_from_slice(&self.nonce);
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