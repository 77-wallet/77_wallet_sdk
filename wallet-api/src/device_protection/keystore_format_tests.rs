use super::*;
use crate::device_protection::{DeviceProtectionProvider, ProtectionFuture};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use wallet_crypto::EncryptedJsonGenerator;

#[derive(Default)]
struct Fixture {
    protects: AtomicUsize,
    opens: AtomicUsize,
}
impl DeviceProtectionProvider for Fixture {
    fn protect<'a>(&'a self, ctx: &'a ProtectionContext, input: &'a [u8]) -> ProtectionFuture<'a> {
        Box::pin(async move {
            self.protects.fetch_add(1, Ordering::SeqCst);
            let mut bytes = ctx.authenticated_data();
            bytes.extend_from_slice(input);
            Ok(bytes)
        })
    }
    fn unprotect<'a>(
        &'a self,
        ctx: &'a ProtectionContext,
        input: &'a [u8],
    ) -> ProtectionFuture<'a> {
        Box::pin(async move {
            self.opens.fetch_add(1, Ordering::SeqCst);
            input
                .strip_prefix(ctx.authenticated_data().as_slice())
                .map(Vec::from)
                .ok_or(ProtectionError::AuthenticationFailed)
        })
    }
}
fn legacy() -> Vec<u8> {
    let json = wallet_crypto::KeystoreJsonGenerator::new(
        rand::thread_rng(),
        wallet_crypto::KdfAlgorithm::Scrypt,
    )
    .generate(b"fixture-password", b"synthetic-payload")
    .unwrap();
    serde_json::to_vec(&json).unwrap()
}
fn context() -> ProtectionContext {
    ProtectionContext::new("wallet-fixture", "root-keystore").unwrap()
}

#[tokio::test]
async fn new_file_never_enters_migration_or_double_wraps() {
    let native = Arc::new(Fixture::default());
    let codec = KeystoreProtection::new(DeviceProtection::new(native.clone()));
    let sealed = codec.seal(&context(), &legacy()).await.unwrap();
    for policy in [ReadPolicy::LegacyMigrationAllowed, ReadPolicy::DeviceRequired] {
        let opened = codec.open(&context(), &sealed, "fixture-password", policy).await.unwrap();
        assert_eq!(opened.plaintext.as_slice(), b"synthetic-payload");
        assert!(opened.replacement.is_none());
    }
    assert_eq!(native.protects.load(Ordering::SeqCst), 1);
    assert!(matches!(
        codec.seal(&context(), &sealed).await,
        Err(KeystoreProtectionError::InvalidFormat)
    ));
}

#[tokio::test]
async fn old_file_upgrades_once_only_after_correct_password() {
    let native = Arc::new(Fixture::default());
    let codec = KeystoreProtection::new(DeviceProtection::new(native.clone()));
    let original = legacy();
    assert!(matches!(
        codec.open(&context(), &original, "wrong", ReadPolicy::LegacyMigrationAllowed).await,
        Err(KeystoreProtectionError::PasswordDecryption)
    ));
    assert_eq!(native.protects.load(Ordering::SeqCst), 0);
    let opened = codec
        .open(&context(), &original, "fixture-password", ReadPolicy::LegacyMigrationAllowed)
        .await
        .unwrap();
    let upgraded = opened.replacement.unwrap();
    let again = codec
        .open(&context(), &upgraded, "fixture-password", ReadPolicy::DeviceRequired)
        .await
        .unwrap();
    assert!(again.replacement.is_none());
    assert_eq!(again.plaintext.as_slice(), b"synthetic-payload");
    assert_eq!(native.protects.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn required_policy_and_unknown_formats_never_fall_back() {
    let codec = KeystoreProtection::new(DeviceProtection::default());
    assert!(matches!(
        codec.open(&context(), &legacy(), "fixture-password", ReadPolicy::DeviceRequired).await,
        Err(KeystoreProtectionError::LegacyForbidden)
    ));
    for bytes in [b"WDK\x02unknown".as_slice(), b"WDK\x01", b"damaged"] {
        assert!(matches!(
            codec
                .open(&context(), bytes, "fixture-password", ReadPolicy::LegacyMigrationAllowed)
                .await,
            Err(KeystoreProtectionError::InvalidFormat)
        ));
    }
    assert!(matches!(
        codec
            .open(
                &context(),
                b"WDK\x01native",
                "fixture-password",
                ReadPolicy::LegacyMigrationAllowed
            )
            .await,
        Err(KeystoreProtectionError::Protection(ProtectionError::NotConfigured))
    ));
}

#[tokio::test]
async fn native_failure_does_not_release_legacy_migration_or_downgrade_current_data() {
    let codec = KeystoreProtection::new(DeviceProtection::default());
    assert!(matches!(
        codec
            .open(&context(), &legacy(), "fixture-password", ReadPolicy::LegacyMigrationAllowed)
            .await,
        Err(KeystoreProtectionError::Protection(ProtectionError::NotConfigured))
    ));
    let native = Arc::new(Fixture::default());
    let codec = KeystoreProtection::new(DeviceProtection::new(native.clone()));
    let sealed = codec.seal(&context(), &legacy()).await.unwrap();
    let wrong = ProtectionContext::new("different-wallet", "root-keystore").unwrap();
    assert!(matches!(
        codec.open(&wrong, &sealed, "fixture-password", ReadPolicy::LegacyMigrationAllowed).await,
        Err(KeystoreProtectionError::Protection(ProtectionError::AuthenticationFailed))
    ));
    assert_eq!(native.protects.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn excessive_kdf_cost_is_rejected_without_native_calls() {
    let native = Arc::new(Fixture::default());
    let codec = KeystoreProtection::new(DeviceProtection::new(native.clone()));
    let mut json: serde_json::Value = serde_json::from_slice(&legacy()).unwrap();
    json["crypto"]["kdfparams"]["n"] = serde_json::json!(2147483648u32);
    let malicious = serde_json::to_vec(&json).unwrap();
    assert!(matches!(
        codec
            .open(&context(), &malicious, "fixture-password", ReadPolicy::LegacyMigrationAllowed)
            .await,
        Err(KeystoreProtectionError::UnsupportedKdfCost)
    ));
    assert_eq!(native.protects.load(Ordering::SeqCst), 0);
    assert_eq!(native.opens.load(Ordering::SeqCst), 0);
}

#[test]
fn current_generated_kdf_profiles_remain_supported() {
    let mut json: serde_json::Value = serde_json::from_slice(&legacy()).unwrap();
    for algorithm in [
        wallet_crypto::KdfAlgorithm::Argon2id,
        wallet_crypto::KdfAlgorithm::Scrypt,
        wallet_crypto::KdfAlgorithm::Pbkdf2,
    ] {
        let kdf = wallet_crypto::kdf::factory::KdfFactory::create(&algorithm, &[1; 32]).unwrap();
        json["crypto"]["kdf"] = serde_json::to_value(algorithm).unwrap();
        json["crypto"]["kdfparams"] = serde_json::to_value(kdf.params()).unwrap();
        validate_inner(&serde_json::to_vec(&json).unwrap()).unwrap();
    }
}
