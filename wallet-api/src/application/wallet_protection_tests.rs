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

async fn derived_password_fixture(ctx: &Context) -> std::path::PathBuf {
    use wallet_tree::{
        file_ops::BulkSubkey,
        wallet_hierarchy::{WalletTreeOps, v2::ModernWalletTree},
    };
    use wallet_utils::address::AccountIndexMap;
    register_wallet_for_password_change(ctx).await;
    wallet_database::repositories::config::ConfigRepo::upsert(
        "wallet_tree_strategy",
        r#"{"wallet_tree_strategy":"v2"}"#,
        Some(0),
        &ctx.core_pool().unwrap(),
    )
    .await
    .unwrap();
    ctx.get_global_dirs().get_root_dir(WALLET).unwrap();
    let subs = ctx.get_global_dirs().get_subs_dir(WALLET).unwrap();
    let account = AccountIndexMap::from_account_id(0).unwrap();
    let other = AccountIndexMap::from_account_id(1).unwrap();
    ModernWalletTree::default()
        .io()
        .store_subkeys_bulk(
            vec![
                BulkSubkey::new(
                    account.clone(),
                    "fixture-eth",
                    "eth",
                    "m/0",
                    b"synthetic eth key".to_vec(),
                ),
                BulkSubkey::new(
                    account,
                    "fixture-sol",
                    "sol",
                    "m/1",
                    b"synthetic sol key".to_vec(),
                ),
                BulkSubkey::new(
                    other,
                    "fixture-other",
                    "eth",
                    "m/2",
                    b"synthetic other key".to_vec(),
                ),
            ],
            &subs,
            PASSWORD,
            wallet_crypto::KdfAlgorithm::Scrypt,
        )
        .unwrap();
    subs
}

#[tokio::test]
async fn derived_password_change_preserves_payload_index_and_other_accounts() {
    use crate::domain::account::AccountDomain;
    use wallet_crypto::EncryptedJsonDecryptor;
    let dir = directory();
    let native = Arc::new(Native::default());
    let ctx = context(dir.path(), native.clone()).await;
    let subs = derived_password_fixture(ctx).await;
    let path = subs.join("key0.keystore");
    let index = std::fs::read(subs.join("derived_meta.json")).unwrap();
    let other = std::fs::read(subs.join("key1.keystore")).unwrap();
    let original = std::fs::read_to_string(&path).unwrap();
    let plain =
        wallet_crypto::KeystoreJsonDecryptor.decrypt(PASSWORD.as_bytes(), &original).unwrap();
    let binding = ProtectionContext::new(WALLET, "derived:0").unwrap();
    let codec = KeystoreProtection::new(ctx.device_protection().clone());
    codec
        .migrate_file(&path, &binding, PASSWORD, ReadPolicy::LegacyMigrationAllowed)
        .await
        .unwrap();
    let before = std::fs::read(&path).unwrap();
    let account = wallet_utils::address::AccountIndexMap::from_account_id(0).unwrap();
    let wrong = AccountDomain::set_account_password(
        ctx,
        WALLET,
        &account,
        "wrong-fixture-password",
        "new-fixture-password",
    )
    .await;
    assert!(matches!(
        wrong,
        Err(ServiceError::DeviceProtection(KeystoreProtectionError::PasswordDecryption))
    ));
    assert!(std::fs::read(&path).unwrap() == before);
    native.fail_protect.store(true, Ordering::SeqCst);
    let failed = AccountDomain::set_account_password(
        ctx,
        WALLET,
        &account,
        PASSWORD,
        "new-fixture-password",
    )
    .await;
    assert!(matches!(
        failed,
        Err(ServiceError::DeviceProtection(KeystoreProtectionError::Protection(
            ProtectionError::PlatformFailure
        )))
    ));
    assert!(std::fs::read(&path).unwrap() == before);
    assert!(std::fs::read(subs.join("derived_meta.json")).unwrap() == index);
    native.fail_protect.store(false, Ordering::SeqCst);
    AccountDomain::set_account_password(ctx, WALLET, &account, PASSWORD, "new-fixture-password")
        .await
        .unwrap();
    assert!(
        codec.load_file(&path, &binding, "new-fixture-password").await.unwrap().as_slice() == plain
    );
    assert!(matches!(
        codec.load_file(&path, &binding, PASSWORD).await,
        Err(KeystoreProtectionError::PasswordDecryption)
    ));
    assert!(std::fs::read(&path).unwrap().starts_with(b"WDK\x01"));
    assert!(std::fs::read(subs.join("derived_meta.json")).unwrap() == index);
    assert!(std::fs::read(subs.join("key1.keystore")).unwrap() == other);
    assert_eq!(std::fs::read_dir(&subs).unwrap().count(), 3);
}

#[tokio::test]
async fn completed_derived_password_change_rejects_legacy_rollback() {
    use crate::domain::account::AccountDomain;
    let dir = directory();
    let ctx = context(dir.path(), Arc::new(Native::default())).await;
    let subs = derived_password_fixture(ctx).await;
    let path = subs.join("key0.keystore");
    let original = std::fs::read(&path).unwrap();
    let index = std::fs::read(subs.join("derived_meta.json")).unwrap();
    let binding = ProtectionContext::new(WALLET, "derived:0").unwrap();
    KeystoreProtection::new(ctx.device_protection().clone())
        .migrate_files(
            &ctx.core_pool().unwrap(),
            &[MigrationFile { path: &path, context: &binding }],
            PASSWORD,
            ReadPolicy::LegacyMigrationAllowed,
        )
        .await
        .unwrap();
    std::fs::write(&path, &original).unwrap();
    let account = wallet_utils::address::AccountIndexMap::from_account_id(0).unwrap();
    let failed = AccountDomain::set_account_password(
        ctx,
        WALLET,
        &account,
        PASSWORD,
        "new-fixture-password",
    )
    .await;
    assert!(matches!(
        failed,
        Err(ServiceError::DeviceProtection(KeystoreProtectionError::LegacyForbidden))
    ));
    assert!(std::fs::read(&path).unwrap() == original);
    assert!(std::fs::read(subs.join("derived_meta.json")).unwrap() == index);
}

#[tokio::test]
async fn password_only_derived_change_remains_compatible() {
    use crate::domain::account::AccountDomain;
    use wallet_crypto::EncryptedJsonDecryptor;
    let dir = directory();
    let native = Arc::new(Native::default());
    let ctx = context(dir.path(), native.clone()).await;
    let subs = derived_password_fixture(ctx).await;
    let path = subs.join("key0.keystore");
    let original = std::fs::read_to_string(&path).unwrap();
    let plain =
        wallet_crypto::KeystoreJsonDecryptor.decrypt(PASSWORD.as_bytes(), &original).unwrap();
    let account = wallet_utils::address::AccountIndexMap::from_account_id(0).unwrap();
    AccountDomain::set_account_password(ctx, WALLET, &account, PASSWORD, "new-fixture-password")
        .await
        .unwrap();
    let updated = std::fs::read_to_string(&path).unwrap();
    assert!(
        wallet_crypto::KeystoreJsonDecryptor.decrypt(b"new-fixture-password", &updated).unwrap()
            == plain
    );
    assert!(wallet_crypto::KeystoreJsonDecryptor.decrypt(PASSWORD.as_bytes(), &updated).is_err());
    assert_eq!(native.opens.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn private_key_export_propagates_device_failure_before_conversion() {
    let dir = directory();
    let native = Arc::new(Native::default());
    let ctx = context(dir.path(), native.clone()).await;
    let subs = derived_password_fixture(ctx).await;
    let path = subs.join("key0.keystore");
    let binding = ProtectionContext::new(WALLET, "derived:0").unwrap();
    KeystoreProtection::new(ctx.device_protection().clone())
        .migrate_file(&path, &binding, PASSWORD, ReadPolicy::LegacyMigrationAllowed)
        .await
        .unwrap();
    let before = std::fs::read(&path).unwrap();
    native.locked.store(true, Ordering::SeqCst);
    let account = wallet_utils::address::AccountIndexMap::from_account_id(0).unwrap();
    let failed =
        crate::domain::account::open_accounts_pk_with_password(ctx, &account, WALLET, PASSWORD)
            .await;
    assert!(matches!(
        failed,
        Err(ServiceError::DeviceProtection(KeystoreProtectionError::Protection(
            ProtectionError::Locked
        )))
    ));
    assert!(std::fs::read(&path).unwrap() == before);
}

#[tokio::test]
async fn derived_private_key_reads_match_exact_metadata_and_preserve_files() {
    let dir = directory();
    let native = Arc::new(Native::default());
    let ctx = context(dir.path(), native.clone()).await;
    let subs = derived_password_fixture(ctx).await;
    let path = subs.join("key0.keystore");
    let index = std::fs::read(subs.join("derived_meta.json")).unwrap();
    let account = wallet_utils::address::AccountIndexMap::from_account_id(0).unwrap();
    let expected = wallet_tree::KeyMeta {
        chain_code: "eth".into(),
        address: "fixture-eth".into(),
        derivation_path: "m/0".into(),
    };
    let legacy =
        WalletApplication::load_account_data(ctx, WALLET, &account, PASSWORD).await.unwrap();
    assert_eq!(legacy.len(), 2);
    assert!(
        WalletApplication::load_sub_key(ctx, WALLET, &account, &expected, PASSWORD).await.unwrap()
            == b"synthetic eth key"
    );
    assert_eq!(native.opens.load(Ordering::SeqCst), 0);
    let binding = ProtectionContext::new(WALLET, "derived:0").unwrap();
    KeystoreProtection::new(ctx.device_protection().clone())
        .migrate_file(&path, &binding, PASSWORD, ReadPolicy::LegacyMigrationAllowed)
        .await
        .unwrap();
    let before = std::fs::read(&path).unwrap();
    let loaded =
        WalletApplication::load_account_data(ctx, WALLET, &account, PASSWORD).await.unwrap();
    assert!(loaded.into_inner() == legacy.into_inner());
    assert!(
        WalletApplication::load_sub_key(ctx, WALLET, &account, &expected, PASSWORD).await.unwrap()
            == b"synthetic eth key"
    );
    let wrong_path =
        wallet_tree::KeyMeta { derivation_path: "m/unknown".into(), ..expected.clone() };
    assert!(matches!(
        WalletApplication::load_sub_key(ctx, WALLET, &account, &wrong_path, PASSWORD).await,
        Err(ServiceError::Tree(wallet_tree::Error::PrivateKeyNotFound))
    ));
    let wrong_password =
        WalletApplication::load_sub_key(ctx, WALLET, &account, &expected, "wrong-fixture-password")
            .await;
    assert!(matches!(
        wrong_password,
        Err(ServiceError::DeviceProtection(KeystoreProtectionError::PasswordDecryption))
    ));
    native.locked.store(true, Ordering::SeqCst);
    assert!(matches!(
        WalletApplication::load_sub_key(ctx, WALLET, &account, &expected, PASSWORD).await,
        Err(ServiceError::DeviceProtection(KeystoreProtectionError::Protection(
            ProtectionError::Locked
        )))
    ));
    assert!(std::fs::read(&path).unwrap() == before);
    assert!(std::fs::read(subs.join("derived_meta.json")).unwrap() == index);
}

#[tokio::test]
async fn malformed_protected_account_payload_is_rejected_without_rewrite() {
    let dir = directory();
    let ctx = context(dir.path(), Arc::new(Native::default())).await;
    let subs = derived_password_fixture(ctx).await;
    let path = subs.join("key0.keystore");
    let binding = ProtectionContext::new(WALLET, "derived:0").unwrap();
    let codec = KeystoreProtection::new(ctx.device_protection().clone());
    for payload in [
        br#"{"invalid-key":[1,2,3]}"#.as_slice(),
        br#"{"eth::address::m/0":[1],"eth::address::m/0":[2]}"#.as_slice(),
        br#"{"eth::addr:1::m/0":[1],"eth::addr\\c1::m/0":[2]}"#.as_slice(),
    ] {
        codec
            .store_file(
                &path,
                &binding,
                zeroize::Zeroizing::new(payload.to_vec()),
                PASSWORD,
                wallet_crypto::KdfAlgorithm::Scrypt,
            )
            .await
            .unwrap();
        let before = std::fs::read(&path).unwrap();
        let account = wallet_utils::address::AccountIndexMap::from_account_id(0).unwrap();
        assert!(matches!(
            WalletApplication::load_account_data(ctx, WALLET, &account, PASSWORD).await,
            Err(ServiceError::DeviceProtection(KeystoreProtectionError::InvalidFormat))
        ));
        assert!(std::fs::read(&path).unwrap() == before);
    }
}

#[tokio::test]
async fn root_store_preserves_existing_protection_and_files_on_failure() {
    let dir = directory();
    let native = Arc::new(Native::default());
    let ctx = context(dir.path(), native.clone()).await;
    register_wallet_for_password_change(ctx).await;
    let root = ctx.get_global_dirs().get_root_dir(WALLET).unwrap();
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
    let path = root.join("root.keystore");
    let before = std::fs::read(&path).unwrap();
    let sibling = root.join("fixture-public-marker");
    std::fs::write(&sibling, b"keep").unwrap();
    native.fail_protect.store(true, Ordering::SeqCst);
    let failed = WalletApplication::store_root(
        ctx,
        WALLET,
        RootData::new(PHRASE, b"synthetic seed"),
        PASSWORD,
    )
    .await;
    assert!(matches!(
        failed,
        Err(ServiceError::DeviceProtection(KeystoreProtectionError::Protection(
            ProtectionError::PlatformFailure
        )))
    ));
    assert!(std::fs::read(&path).unwrap() == before);
    assert!(std::fs::read(&sibling).unwrap() == b"keep");
    assert!(WalletService::new(ctx).get_phrase(WALLET, PASSWORD).await.unwrap().phrase == PHRASE);
    native.fail_protect.store(false, Ordering::SeqCst);
    WalletApplication::store_root(ctx, WALLET, RootData::new(PHRASE, b"synthetic seed"), PASSWORD)
        .await
        .unwrap();
    assert!(std::fs::read(&path).unwrap().starts_with(b"WDK\x01"));
    assert!(WalletService::new(ctx).get_phrase(WALLET, PASSWORD).await.unwrap().phrase == PHRASE);
    assert!(std::fs::read(&sibling).unwrap() == b"keep");
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
}

#[tokio::test]
async fn root_store_retains_legacy_creation_until_activation() {
    let dir = directory();
    let native = Arc::new(Native::default());
    let ctx = context(dir.path(), native.clone()).await;
    register_wallet_for_password_change(ctx).await;
    WalletApplication::store_root(ctx, WALLET, RootData::new(PHRASE, b"synthetic seed"), PASSWORD)
        .await
        .unwrap();
    assert!(WalletService::new(ctx).get_phrase(WALLET, PASSWORD).await.unwrap().phrase == PHRASE);
    assert_eq!(native.opens.load(Ordering::SeqCst), 0);
    assert!(
        ctx.get_global_dirs().get_root_dir(WALLET).unwrap().join("fixturewallet-seed").exists()
    );
}
