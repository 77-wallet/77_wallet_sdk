mod derived_store;

use std::collections::HashSet;

use wallet_database::{
    CoreDbPool,
    entities::config::config_key::{KEYSTORE_KDF_ALGORITHM, WALLET_TREE_STRATEGY},
    repositories::{
        account::AccountRepo,
        api_wallet::wallet::ApiWalletRepo,
        device::DeviceRepo,
        device_protection::{MigrationRepo, MigrationState},
        wallet::WalletRepo,
    },
};
use wallet_tree::{KdfAlgorithm, WalletTreeStrategy};

use crate::{
    context::Context,
    device_protection::{
        ProtectionContext,
        keystore_format::{KeystoreProtection, KeystoreProtectionError, has_device_header},
        migration::record_scope,
    },
    domain::{api_wallet::wallet::ApiWalletDomain, app::config::ConfigDomain},
    error::service::ServiceError,
    infrastructure::unlock_session,
};

pub struct WalletApplication;

impl WalletApplication {
    pub(crate) async fn load_root(
        ctx: &Context,
        wallet_address: &str,
        password: &str,
    ) -> Result<wallet_tree::file_ops::RootData, ServiceError> {
        let dirs = ctx.get_global_dirs();
        let root_dir = dirs.get_root_dir(wallet_address)?;
        let binding = ProtectionContext::new(wallet_address, "root-keystore")
            .map_err(KeystoreProtectionError::from)?;
        if Self::record_requires_device(ctx, &binding, &root_dir.join("root.keystore")).await? {
            return Ok(KeystoreProtection::new(ctx.device_protection().clone())
                .load_root(&root_dir, wallet_address, password)
                .await?);
        }

        let strategy = ConfigDomain::get_wallet_tree_strategy(ctx).await?;
        let wallet_tree = strategy.get_wallet_tree(&dirs.wallet_dir)?;
        Ok(wallet_tree::api::KeystoreApi::load_root(
            &*wallet_tree,
            &root_dir,
            wallet_address,
            password,
        )?)
    }

    pub(crate) async fn store_root(
        ctx: &Context,
        wallet_address: &str,
        root: wallet_tree::file_ops::RootData,
        password: &str,
    ) -> Result<(), ServiceError> {
        let dirs = ctx.get_global_dirs();
        let root_dir = dirs.get_root_dir(wallet_address)?;
        let binding = ProtectionContext::new(wallet_address, "root-keystore")
            .map_err(KeystoreProtectionError::from)?;
        let algorithm = ConfigDomain::get_keystore_kdf_algorithm(ctx).await?;
        if Self::record_requires_device(ctx, &binding, &root_dir.join("root.keystore")).await? {
            return Ok(KeystoreProtection::new(ctx.device_protection().clone())
                .store_root(&root_dir, wallet_address, root, password, algorithm)
                .await?);
        }

        // Preserve the existing unprotected creation path until provisioning/rollout is wired.
        let strategy = ConfigDomain::get_wallet_tree_strategy(ctx).await?;
        let wallet_tree = strategy.get_wallet_tree(&dirs.wallet_dir)?;
        wallet_utils::file_func::recreate_dir_all(&root_dir)?;
        Ok(wallet_tree::api::KeystoreApi::initialize_root_keystore(
            wallet_tree,
            wallet_address,
            root,
            &root_dir,
            password,
            algorithm,
        )?)
    }

    // Shared policy for existing records. Not permission to provision or migrate.
    async fn record_requires_device(
        ctx: &Context,
        binding: &ProtectionContext,
        path: &std::path::Path,
    ) -> Result<bool, ServiceError> {
        let state = MigrationRepo::get(&record_scope(binding), &ctx.core_pool()?)
            .await
            .map_err(|_| ServiceError::Context("Cannot read keystore migration progress".into()))?;
        let protected = has_device_header(path).await?;
        // Completion forbids legacy fallback, even when the protected file is missing.
        Ok(protected || state == Some(MigrationState::DeviceRequired))
    }

    pub(crate) async fn change_root_password(
        ctx: &Context,
        wallet_address: &str,
        old_password: &str,
        new_password: &str,
    ) -> Result<(), ServiceError> {
        let dirs = ctx.get_global_dirs();
        let root_dir = dirs.get_root_dir(wallet_address)?;
        let algorithm = ConfigDomain::get_keystore_kdf_algorithm(ctx).await?;
        let binding = ProtectionContext::new(wallet_address, "root-keystore")
            .map_err(KeystoreProtectionError::from)?;
        if Self::record_requires_device(ctx, &binding, &root_dir.join("root.keystore")).await? {
            return Ok(KeystoreProtection::new(ctx.device_protection().clone())
                .change_file_password(
                    &root_dir.join("root.keystore"),
                    &binding,
                    old_password,
                    new_password,
                    algorithm,
                )
                .await?);
        }

        let strategy = ConfigDomain::get_wallet_tree_strategy(ctx).await?;
        let wallet_tree = strategy.get_wallet_tree(&dirs.wallet_dir)?;
        wallet_tree::api::KeystoreApi::update_root_password(
            root_dir,
            wallet_tree,
            wallet_address,
            old_password,
            new_password,
            algorithm,
        )
        .map_err(|error| crate::error::system::SystemError::Service(error.to_string()))?;
        Ok(())
    }

    pub(crate) async fn change_account_password(
        ctx: &Context,
        wallet_address: &str,
        account_index_map: &wallet_utils::address::AccountIndexMap,
        old_password: &str,
        new_password: &str,
    ) -> Result<(), ServiceError> {
        let _guard = derived_store::lock().await;
        let dirs = ctx.get_global_dirs();
        let subs_dir = dirs.get_subs_dir(wallet_address)?;
        let path = subs_dir.join(format!("key{}.keystore", account_index_map.account_id));
        let binding = ProtectionContext::new(
            wallet_address,
            format!("derived:{}", account_index_map.account_id),
        )
        .map_err(KeystoreProtectionError::from)?;
        let algorithm = ConfigDomain::get_keystore_kdf_algorithm(ctx).await?;
        if Self::record_requires_device(ctx, &binding, &path).await? {
            return Ok(KeystoreProtection::new(ctx.device_protection().clone())
                .change_file_password(&path, &binding, old_password, new_password, algorithm)
                .await?);
        }

        let strategy = ConfigDomain::get_wallet_tree_strategy(ctx).await?;
        let wallet_tree = strategy.get_wallet_tree(&dirs.wallet_dir)?;
        wallet_tree::api::KeystoreApi::update_account_password(
            wallet_tree,
            &subs_dir,
            account_index_map,
            old_password,
            new_password,
            algorithm,
        )
        .map_err(|error| crate::error::system::SystemError::Service(error.to_string()))?;
        Ok(())
    }

    pub(crate) async fn load_account_data(
        ctx: &Context,
        wallet_address: &str,
        account_index_map: &wallet_utils::address::AccountIndexMap,
        password: &str,
    ) -> Result<wallet_tree::file_ops::AccountData, ServiceError> {
        let dirs = ctx.get_global_dirs();
        let subs_dir = dirs.get_subs_dir(wallet_address)?;
        let path = subs_dir.join(format!("key{}.keystore", account_index_map.account_id));
        let binding = ProtectionContext::new(
            wallet_address,
            format!("derived:{}", account_index_map.account_id),
        )
        .map_err(KeystoreProtectionError::from)?;
        if Self::record_requires_device(ctx, &binding, &path).await? {
            return Self::load_protected_account(ctx, &binding, &path, password).await;
        }
        let strategy = ConfigDomain::get_wallet_tree_strategy(ctx).await?;
        let wallet_tree = strategy.get_wallet_tree(&dirs.wallet_dir)?;
        Ok(wallet_tree::api::KeystoreApi::load_account_pk(
            &*wallet_tree,
            account_index_map,
            &subs_dir,
            password,
        )?)
    }

    pub(crate) async fn load_sub_key(
        ctx: &Context,
        wallet_address: &str,
        account_index_map: &wallet_utils::address::AccountIndexMap,
        expected: &wallet_tree::KeyMeta,
        password: &str,
    ) -> Result<Vec<u8>, ServiceError> {
        let dirs = ctx.get_global_dirs();
        let subs_dir = dirs.get_subs_dir(wallet_address)?;
        let path = subs_dir.join(format!("key{}.keystore", account_index_map.account_id));
        let binding = ProtectionContext::new(
            wallet_address,
            format!("derived:{}", account_index_map.account_id),
        )
        .map_err(KeystoreProtectionError::from)?;
        if Self::record_requires_device(ctx, &binding, &path).await? {
            let mut keys = Self::load_protected_account(ctx, &binding, &path, password).await?;
            return keys
                .remove(expected)
                .ok_or_else(|| wallet_tree::Error::PrivateKeyNotFound.into());
        }
        let strategy = ConfigDomain::get_wallet_tree_strategy(ctx).await?;
        let wallet_tree = strategy.get_wallet_tree(&dirs.wallet_dir)?;
        Ok(wallet_tree::api::KeystoreApi::load_sub_pk(
            &*wallet_tree,
            Some(account_index_map),
            &subs_dir,
            &expected.address,
            &expected.chain_code,
            &expected.derivation_path,
            password,
        )?)
    }

    async fn load_protected_account(
        ctx: &Context,
        binding: &ProtectionContext,
        path: &std::path::Path,
        password: &str,
    ) -> Result<wallet_tree::file_ops::AccountData, ServiceError> {
        let plain = KeystoreProtection::new(ctx.device_protection().clone())
            .load_file(path, binding, password)
            .await?;
        Ok(decode_account_data(&plain)?)
    }

    pub(crate) async fn validate_password(
        ctx: &'static Context,
        password: &str,
    ) -> Result<(), ServiceError> {
        let core_pool = ctx.core_pool()?;
        let api_pool = ctx.api_wallet_pool()?;

        if unlock_session::wallet_unlock_token_is_active(password).await? {
            return Ok(());
        }

        let sn = ctx.get_sn();
        let Some(device) = DeviceRepo::get_device_info(core_pool.clone(), sn).await? else {
            return Err(crate::error::business::BusinessError::Device(
                crate::error::business::device::DeviceError::Uninitialized,
            )
            .into());
        };

        if device.password.is_some() {
            Self::upgrade_algorithm(ctx, password).await?;
            return Ok(());
        }

        let has_wallets = WalletRepo::wallet_latest(core_pool.clone()).await?.is_some()
            || ApiWalletRepo::wallet_latest(&api_pool).await?.is_some();
        if !has_wallets {
            return Ok(());
        }

        match &device.password_proof {
            Some(proof) => {
                if let Err(_) = Self::decrypt_password_proof(proof, password).await {
                    tracing::info!("password validation failed");
                    return Err(crate::error::business::BusinessError::Wallet(
                        crate::error::business::wallet::WalletError::PasswordIncorrect,
                    )
                    .into());
                }
            }
            None => {
                tracing::info!("password_proof is None, trying fallback validation");
                if Self::try_decrypt_wallet_db(ctx, password).await? {
                    let proof = Self::generate_password_proof(ctx, password).await?;
                    DeviceRepo::update_password_proof(core_pool.clone(), sn, Some(&proof)).await?;
                    tracing::info!("password_proof generated and stored");
                } else {
                    tracing::info!("password validation failed");
                    return Err(crate::error::business::BusinessError::Wallet(
                        crate::error::business::wallet::WalletError::PasswordIncorrect,
                    )
                    .into());
                }
            }
        }

        Ok(())
    }

    pub(crate) async fn upgrade_algorithm(
        ctx: &'static Context,
        password: &str,
    ) -> Result<(), ServiceError> {
        let dirs = ctx.get_global_dirs();

        let mut legacy_wallet_tree = WalletTreeStrategy::V1.get_wallet_tree(&dirs.wallet_dir)?;
        #[derive(Debug, Clone, Eq, PartialEq, Hash)]
        struct AccountInfo {
            wallet_address: String,
            address: String,
            chain_code: String,
            derivation_path: String,
        }

        let modern_wallet_tree = WalletTreeStrategy::V2.get_wallet_tree(&dirs.wallet_dir)?;
        let mut account_data = std::collections::HashMap::<AccountInfo, Vec<u8>>::new();
        let legacy_wallet_count = legacy_wallet_tree.iter().count();
        tracing::info!(legacy_wallet_count, "legacy wallet tree loaded for algorithm upgrade");

        let mut delete_roots = Vec::new();
        let mut delete_subs = Vec::new();
        for (k, v) in legacy_wallet_tree.iter() {
            let root_dir = dirs.get_root_dir(k)?;
            let subs_dir = dirs.get_subs_dir(k)?;
            match legacy_wallet_tree.io().load_root(k, &root_dir, password) {
                Ok(root_data) => {
                    if let Err(e) = modern_wallet_tree.io().store_root(
                        k,
                        root_data.seed(),
                        root_data.phrase(),
                        &root_dir,
                        password,
                        wallet_tree::KdfAlgorithm::Argon2id,
                    ) {
                        tracing::error!("store_root error: {:?}", e);
                    };
                }
                Err(e) => {
                    tracing::error!("load_root error: {:?}", e);
                }
            };

            for account in v.get_accounts().into_iter() {
                let address = account.get_address();
                let chain_code = account.chain_code().unwrap_or_default();
                let derivation_path = account.derivation_path().unwrap_or_default();

                let pk = legacy_wallet_tree.io().load_subkey(
                    None,
                    address,
                    &chain_code,
                    &derivation_path,
                    &subs_dir,
                    password,
                )?;

                account_data.insert(
                    AccountInfo {
                        wallet_address: k.to_string(),
                        address: address.to_string(),
                        chain_code,
                        derivation_path,
                    },
                    pk,
                );
            }
            delete_roots.push(k);
        }

        modern_wallet_tree.io().store(
            "verify",
            &"data",
            &dirs.root_dir,
            password,
            wallet_tree::KdfAlgorithm::Argon2id,
        )?;

        let mut subkeys = std::collections::HashMap::new();
        for (info, d) in account_data {
            let hd_path = wallet_chain_instance::derivation_path::get_account_hd_path_from_path(
                &info.derivation_path,
            )?;
            let account_index_map =
                wallet_utils::address::AccountIndexMap::from_account_id(hd_path.get_account_id()?)?;

            let subkey = wallet_tree::file_ops::BulkSubkey::new(
                account_index_map.clone(),
                &info.address,
                &info.chain_code,
                &info.derivation_path,
                d,
            );

            subkeys.entry(info.wallet_address.clone()).or_insert_with(Vec::new).push(subkey);
            delete_subs.push(SubsKeyInfo {
                wallet_address: info.wallet_address,
                address: info.address,
                chain_code: info.chain_code,
            });
        }

        for (wallet_address, subkey) in subkeys {
            let subs_dir = dirs.get_subs_dir(&wallet_address)?;
            modern_wallet_tree.io().store_subkeys_bulk(
                subkey,
                &subs_dir,
                password,
                wallet_tree::KdfAlgorithm::Argon2id,
            )?;
        }

        let keystore_kdf_algorithm = wallet_database::entities::config::KeystoreKdfAlgorithm {
            keystore_kdf_algorithm: KdfAlgorithm::Argon2id,
        };
        let wallet_tree_strategy = wallet_database::entities::config::WalletTreeStrategy {
            wallet_tree_strategy: wallet_tree::WalletTreeStrategy::V2,
        };
        crate::domain::app::config::ConfigDomain::set_config(
            ctx,
            KEYSTORE_KDF_ALGORITHM,
            &keystore_kdf_algorithm.to_json_str()?,
        )
        .await?;
        crate::domain::app::config::ConfigDomain::set_config(
            ctx,
            WALLET_TREE_STRATEGY,
            &wallet_tree_strategy.to_json_str()?,
        )
        .await?;

        for k in delete_roots {
            let root_dir = dirs.get_root_dir(k)?;
            legacy_wallet_tree.io().delete_root(k, &root_dir)?;
        }

        for k in delete_subs {
            let subs_dir = dirs.get_subs_dir(&k.wallet_address)?;
            legacy_wallet_tree.delete_subkey(
                &k.wallet_address,
                &k.address,
                k.chain_code.as_str(),
                &subs_dir,
                password,
            )?;
        }

        let sn = ctx.get_sn();
        let core_pool = ctx.core_pool()?;
        DeviceRepo::update_password(core_pool, sn, None).await?;

        Ok(())
    }

    pub(crate) async fn get_seed(
        ctx: &'static Context,
        wallet_address: &str,
        wallet_password: &str,
    ) -> Result<Vec<u8>, ServiceError> {
        Ok(Self::load_root(ctx, wallet_address, wallet_password).await?.seed().to_vec())
    }

    pub(crate) async fn restart_existing_wallet(
        core_pool: CoreDbPool,
        address: &str,
    ) -> Result<HashSet<u32>, ServiceError> {
        let mut account_ids = HashSet::new();
        if let Some(wallet) = WalletRepo::detail_all_status(core_pool.clone(), address).await? {
            if wallet.status == 2 {
                WalletRepo::restart(core_pool.clone(), &[address]).await?;
                for account in AccountRepo::restart(core_pool.clone(), address).await? {
                    account_ids.insert(account.account_id);
                }
            }
        }
        if account_ids.is_empty() {
            account_ids.insert(1);
        }
        Ok(account_ids)
    }

    pub(crate) async fn check_api_wallet_exist(
        ctx: &'static Context,
        address: &str,
    ) -> Result<bool, ServiceError> {
        let pool = ctx.api_wallet_pool()?;
        let res = ApiWalletRepo::find_by_address(&pool, address).await?;
        Ok(res.is_some())
    }

    pub(crate) async fn generate_password_proof(
        ctx: &'static Context,
        password: &str,
    ) -> Result<String, ServiceError> {
        const PROOF_STRING: &str = "wallet-sdk-password-proof";

        let algorithm =
            crate::domain::app::config::ConfigDomain::get_keystore_kdf_algorithm(ctx).await?;
        let rng = rand::rngs::OsRng::default();

        let proof =
            ApiWalletDomain::encrypt_password_proof(algorithm, rng, password, PROOF_STRING).await?;

        Ok(proof)
    }

    async fn decrypt_password_proof(proof: &str, password: &str) -> Result<(), ServiceError> {
        const PROOF_STRING: &str = "wallet-sdk-password-proof";

        let decrypted = ApiWalletDomain::decrypt_password_proof(password, proof).await?;
        if decrypted == PROOF_STRING {
            Ok(())
        } else {
            Err(crate::error::business::BusinessError::Wallet(
                crate::error::business::wallet::WalletError::PasswordIncorrect,
            )
            .into())
        }
    }

    async fn try_decrypt_wallet_db(
        ctx: &'static Context,
        password: &str,
    ) -> Result<bool, ServiceError> {
        let core_pool = ctx.core_pool()?;
        let pool = ctx.api_wallet_pool()?;

        if let Some(wallet) = WalletRepo::wallet_latest(core_pool.clone()).await? {
            match Self::load_root(ctx, &wallet.address, password).await {
                Ok(_) => {
                    tracing::info!("standard wallet decryption succeeded");
                    return Ok(true);
                }
                // Preserve password fallback, but never hide a device or policy failure.
                Err(ServiceError::DeviceProtection(
                    KeystoreProtectionError::PasswordDecryption,
                ))
                | Err(ServiceError::Tree(_))
                | Err(ServiceError::Keystore(_)) => {}
                Err(error) => return Err(error),
            }
        } else {
            tracing::info!("no WalletRepo wallet_latest");
        }

        if let Some(wallet) = ApiWalletRepo::wallet_latest(&pool).await? {
            match ApiWalletDomain::decrypt_phrase(password, &wallet.phrase).await {
                Ok(_) => {
                    tracing::info!("API wallet phrase decryption succeeded");
                    return Ok(true);
                }
                Err(e) => {
                    tracing::warn!("API wallet seed decryption error {:?}", e,);
                }
            }
            match ApiWalletDomain::decrypt_seed(password, &wallet.seed).await {
                Ok(_) => {
                    tracing::info!("API wallet seed decryption succeeded");
                    return Ok(true);
                }
                Err(e) => {
                    tracing::warn!("API wallet seed decryption error {:?}", e);
                }
            }
        } else {
            tracing::info!("no ApiWalletRepo wallet_latest");
        }

        tracing::info!("all wallet decryption attempts failed");
        Ok(false)
    }
}

struct SubsKeyInfo {
    pub wallet_address: String,
    pub address: String,
    pub chain_code: String,
}

// Visit entries directly: a normal map deserializer discards duplicate JSON keys.
fn decode_account_data(
    plain: &[u8],
) -> Result<wallet_tree::file_ops::AccountData, KeystoreProtectionError> {
    use serde::de::{Error as _, MapAccess, Visitor};
    struct AccountVisitor;
    impl<'de> Visitor<'de> for AccountVisitor {
        type Value = wallet_tree::file_ops::AccountData;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("an account key map")
        }

        fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
            let mut keys = Self::Value::default();
            while let Some((encoded, key)) = map.next_entry::<String, Vec<u8>>()? {
                let meta = wallet_tree::KeyMeta::decode(&encoded)
                    .map_err(|_| M::Error::custom("invalid account metadata"))?;
                if keys.insert(meta, key).is_some() {
                    return Err(M::Error::custom("duplicate account metadata"));
                }
            }
            Ok(keys)
        }
    }
    let mut decoder = serde_json::Deserializer::from_slice(plain);
    let keys = serde::de::Deserializer::deserialize_map(&mut decoder, AccountVisitor)
        .map_err(|_| KeystoreProtectionError::InvalidFormat)?;
    decoder.end().map_err(|_| KeystoreProtectionError::InvalidFormat)?;
    Ok(keys)
}

#[cfg(test)]
#[path = "wallet_protection_tests.rs"]
mod protection_tests;
