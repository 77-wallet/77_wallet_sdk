use wallet_database::{
    entities::{
        config::config_key::{KEYSTORE_KDF_ALGORITHM, WALLET_TREE_STRATEGY},
        device::DeviceEntity,
        wallet::WalletEntity,
    },
    repositories::{
        ResourcesRepo, account::AccountRepoTrait, api_wallet::wallet::ApiWalletRepo,
        wallet::WalletRepoTrait,
    },
};
use wallet_tree::{KdfAlgorithm, WalletTreeStrategy, api::KeystoreApi};
use wallet_types::chain::{
    address::r#type::{
        AddressType, BTC_ADDRESS_TYPES, DOG_ADDRESS_TYPES, LTC_ADDRESS_TYPES, TON_ADDRESS_TYPES,
    },
    chain::ChainCode,
};

use super::app::config::ConfigDomain;

const DEFAULT_SALT: &str = "salt";

pub struct WalletDomain {}
impl Default for WalletDomain {
    fn default() -> Self {
        Self::new()
    }
}
impl WalletDomain {
    pub fn new() -> Self {
        Self {}
    }

    pub(crate) fn encrypt_password(
        password: &str,
        salt: &str,
    ) -> Result<String, crate::error::service::ServiceError> {
        let encrypted_password = wallet_utils::pbkdf2_string(
            password,
            &format!("{}{}", salt, DEFAULT_SALT),
            100000,
            32,
        )?;
        Ok(encrypted_password)
    }

    pub(crate) async fn validate_password(
        password: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let dirs = crate::context::CONTEXT.get().unwrap().get_global_dirs();

        if WalletEntity::wallet_latest(&*pool).await?.is_none() {
            KeystoreApi::remove_verify_file(&dirs.root_dir)?;
        };

        let Some(device) = DeviceEntity::get_device_info(&*pool).await? else {
            return Err(crate::error::business::BusinessError::Device(
                crate::error::business::device::DeviceError::Uninitialized,
            )
            .into());
        };

        if device.password.is_none() {
            let wallet_tree_strategy = ConfigDomain::get_wallet_tree_strategy().await?;
            let wallet_tree = wallet_tree_strategy.get_wallet_tree(&dirs.wallet_dir)?;

            let file_name = "verify";
            let file_path = dirs.root_dir.join(file_name);
            if wallet_utils::file_func::exists(&file_path)? {
                tracing::info!("load verify file");
                if KeystoreApi::load_verify_file(&*wallet_tree, &dirs.root_dir, password).is_err() {
                    return Err(crate::error::business::BusinessError::Wallet(
                        crate::error::business::wallet::WalletError::PasswordIncorrect,
                    )
                    .into());
                }
                tracing::info!("verify file exists");
            } else {
                tracing::info!("verify file not exists");
                KeystoreApi::store_verify_file(&*wallet_tree, &dirs.root_dir, password)?;
            }
        } else {
            WalletDomain::upgrade_algorithm(password).await?;
        }

        Ok(())
    }

    pub(crate) async fn upgrade_algorithm(
        password: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let dirs = crate::context::CONTEXT.get().unwrap().get_global_dirs();

        let mut legacy_wallet_tree = WalletTreeStrategy::V1.get_wallet_tree(&dirs.wallet_dir)?;
        // tracing::info!("legacy_wallet_tree: {:?}", legacy_wallet_tree);
        #[derive(Debug, Clone, Eq, PartialEq, Hash)]
        pub struct AccountInfo {
            pub wallet_address: String,
            pub address: String,
            pub chain_code: String,
            pub derivation_path: String,
        }

        let modern_wallet_tree = WalletTreeStrategy::V2.get_wallet_tree(&dirs.wallet_dir)?;
        // 将子密钥全部读取出来
        let mut account_data = std::collections::HashMap::<AccountInfo, Vec<u8>>::new();
        tracing::info!("legacy_wallet_tree: {:#?}", legacy_wallet_tree);

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

        // let wallet_tr
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

            // subkeys.push(subkey);
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
        ConfigDomain::set_config(KEYSTORE_KDF_ALGORITHM, &keystore_kdf_algorithm.to_json_str()?)
            .await?;
        ConfigDomain::set_config(WALLET_TREE_STRATEGY, &wallet_tree_strategy.to_json_str()?)
            .await?;

        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

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
        DeviceEntity::update_password(&*pool, None).await?;

        Ok(())
    }

    pub(crate) async fn get_seed(
        dirs: &crate::dirs::Dirs,
        wallet_address: &str,
        wallet_password: &str,
    ) -> Result<Vec<u8>, crate::error::service::ServiceError> {
        let root_dir = dirs.get_root_dir(wallet_address)?;
        let wallet_tree_strategy = ConfigDomain::get_wallet_tree_strategy().await?;
        let wallet_tree = wallet_tree_strategy.get_wallet_tree(&dirs.wallet_dir)?;

        Ok(wallet_tree::api::KeystoreApi::load_seed(
            &*wallet_tree,
            &root_dir,
            wallet_address,
            wallet_password,
        )?)
    }

    pub(crate) async fn restart_existing_wallet(
        &self,
        repo: &mut ResourcesRepo,
        address: &str,
    ) -> Result<std::collections::HashSet<u32>, crate::error::service::ServiceError> {
        // 查询钱包状态并处理重启逻辑
        let mut account_ids = std::collections::HashSet::new();
        if let Some(wallet) = WalletRepoTrait::detail_all_status(repo, address).await? {
            if wallet.status == 2 {
                WalletRepoTrait::restart(repo, &[address]).await?;
                for account in AccountRepoTrait::restart(repo, address).await? {
                    account_ids.insert(account.account_id);
                }
            }
        }
        if account_ids.is_empty() {
            account_ids.insert(1);
        }
        Ok(account_ids)
    }

    pub fn address_type_by_chain(chian: ChainCode) -> Vec<AddressType> {
        match chian {
            ChainCode::Bitcoin => BTC_ADDRESS_TYPES.to_vec(),
            ChainCode::Dogcoin => DOG_ADDRESS_TYPES.to_vec(),
            ChainCode::Litecoin => LTC_ADDRESS_TYPES.to_vec(),
            ChainCode::Ton => TON_ADDRESS_TYPES.to_vec(),
            _ => vec![AddressType::Other],
        }
    }

    pub(crate) async fn check_api_wallet_exist(
        address: &str,
    ) -> Result<bool, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let res = ApiWalletRepo::find_by_address(&pool, address).await?;
        Ok(!res.is_none())
    }
}

struct SubsKeyInfo {
    pub wallet_address: String,
    pub address: String,
    pub chain_code: String,
}
