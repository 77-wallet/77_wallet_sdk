use super::WalletService;
use crate::{
    context::Context,
    device_protection::{
        DeviceProtection, DeviceProtectionProvider, ProtectionContext, ProtectionError,
        ProtectionFuture,
        keystore_format::{KeystoreProtection, KeystoreProtectionError, ReadPolicy},
        migration::MigrationFile,
    },
    dirs::Dirs,
    error::service::ServiceError,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use wallet_tree::file_ops::RootData;

#[derive(Default)]
struct Native {
    locked: AtomicBool,
    opens: AtomicUsize,
}
impl DeviceProtectionProvider for Native {
    fn protect<'a>(&'a self, ctx: &'a ProtectionContext, input: &'a [u8]) -> ProtectionFuture<'a> {
        Box::pin(async move {
            let mut result = ctx.authenticated_data();
            result.extend_from_slice(input);
            Ok(result)
        })
    }
    fn unprotect<'a>(&'a self, ctx: &'a ProtectionContext, blob: &'a [u8]) -> ProtectionFuture<'a> {
        Box::pin(async move {
            self.opens.fetch_add(1, Ordering::SeqCst);
            if self.locked.load(Ordering::SeqCst) {
                return Err(ProtectionError::Locked);
            }
            blob.strip_prefix(ctx.authenticated_data().as_slice())
                .map(Vec::from)
                .ok_or(ProtectionError::AuthenticationFailed)
        })
    }
}
const WALLET: &str = "fixturewallet";
const PASSWORD: &str = "fixture-password";
const PHRASE: &str = "synthetic phrase payload";
fn directory() -> tempfile::TempDir {
    let base =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/phrase-service-tests");
    std::fs::create_dir_all(&base).unwrap();
    tempfile::tempdir_in(base).unwrap()
}
async fn context(path: &std::path::Path, native: Arc<Native>) -> &'static Context {
    let config = serde_json::from_value(serde_json::json!({
        "backend_api": {"dev_url":"http://127.0.0.1:1", "test_url":"http://127.0.0.1:1", "prod_url":"http://127.0.0.1:1"},
        "aggregate_api": {"dev_url":"http://127.0.0.1:1", "test_url":"http://127.0.0.1:1", "prod_url":"http://127.0.0.1:1"},
        "crypto": {"aes_key":"00000000000000000000000000000000", "aes_iv":"0000000000000000"},
        "app_code":"fixture"
    })).unwrap();
    let ctx =
        Context::new_without_handles_for_test(Dirs::new(path.to_str().unwrap()).unwrap(), config)
            .await
            .unwrap()
            .with_device_protection_for_test(DeviceProtection::new(native));
    Box::leak(Box::new(ctx))
}

#[tokio::test]
async fn protected_phrase_uses_actual_service_after_context_reopen() {
    let dir = directory();
    let native = Arc::new(Native::default());
    let ctx = context(dir.path(), native.clone()).await;
    let root = ctx.get_global_dirs().get_root_dir(WALLET).unwrap();
    let codec = KeystoreProtection::new(ctx.device_protection().clone());
    codec
        .store_root(
            &root,
            WALLET,
            RootData::new(PHRASE, b"synthetic seed"),
            PASSWORD,
            wallet_crypto::KdfAlgorithm::Scrypt,
        )
        .await
        .unwrap();
    let before = std::fs::read(root.join("root.keystore")).unwrap();
    let response = WalletService::new(ctx).get_phrase(WALLET, PASSWORD).await.unwrap();
    assert!(response.phrase == PHRASE);
    let reopened = context(dir.path(), native.clone()).await;
    assert!(
        WalletService::new(reopened).get_phrase(WALLET, PASSWORD).await.unwrap().phrase == PHRASE
    );
    let wrong = WalletService::new(reopened).get_phrase(WALLET, "wrong-fixture-password").await;
    assert!(matches!(
        wrong,
        Err(ServiceError::DeviceProtection(KeystoreProtectionError::PasswordDecryption))
    ));
    native.locked.store(true, Ordering::SeqCst);
    let result = WalletService::new(reopened).get_phrase(WALLET, PASSWORD).await;
    assert!(matches!(
        result,
        Err(ServiceError::DeviceProtection(KeystoreProtectionError::Protection(
            ProtectionError::Locked
        )))
    ));
    assert!(std::fs::read(root.join("root.keystore")).unwrap() == before);
}

#[tokio::test]
async fn legacy_phrase_stays_readable_but_completed_migration_blocks_rollback() {
    use wallet_tree::wallet_hierarchy::{WalletTreeOps, v1::LegacyWalletTree};
    let dir = directory();
    let native = Arc::new(Native::default());
    let ctx = context(dir.path(), native.clone()).await;
    let root = ctx.get_global_dirs().get_root_dir(WALLET).unwrap();
    LegacyWalletTree::default()
        .io()
        .store_root(
            WALLET,
            b"synthetic seed",
            PHRASE,
            &root,
            PASSWORD,
            wallet_crypto::KdfAlgorithm::Scrypt,
        )
        .unwrap();
    let legacy = std::fs::read(root.join("fixturewallet-phrase")).unwrap();
    assert!(WalletService::new(ctx).get_phrase(WALLET, PASSWORD).await.unwrap().phrase == PHRASE);
    assert_eq!(native.opens.load(Ordering::SeqCst), 0);
    let codec = KeystoreProtection::new(ctx.device_protection().clone());
    codec
        .store_root(
            &root,
            WALLET,
            RootData::new(PHRASE, b"synthetic seed"),
            PASSWORD,
            wallet_crypto::KdfAlgorithm::Scrypt,
        )
        .await
        .unwrap();
    let binding = ProtectionContext::new(WALLET, "root-keystore").unwrap();
    let path = root.join("root.keystore");
    codec
        .migrate_files(
            &ctx.core_pool().unwrap(),
            &[MigrationFile { path: &path, context: &binding }],
            PASSWORD,
            ReadPolicy::DeviceRequired,
        )
        .await
        .unwrap();
    std::fs::remove_file(&path).unwrap();
    // Older root files remain valid but may not be used once this root completed migration.
    assert!(WalletService::new(ctx).get_phrase(WALLET, PASSWORD).await.is_err());
    assert!(std::fs::read(root.join("fixturewallet-phrase")).unwrap() == legacy);
}

#[tokio::test]
async fn modern_password_only_phrase_uses_existing_reader_without_native_calls() {
    use wallet_tree::wallet_hierarchy::{WalletTreeOps, v2::ModernWalletTree};
    let dir = directory();
    let native = Arc::new(Native::default());
    let ctx = context(dir.path(), native.clone()).await;
    wallet_database::repositories::config::ConfigRepo::upsert(
        "wallet_tree_strategy",
        r#"{"wallet_tree_strategy":"v2"}"#,
        Some(0),
        &ctx.core_pool().unwrap(),
    )
    .await
    .unwrap();
    let root = ctx.get_global_dirs().get_root_dir(WALLET).unwrap();
    ModernWalletTree::default()
        .io()
        .store_root(
            WALLET,
            b"synthetic seed",
            PHRASE,
            &root,
            PASSWORD,
            wallet_crypto::KdfAlgorithm::Scrypt,
        )
        .unwrap();
    let path = root.join("root.keystore");
    let before = std::fs::read(&path).unwrap();
    assert!(WalletService::new(ctx).get_phrase(WALLET, PASSWORD).await.unwrap().phrase == PHRASE);
    assert_eq!(native.opens.load(Ordering::SeqCst), 0);
    assert!(std::fs::read(&path).unwrap() == before);
    std::fs::write(&path, b"WDK\x02unsupported-fixture").unwrap();
    let failure = WalletService::new(ctx).get_phrase(WALLET, PASSWORD).await;
    assert!(matches!(
        failure,
        Err(ServiceError::DeviceProtection(KeystoreProtectionError::InvalidFormat))
    ));
    assert_eq!(native.opens.load(Ordering::SeqCst), 0);
    assert!(std::fs::read(&path).unwrap() == b"WDK\x02unsupported-fixture");
}
