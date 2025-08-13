mod data;
mod encryption;
mod error;

// 重新导出主要的加密功能
pub use encryption::{
    decrypt_with_aad, decrypt_with_shared_secret, encrypt_with_aad, encrypt_with_shared_secret,
};

use anyhow::Result;
use hkdf::Hkdf;
use k256::sha2::Sha256;
use k256::{
    PublicKey, SecretKey,
    ecdh::SharedSecret,
    ecdsa::{Signature, SigningKey, signature::Signer},
};

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

// 从 ECDH 共享密钥派生 ECDSA 密钥对
pub fn derive_ecdsa_from_shared_secret(shared_secret: &SharedSecret) -> (SecretKey, PublicKey) {
    // 1. 使用 HKDF 从共享密钥派生私钥
    let shared_bytes = shared_secret.raw_secret_bytes();
    let hkdf = Hkdf::<Sha256>::new(None, shared_bytes);

    // 2. 派生 ECDSA 私钥
    let mut private_key_bytes = [0u8; 32];
    hkdf.expand(b"ecdsa_private_key", &mut private_key_bytes)
        .unwrap();

    // 3. 创建 ECDSA 密钥对
    let secret_key = SecretKey::from_bytes(&private_key_bytes.into()).unwrap();
    let public_key = secret_key.public_key();
    (secret_key, public_key)
}

// 使用派生的 ECDSA 密钥进行签名
pub fn sign_with_derived_ecdsa(message: &[u8], shared_secret: &SharedSecret) -> Result<Signature> {
    let (secret_key, _) = derive_ecdsa_from_shared_secret(shared_secret);
    // 创建 SigningKey
    let signing_key = SigningKey::from_bytes(&secret_key.to_bytes())?;

    // 生成签名
    let (signature, _) = signing_key.sign(message);

    Ok(signature)
}

// 验证使用派生 ECDSA 密钥的签名
pub fn verify_derived_ecdsa_signature(
    message: &[u8],
    signature: &k256::ecdsa::Signature,
    shared_secret: &k256::ecdh::SharedSecret,
) -> bool {
    use k256::ecdsa::signature::Verifier;

    let (_, public_key) = derive_ecdsa_from_shared_secret(shared_secret);
    let verifying_key = k256::ecdsa::VerifyingKey::from(public_key);
    verifying_key.verify(message, signature).is_ok()
}

#[cfg(test)]
mod tests {
    use k256::ecdh::EphemeralSecret;
    use rand_core::OsRng;

    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }

    #[test]
    fn test_ecdh1() {
        // 创建 secp256k1 对象
        // let secp = Secp256k1::new();

        // 1. 生成 Alice 和 Bob 的密钥对
        // let (alice_secret, alice_public) = secp.generate_keypair(&mut thread_rng());
        // let (bob_secret, bob_public) = secp.generate_keypair(&mut thread_rng());

        // // 2. 计算共享密钥
        // let alice_shared_secret = secp.ecdh(&bob_public, &alice_secret);
        // let bob_shared_secret = secp.ecdh(&alice_public, &bob_secret);

        // // 3. 打印结果
        // println!("Alice's Shared Secret: {:?}", alice_shared_secret);
        // println!("Bob's Shared Secret: {:?}", bob_shared_secret);

        // // 验证共享密钥是否相同
        // assert_eq!(alice_shared_secret, bob_shared_secret);
    }

    #[test]
    fn test_ecdh_with_ephemeral_secret() {
        // 使用 EphemeralSecret 进行 ECDH 交换

        // Alice 生成临时密钥
        let alice_secret = EphemeralSecret::random(&mut OsRng);
        let alice_public = alice_secret.public_key();

        // Bob 生成临时密钥
        let bob_secret = EphemeralSecret::random(&mut OsRng);
        let bob_public = bob_secret.public_key();

        // 计算共享密钥
        let alice_shared = alice_secret.diffie_hellman(&bob_public);
        let bob_shared = bob_secret.diffie_hellman(&alice_public);

        // 打印十六进制格式的共享密钥
        println!(
            "Alice's Shared Secret (hex): 0x{}",
            hex::encode(alice_shared.raw_secret_bytes())
        );
        println!(
            "Bob's Shared Secret (hex): 0x{}",
            hex::encode(bob_shared.raw_secret_bytes())
        );

        // 验证共享密钥相同
        assert_eq!(
            alice_shared.raw_secret_bytes(),
            bob_shared.raw_secret_bytes()
        );

        println!("Ephemeral ECDH 共享密钥生成成功！");
        println!(
            "共享密钥长度: {} 字节",
            alice_shared.raw_secret_bytes().len()
        );
    }

    #[test]
    fn test_derive_ecdsa_from_shared_secret() {
        // 1. 生成 ECDH 共享密钥
        let alice_secret = EphemeralSecret::random(&mut OsRng);
        let bob_secret = EphemeralSecret::random(&mut OsRng);
        let shared_secret1 = alice_secret.diffie_hellman(&bob_secret.public_key());
        let shared_secret2 = bob_secret.diffie_hellman(&alice_secret.public_key());

        // 5. 测试签名和验证
        let message = b"Hello, ECDSA derived from ECDH!";
        let signature = sign_with_derived_ecdsa(message, &shared_secret1).unwrap();
        let is_valid = verify_derived_ecdsa_signature(message, &signature, &shared_secret2);

        assert!(is_valid, "ECDSA 签名验证失败");
        println!("ECDSA 签名验证成功！");

        // 6. 打印签名
        println!(
            "ECDSA Signature (hex): 0x{}",
            hex::encode(signature.to_bytes())
        );
    }
}
