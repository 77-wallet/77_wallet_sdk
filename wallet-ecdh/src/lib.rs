use hkdf::Hkdf;
use k256::sha2::Sha256;
use k256::ecdsa::signature::Signer;

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

// 从 ECDH 共享密钥派生 ECDSA 密钥对
pub fn derive_ecdsa_from_shared_secret(shared_secret: &k256::ecdh::SharedSecret) -> (k256::SecretKey, k256::PublicKey) {
    
    // 1. 使用 HKDF 从共享密钥派生私钥
    let shared_bytes = shared_secret.raw_secret_bytes();
    let hkdf = Hkdf::<Sha256>::new(None, shared_bytes);
    
    // 2. 派生 ECDSA 私钥
    let mut private_key_bytes = [0u8; 32];
    hkdf.expand(b"ecdsa_private_key", &mut private_key_bytes).unwrap();
    
    // 3. 创建 ECDSA 密钥对
    let secret_key = k256::SecretKey::from_bytes(&private_key_bytes.into()).unwrap();
    let public_key = secret_key.public_key();
    
    (secret_key, public_key)
}

// 使用派生的 ECDSA 密钥进行签名
pub fn sign_with_derived_ecdsa(message: &[u8], shared_secret: &k256::ecdh::SharedSecret) -> k256::ecdsa::Signature {
    
    
    let (secret_key, _) = derive_ecdsa_from_shared_secret(shared_secret);
    let signing_key = k256::ecdsa::SigningKey::from(secret_key);
    signing_key.sign(message)
}

// 验证使用派生 ECDSA 密钥的签名
pub fn verify_derived_ecdsa_signature(
    message: &[u8], 
    signature: &k256::ecdsa::Signature, 
    shared_secret: &k256::ecdh::SharedSecret
) -> bool {
    use k256::ecdsa::signature::Verifier;
    
    let (_, public_key) = derive_ecdsa_from_shared_secret(shared_secret);
    let verifying_key = k256::ecdsa::VerifyingKey::from(public_key);
    verifying_key.verify(message, signature).is_ok()
}

// 打印 PublicKey<Secp256k1> 为十六进制格式
pub fn print_public_key_hex(public_key: &k256::PublicKey) {
    // 方法1: 使用 to_bytes() 获取压缩格式
    // let compressed_bytes = public_key.to_bytes();
    // println!("Public Key (compressed): 0x{}", hex::encode(compressed_bytes));
    
    // // 方法2: 使用 to_encoded_point(false) 获取未压缩格式
    // let uncompressed_point = public_key.to_encoded_point(false);
    // let uncompressed_bytes = uncompressed_point.as_bytes();
    // println!("Public Key (uncompressed): 0x{}", hex::encode(uncompressed_bytes));
    
    // // 方法3: 分别打印 x 和 y 坐标
    // let affine = public_key.as_affine();
    // let x_bytes = affine.x().to_bytes();
    // let y_bytes = affine.y().to_bytes();
    // println!("Public Key X: 0x{}", hex::encode(x_bytes));
    // println!("Public Key Y: 0x{}", hex::encode(y_bytes));
    
    // // 方法4: 获取 SEC1 编码格式
    // let sec1_encoded = public_key.to_encoded_point(true);
    // println!("Public Key (SEC1): 0x{}", hex::encode(sec1_encoded.as_bytes()));
}

// 打印不同格式的公钥
pub fn print_all_public_key_formats(public_key: &k256::PublicKey) {
    // println!("=== PublicKey<Secp256k1> 十六进制格式 ===");
    
    // // 压缩格式 (33 字节)
    // let compressed = public_key.to_bytes();
    // println!("压缩格式 (33字节): 0x{}", hex::encode(compressed));
    
    // // 未压缩格式 (65 字节)
    // let uncompressed = public_key.to_encoded_point(false);
    // println!("未压缩格式 (65字节): 0x{}", hex::encode(uncompressed.as_bytes()));
    
    // // SEC1 编码
    // let sec1_compressed = public_key.to_encoded_point(true);
    // println!("SEC1压缩格式: 0x{}", hex::encode(sec1_compressed.as_bytes()));
    
    // // 坐标格式
    // let affine = public_key.as_affine();
    // let x = affine.x().to_bytes();
    // let y = affine.y().to_bytes();
    // println!("X 坐标: 0x{}", hex::encode(x));
    // println!("Y 坐标: 0x{}", hex::encode(y));
    
    // println!("=== 格式说明 ===");
    // println!("压缩格式: 以 02 或 03 开头，表示 Y 坐标的奇偶性");
    // println!("未压缩格式: 以 04 开头，包含完整的 X 和 Y 坐标");
    // println!("SEC1格式: 符合 SEC1 标准的编码");
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
        println!("Alice's Shared Secret (hex): 0x{}", hex::encode(alice_shared.raw_secret_bytes()));
        println!("Bob's Shared Secret (hex): 0x{}", hex::encode(bob_shared.raw_secret_bytes()));
        
        // 验证共享密钥相同
        assert_eq!(
            alice_shared.raw_secret_bytes(),
            bob_shared.raw_secret_bytes()
        );
        
        println!("Ephemeral ECDH 共享密钥生成成功！");
        println!("共享密钥长度: {} 字节", alice_shared.raw_secret_bytes().len());
    }

    #[test]
    fn test_derive_ecdsa_from_shared_secret() {
        // 1. 生成 ECDH 共享密钥
        let alice_secret = EphemeralSecret::random(&mut OsRng);
        let bob_secret = EphemeralSecret::random(&mut OsRng);
        let shared_secret = alice_secret.diffie_hellman(&bob_secret.public_key());
        
        // 2. 从共享密钥派生 ECDSA 密钥对
        let (alice_ecdsa_private, alice_ecdsa_public) = derive_ecdsa_from_shared_secret(&shared_secret);
        let (bob_ecdsa_private, bob_ecdsa_public) = derive_ecdsa_from_shared_secret(&shared_secret);
        
        // 3. 验证派生的密钥对相同
        assert_eq!(alice_ecdsa_private.to_bytes(), bob_ecdsa_private.to_bytes());
        // assert_eq!(alice_ecdsa_public.to_bytes(), bob_ecdsa_public.to_bytes());
        
        // 4. 打印派生的 ECDSA 密钥
        println!("Derived ECDSA Private Key (hex): 0x{}", hex::encode(alice_ecdsa_private.to_bytes()));
        // println!("Derived ECDSA Public Key (hex): 0x{}", hex::encode(alice_ecdsa_public.to_bytes()));
        
        // 5. 测试签名和验证
        let message = b"Hello, ECDSA derived from ECDH!";
        let signature = sign_with_derived_ecdsa(message, &shared_secret);
        let is_valid = verify_derived_ecdsa_signature(message, &signature, &shared_secret);
        
        assert!(is_valid, "ECDSA 签名验证失败");
        println!("ECDSA 签名验证成功！");
        
        // 6. 打印签名
        println!("ECDSA Signature (hex): 0x{}", hex::encode(signature.to_bytes()));
    }
}
