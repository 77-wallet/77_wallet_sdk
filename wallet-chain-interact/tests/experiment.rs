use alloy::primitives::Address;
use alloy::sol_types::SolValue;
use bitcoin::{
    bip32::{Xpriv, Xpub},
    hashes::{ripemd160, sha256, Hash},
    key::Secp256k1,
};
use hex::decode;
use libsecp256k1::{Message, SecretKey};
use sha2::Digest;
use sha2::Sha256;
use std::time::{Duration, UNIX_EPOCH};

#[test]
fn readable_hex() {
    let hex_str = "436f6e74726163742076616c6964617465206572726f72203a204e6f20636f6e747261637421";
    let bytes = hex::decode(hex_str).expect("Decoding failed");

    let readable_str = String::from_utf8(bytes).expect("Invalid UTF-8");
    println!("Readable string: {}", readable_str);
}

#[test]
fn test_sign() {
    // let hash = "159817a085f113d099d3d93c051410e9bfe043cc5c20e43aa9a083bf73660145";
    let hash = "8ad8702f0b89a5894c9706d671817bd13aeb0bd17fe28b307057d5767298d5a9";
    // let hash = "2359f1c41ed99d25f90b60d6a451376c95b87108928c3de8ed0a3fbdb11afa59";
    let hash = hex::decode(hash).unwrap();
    let message = Message::parse_slice(&hash).unwrap();

    // let private_key_hex = "8e812436a0e3323166e1f0e8ba79e19e217b2c4a53c970d4cca0cfb1078979df";
    let private_key_hex = "2e8b5599ed26d8b465d66049c42df32df744baf4bcf18f777d2daf6b28e06e5b";
    let private_key_bytes = decode(private_key_hex).unwrap();
    let private_key = SecretKey::parse_slice(&private_key_bytes).unwrap();

    let (signature, recovery_id) = libsecp256k1::sign(&message, &private_key);

    let sss = signature.serialize();
    let mut full_signature = vec![0u8; 65];
    full_signature[..64].copy_from_slice(&sss);
    let id: u8 = recovery_id.into();
    let id = id + 27;
    full_signature[64] = id;

    println!("{:?}", hex::encode(full_signature));
}

#[test]
fn test_abi() {
    let addr = "410583A68A3BCD86C25AB1BEE482BAC04A216B0261";
    // 将地址从十六进制字符串转换为字节数组
    let addr_bytes = hex::decode(&addr).unwrap();

    // 截取中间的20个字节
    let address_bytes = &addr_bytes[1..21];
    let address = Address::from_slice(address_bytes);

    let abi = address.abi_encode();
    println!("字节的长度{}", abi.len());

    // 将编码后的字节数组转换为十六进制字符串
    let encoded_address = hex::encode(abi);

    // 输出编码结果
    println!("编码后的地址: {}", encoded_address);
    // 断言编码结果是否符合预期
    assert_eq!(
        encoded_address,
        "0000000000000000000000000583a68a3bcd86c25ab1bee482bac04a216b0261"
    );
}

#[test]
fn test_btc_addre1() {
    let xpriv_str = "xprv9s21ZrQH143K2rP7uQpyU2DsUZeHGK6kwmPEsgLengJ6RiGbBKKo3dJHYkcgzUDZUj8HxcsxpYubPQnM37j7Ja4ypTDB8znDSRqWDDgcsfg";
    let xpriv = xpriv_str.parse::<Xpriv>().unwrap();
    let secp = Secp256k1::new();
    let xpub = Xpub::from_priv(&secp, &xpriv);

    println!("xpub: {}", xpub);

    let xpub_bytes = xpub.public_key.serialize();
    let sha256_hash = sha256::Hash::hash(&xpub_bytes);
    let ripemd160_hash = ripemd160::Hash::hash(&sha256_hash.to_byte_array());
    let fingerprint = &ripemd160_hash[..4]; // 取前4个字节作为指纹

    // 将指纹转换为十六进制字符串
    let fingerprint_hex = fingerprint
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();

    // 打印指纹
    println!("Master fingerprint: {}", fingerprint_hex);

    // 验证与常量是否匹配
    const EXPECTED_FINGERPRINT: &str = "9680603f";
    assert_eq!(
        fingerprint_hex, EXPECTED_FINGERPRINT,
        "Fingerprints do not match!"
    );
}

#[test]
fn test_time() {
    let timestamp: i64 = 1625097600000; // 例如：2021-07-01 00:00:00 UTC (毫秒)

    // 将时间戳转换为 SystemTime
    let system_time = UNIX_EPOCH + Duration::from_millis(timestamp as u64);

    // 创建一个表示一天时间的 Duration (86400000 毫秒)
    let one_day = Duration::from_millis(24 * 60 * 60 * 1000);

    // 将 SystemTime 和 Duration 相加
    let new_system_time = system_time + one_day;

    // 将新的 SystemTime 转换回时间戳（以毫秒为单位）
    let new_timestamp = new_system_time
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis() as i64;

    println!("Original timestamp: {}", timestamp);
    println!("New timestamp: {}", new_timestamp);
}

#[test]
fn test_num() {
    let instruction_name = "global:multisig_create_v2";

    // 计算分辨符
    let mut hasher = Sha256::new();
    hasher.update(instruction_name.as_bytes());
    let hash_result = hasher.finalize();
    let discriminator: [u8; 8] = hash_result[..8]
        .try_into()
        .expect("slice with incorrect length");

    println!("Discriminator: {:?}", hex::encode(discriminator));
}
