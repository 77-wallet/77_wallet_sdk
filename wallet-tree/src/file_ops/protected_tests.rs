use super::*;
use secrecy::zeroize::Zeroizing;
use wallet_crypto::{EncryptedJsonDecryptor, KdfAlgorithm};

fn fixture() -> tempfile::TempDir {
    let parent =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/shared-keystore-tests");
    std::fs::create_dir_all(&parent).unwrap();
    tempfile::tempdir_in(parent).unwrap()
}

#[tokio::test]
async fn same_io_handles_root_derived_and_verify_as_opaque_bytes() {
    let dir = fixture();
    // Synthetic binary payload deliberately unrelated to RootData or any JSON schema.
    for name in ["root.keystore", "key0.keystore", "verify"] {
        let path = dir.path().join(name);
        let payload = vec![0, 255, 128, 1, 2];
        store(
            &path,
            Zeroizing::new(payload.clone()),
            "fixture-password",
            KdfAlgorithm::Scrypt,
            |inner| async move {
                tokio::task::yield_now().await;
                let mut bytes = b"fixture-envelope:".to_vec();
                bytes.extend(inner);
                Ok::<_, crate::Error>(bytes)
            },
        )
        .await
        .unwrap();
        assert!(std::fs::read(&path).unwrap().starts_with(b"fixture-envelope:"));
        let decoded = load(&path, |stored| async move {
            let json =
                std::str::from_utf8(stored.strip_prefix(b"fixture-envelope:").unwrap()).unwrap();
            Ok::<_, crate::Error>(
                wallet_crypto::KeystoreJsonDecryptor.decrypt(b"fixture-password", json)?,
            )
        })
        .await
        .unwrap();
        assert!(decoded == payload);
    }
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 3);
}

#[tokio::test]
async fn failed_protection_preserves_each_existing_file() {
    let dir = fixture();
    for name in ["root.keystore", "key0.keystore", "verify"] {
        let path = dir.path().join(name);
        std::fs::write(&path, b"existing synthetic ciphertext").unwrap();
        let result = store(
            &path,
            Zeroizing::new(vec![0, 255]),
            "fixture-password",
            KdfAlgorithm::Scrypt,
            |_| async { Err::<Vec<u8>, _>(crate::Error::InvalidKeyFormat) },
        )
        .await;
        assert!(result.is_err());
        assert!(std::fs::read(path).unwrap() == b"existing synthetic ciphertext");
    }
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 3);
}
