use super::WalletApplication;
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
    service::wallet::WalletService,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use wallet_tree::file_ops::RootData;

#[derive(Default)]
struct Native {
    locked: AtomicBool,
    fail_protect: AtomicBool,
    opens: AtomicUsize,
}
impl DeviceProtectionProvider for Native {
    fn protect<'a>(&'a self, ctx: &'a ProtectionContext, input: &'a [u8]) -> ProtectionFuture<'a> {
        Box::pin(async move {
            if self.fail_protect.load(Ordering::SeqCst) {
                return Err(ProtectionError::PlatformFailure);
            }
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

#[tokio::test]
async fn protected_seed_and_password_fallback_use_device_reader() {
    let dir = directory();
    let native = Arc::new(Native::default());
    let ctx = context(dir.path(), native.clone()).await;
    let dirs = ctx.get_global_dirs();
    let root = dirs.get_root_dir(WALLET).unwrap();
    KeystoreProtection::new(ctx.device_protection().clone())
        .store_root(
            &root,
            WALLET,
            RootData::new(PHRASE, b"synthetic seed"),
            PASSWORD,
            wallet_crypto::KdfAlgorithm::Scrypt,
        )
        .await
        .unwrap();
    wallet_database::repositories::wallet::WalletRepo::upsert_wallet(
        ctx.core_pool().unwrap(),
        WALLET,
        "fixture-uid",
        "fixture",
    )
    .await
    .unwrap();
    let before = std::fs::read(root.join("root.keystore")).unwrap();
    let seed = WalletApplication::get_seed(ctx, WALLET, PASSWORD).await.unwrap();
    assert!(seed == b"synthetic seed");
    assert!(WalletApplication::try_decrypt_wallet_db(ctx, PASSWORD).await.unwrap());
    assert!(
        !WalletApplication::try_decrypt_wallet_db(ctx, "wrong-fixture-password").await.unwrap()
    );
    native.locked.store(true, Ordering::SeqCst);
    let failure = WalletApplication::try_decrypt_wallet_db(ctx, PASSWORD).await;
    assert!(matches!(
        failure,
        Err(ServiceError::DeviceProtection(KeystoreProtectionError::Protection(
            ProtectionError::Locked
        )))
    ));
    assert!(std::fs::read(root.join("root.keystore")).unwrap() == before);
}

#[tokio::test]
async fn legacy_password_fallback_preserves_success_and_wrong_password_behavior() {
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
    wallet_database::repositories::wallet::WalletRepo::upsert_wallet(
        ctx.core_pool().unwrap(),
        WALLET,
        "fixture-uid",
        "fixture",
    )
    .await
    .unwrap();
    assert!(WalletApplication::try_decrypt_wallet_db(ctx, PASSWORD).await.unwrap());
    assert!(
        !WalletApplication::try_decrypt_wallet_db(ctx, "wrong-fixture-password").await.unwrap()
    );
    assert_eq!(native.opens.load(Ordering::SeqCst), 0);
}

async fn register_wallet_for_password_change(ctx: &Context) {
    use wallet_database::{
        entities::config::{KeystoreKdfAlgorithm, config_key::KEYSTORE_KDF_ALGORITHM},
        repositories::{config::ConfigRepo, wallet::WalletRepo},
    };
    let pool = ctx.core_pool().unwrap();
    WalletRepo::upsert_wallet(pool.clone(), WALLET, "fixture-uid", "fixture").await.unwrap();
    let config =
        KeystoreKdfAlgorithm { keystore_kdf_algorithm: wallet_crypto::KdfAlgorithm::Scrypt };
    ConfigRepo::upsert(KEYSTORE_KDF_ALGORITHM, &config.to_json_str().unwrap(), Some(0), &pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn root_password_change_preserves_wdk_and_recovers_from_native_failure() {
    use crate::domain::account::AccountDomain;
    let dir = directory();
    let native = Arc::new(Native::default());
    let ctx = context(dir.path(), native.clone()).await;
    register_wallet_for_password_change(ctx).await;
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
    let path = root.join("root.keystore");
    let before = std::fs::read(&path).unwrap();
    native.fail_protect.store(true, Ordering::SeqCst);
    let failure =
        AccountDomain::set_root_password(ctx, WALLET, PASSWORD, "new-fixture-password").await;
    assert!(matches!(
        failure,
        Err(ServiceError::DeviceProtection(KeystoreProtectionError::Protection(
            ProtectionError::PlatformFailure
        )))
    ));
    assert!(std::fs::read(&path).unwrap() == before);
    assert!(WalletService::new(ctx).get_phrase(WALLET, PASSWORD).await.unwrap().phrase == PHRASE);
    native.fail_protect.store(false, Ordering::SeqCst);
    let wrong = AccountDomain::set_root_password(
        ctx,
        WALLET,
        "wrong-fixture-password",
        "new-fixture-password",
    )
    .await;
    assert!(matches!(
        wrong,
        Err(ServiceError::DeviceProtection(KeystoreProtectionError::PasswordDecryption))
    ));
    assert!(std::fs::read(&path).unwrap() == before);
    AccountDomain::set_root_password(ctx, WALLET, PASSWORD, "new-fixture-password").await.unwrap();
    let after = std::fs::read(&path).unwrap();
    assert!(after.starts_with(b"WDK\x01") && after != before);
    assert!(
        WalletService::new(ctx).get_phrase(WALLET, "new-fixture-password").await.unwrap().phrase
            == PHRASE
    );
    assert!(
        WalletApplication::get_seed(ctx, WALLET, "new-fixture-password").await.unwrap()
            == b"synthetic seed"
    );
    assert!(matches!(
        WalletService::new(ctx).get_phrase(WALLET, PASSWORD).await,
        Err(ServiceError::DeviceProtection(KeystoreProtectionError::PasswordDecryption))
    ));
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    // Competing changes using the same old password cannot overwrite each other.
    let (left, right) = tokio::join!(
        AccountDomain::set_root_password(ctx, WALLET, "new-fixture-password", "next-fixture-a"),
        AccountDomain::set_root_password(ctx, WALLET, "new-fixture-password", "next-fixture-b"),
    );
    assert_ne!(left.is_ok(), right.is_ok());
    let winner = if left.is_ok() { "next-fixture-a" } else { "next-fixture-b" };
    assert!(WalletService::new(ctx).get_phrase(WALLET, winner).await.unwrap().phrase == PHRASE);
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
}

#[tokio::test]
async fn completed_root_password_change_rejects_legacy_rollback() {
    use crate::domain::account::AccountDomain;
    use wallet_tree::wallet_hierarchy::{WalletTreeOps, v1::LegacyWalletTree};
    let dir = directory();
    let native = Arc::new(Native::default());
    let ctx = context(dir.path(), native).await;
    register_wallet_for_password_change(ctx).await;
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
    let original = std::fs::read(root.join("fixturewallet-phrase")).unwrap();
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
    let path = root.join("root.keystore");
    let binding = ProtectionContext::new(WALLET, "root-keystore").unwrap();
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
    assert!(
        AccountDomain::set_root_password(ctx, WALLET, PASSWORD, "new-fixture-password")
            .await
            .is_err()
    );
    assert!(std::fs::read(root.join("fixturewallet-phrase")).unwrap() == original);
}

#[tokio::test]
async fn legacy_root_password_change_keeps_existing_format() {
    use crate::domain::account::AccountDomain;
    use wallet_tree::wallet_hierarchy::{WalletTreeOps, v1::LegacyWalletTree};
    let dir = directory();
    let native = Arc::new(Native::default());
    let ctx = context(dir.path(), native.clone()).await;
    register_wallet_for_password_change(ctx).await;
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
    AccountDomain::set_root_password(ctx, WALLET, PASSWORD, "new-fixture-password").await.unwrap();
    assert!(
        WalletService::new(ctx).get_phrase(WALLET, "new-fixture-password").await.unwrap().phrase
            == PHRASE
    );
    assert!(WalletService::new(ctx).get_phrase(WALLET, PASSWORD).await.is_err());
    assert!(!root.join("root.keystore").exists());
    assert_eq!(native.opens.load(Ordering::SeqCst), 0);
}
