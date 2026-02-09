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
use wallet_tree::{KdfAlgorithm, WalletTreeStrategy};
use wallet_types::chain::{
    address::r#type::{
        AddressType, BTC_ADDRESS_TYPES, DOG_ADDRESS_TYPES, LTC_ADDRESS_TYPES, TON_ADDRESS_TYPES,
    },
    chain::ChainCode,
};

use super::{api_wallet::wallet::ApiWalletDomain, app::config::ConfigDomain};

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
        let pool = crate::context::CONTEXT.get().unwrap().api_wallet_pool()?;

        let sn = crate::context::CONTEXT.get().unwrap().get_sn();
        let Some(device) = DeviceEntity::get_device_info(pool.as_ref(), sn).await? else {
            return Err(crate::error::business::BusinessError::Device(
                crate::error::business::device::DeviceError::Uninitialized,
            )
            .into());
        };

        // 如果是旧版本，device.password 不为空，执行升级逻辑
        if device.password.is_some() {
            WalletDomain::upgrade_algorithm(password).await?;
            return Ok(());
        }

        // 检查是否存在钱包数据
        let has_wallets = WalletEntity::wallet_latest(&*pool.into_inner()).await?.is_some()
            || ApiWalletRepo::wallet_latest(&pool).await?.is_some();

        // 如果没有钱包数据，不需要密码验证
        if !has_wallets {
            return Ok(());
        }

        // 使用 password_proof 验证密码
        match &device.password_proof {
            Some(proof) => {
                // 尝试解密 password_proof
                if let Err(_) = Self::decrypt_password_proof(proof, password).await {
                    tracing::info!("password validation failed");
                    return Err(crate::error::business::BusinessError::Wallet(
                        crate::error::business::wallet::WalletError::PasswordIncorrect,
                    )
                    .into());
                }
            }
            None => {
                // 兼容历史数据：如果 password_proof 为空，尝试用传统方式验证
                tracing::info!("password_proof is None, trying fallback validation");
                if try_decrypt_wallet_db(password).await? {
                    // 验证成功，生成并存储 password_proof
                    let proof = Self::generate_password_proof(password).await?;
                    DeviceEntity::update_password_proof(pool.as_ref(), sn, Some(&proof)).await?;
                    tracing::info!("password_proof generated and stored");
                } else {
                    // 验证失败
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

        let pool = crate::context::CONTEXT.get().unwrap().api_wallet_pool()?;

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
        let sn = crate::context::CONTEXT.get().unwrap().get_sn();
        DeviceEntity::update_password(pool.as_ref(), sn, None).await?;

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
        let pool = crate::context::CONTEXT.get().unwrap().api_wallet_pool()?;
        let res = ApiWalletRepo::find_by_address(&pool, address).await?;
        Ok(!res.is_none())
    }

    // 生成 password_proof：使用用户密码加密固定明文
    pub(crate) async fn generate_password_proof(
        password: &str,
    ) -> Result<String, crate::error::service::ServiceError> {
        // 固定明文，不包含任何敏感信息
        const PROOF_STRING: &str = "wallet-sdk-password-proof";

        // 使用与系统一致的加密方式
        let algorithm = ConfigDomain::get_keystore_kdf_algorithm().await?;
        let rng = rand::rngs::OsRng::default();

        let proof = crate::domain::api_wallet::wallet::ApiWalletDomain::encrypt_password_proof(
            algorithm,
            rng,
            password,
            PROOF_STRING,
        )
        .await?;

        Ok(proof)
    }

    // 验证 password_proof：尝试用用户密码解密
    async fn decrypt_password_proof(
        proof: &str,
        password: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        // 固定明文，与 generate_password_proof 中的保持一致
        const PROOF_STRING: &str = "wallet-sdk-password-proof";

        // 使用 API wallet 中的解密函数
        let decrypted = crate::domain::api_wallet::wallet::ApiWalletDomain::decrypt_password_proof(
            password, proof,
        )
        .await?;

        if decrypted == PROOF_STRING {
            Ok(())
        } else {
            Err(crate::error::business::BusinessError::Wallet(
                crate::error::business::wallet::WalletError::PasswordIncorrect,
            )
            .into())
        }
    }
}

// 尝试用密码解密钱包数据库，最小成功原则
async fn try_decrypt_wallet_db(
    password: &str,
) -> Result<bool, crate::error::service::ServiceError> {
    let pool = crate::context::CONTEXT.get().unwrap().api_wallet_pool()?;

    // 尝试解密标准钱包（如果有）
    if let Some(wallet) =
        wallet_database::entities::wallet::WalletEntity::wallet_latest(&*pool.into_inner()).await?
    {
        // 尝试获取种子，这会涉及解密操作
        let dirs = crate::context::CONTEXT.get().unwrap().get_global_dirs();
        let root_dir = dirs.get_root_dir(&wallet.address)?;
        let wallet_tree_strategy = ConfigDomain::get_wallet_tree_strategy().await?;
        let wallet_tree = wallet_tree_strategy.get_wallet_tree(&dirs.wallet_dir)?;

        // 直接调用底层解密函数，避免副作用
        if wallet_tree::api::KeystoreApi::load_seed(
            &*wallet_tree,
            &root_dir,
            &wallet.address,
            password,
        )
        .is_ok()
        {
            tracing::info!("standard wallet decryption succeeded");
            return Ok(true);
        }
    }

    // 尝试解密 API 钱包（如果有）
    let api_wallets =
        wallet_database::repositories::api_wallet::wallet::ApiWalletRepo::list(&pool, None).await?;
    if let Some(wallet) = api_wallets.first() {
        // 尝试解密 API 钱包的 phrase
        if ApiWalletDomain::decrypt_phrase(password, &wallet.phrase).await.is_ok() {
            tracing::info!("API wallet phrase decryption succeeded");
            return Ok(true);
        }
        // 尝试解密 API 钱包的 seed
        if ApiWalletDomain::decrypt_seed(password, &wallet.seed).await.is_ok() {
            tracing::info!("API wallet seed decryption succeeded");
            return Ok(true);
        }
    }

    tracing::info!("all wallet decryption attempts failed");
    Ok(false)
}

struct SubsKeyInfo {
    pub wallet_address: String,
    pub address: String,
    pub chain_code: String,
}
