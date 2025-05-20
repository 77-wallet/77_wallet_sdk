use wallet_database::{
    entities::{account::AccountEntity, chain::ChainEntity, wallet::WalletEntity},
    repositories::{account::AccountRepoTrait, device::DeviceRepoTrait, ResourcesRepo},
};
use wallet_types::chain::{address::r#type::AddressType, chain::ChainCode};

use crate::{
    infrastructure::task_queue::BackendApiTaskData, response_vo::account::CreateAccountRes,
    service::asset::AddressChainCode,
};

use super::app::config::ConfigDomain;

pub struct AccountDomain {}

impl Default for AccountDomain {
    fn default() -> Self {
        Self::new()
    }
}

impl AccountDomain {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn get_addresses(
        &self,
        repo: &mut ResourcesRepo,
        address: &str,
        account_id: Option<u32>,
        chain_code: Option<String>,
        is_multisig: Option<bool>,
    ) -> Result<Vec<AddressChainCode>, crate::ServiceError> {
        let pool = crate::Context::get_global_sqlite_pool()?;
        let mut account_addresses = Vec::new();

        let chain_codes = if let Some(chain_code) = &chain_code {
            vec![chain_code.to_string()]
        } else {
            vec![]
        };

        if let Some(is_multisig) = is_multisig {
            if is_multisig {
                tracing::debug!("开始查询多签账户 is_multisig: {is_multisig}");
                tracing::debug!("多签账户地址 address: {address}");

                // 查询多签账户下的资产
                let account =
                    super::multisig::MultisigDomain::account_by_address(address, true, &pool)
                        .await?;
                tracing::debug!("查询成功 account: {account:?}");
                account_addresses.push(AddressChainCode {
                    address: account.address,
                    chain_code: account.chain_code,
                });
            } else {
                // 获取钱包下的这个账户的所有地址
                let accounts = repo
                    .account_list_by_wallet_address_and_account_id_and_chain_codes(
                        Some(address),
                        account_id,
                        chain_codes,
                    )
                    .await?;

                for account in accounts {
                    if !account_addresses.iter().any(|address| {
                        address.address == account.address
                            && address.chain_code == account.chain_code
                    }) {
                        account_addresses.push(AddressChainCode {
                            address: account.address,
                            chain_code: account.chain_code,
                        });
                    }
                }
            }
        } else {
            // 获取钱包下的这个账户的所有地址
            let accounts = repo
                .account_list_by_wallet_address_and_account_id_and_chain_codes(
                    Some(address),
                    account_id,
                    chain_codes,
                )
                .await?;
            for account in accounts {
                if !account_addresses.iter().any(|address| {
                    address.address == account.address && address.chain_code == account.chain_code
                }) {
                    account_addresses.push(AddressChainCode {
                        address: account.address,
                        chain_code: account.chain_code,
                    });
                }
            }
        }
        tracing::debug!("[get addresses] account_addresses: {account_addresses:?}");
        Ok(account_addresses)
    }

    pub(crate) async fn create_account(
        repo: &mut ResourcesRepo,
        seed: &[u8],
        instance: &wallet_chain_instance::instance::ChainObject,
        derivation_path: Option<&str>,
        account_index_map: &wallet_utils::address::AccountIndexMap,
        uid: &str,
        wallet_address: &str,
        name: &str,
        is_default_name: bool,
    ) -> Result<(CreateAccountRes, String, BackendApiTaskData), crate::ServiceError> {
        let (address, name, derivation_path) = Self::derive_subkey(
            repo,
            seed,
            account_index_map,
            instance,
            derivation_path,
            wallet_address,
            name,
            is_default_name,
        )
        .await?;
        let res = CreateAccountRes {
            address: address.to_string(),
        };
        let task_data = Self::address_init(
            repo,
            uid,
            &address,
            account_index_map.input_index,
            &instance.chain_code().to_string(),
            &name,
        )
        .await?;

        Ok((res, derivation_path, task_data))
    }

    pub(crate) async fn address_init(
        repo: &mut ResourcesRepo,
        uid: &str,
        address: &str,
        index: i32,
        chain_code: &str,
        name: &str,
    ) -> Result<BackendApiTaskData, crate::ServiceError> {
        let Some(device) = DeviceRepoTrait::get_device_info(repo).await? else {
            return Err(crate::BusinessError::Device(crate::DeviceError::Uninitialized).into());
        };
        let address_init_req = wallet_transport_backend::request::AddressInitReq::new(
            uid,
            address,
            index,
            chain_code,
            &device.sn,
            vec!["".to_string()],
            name,
        );
        let address_init_task_data = BackendApiTaskData::new(
            wallet_transport_backend::consts::endpoint::ADDRESS_INIT,
            &address_init_req,
        )?;

        Ok(address_init_task_data)
    }

    pub(crate) async fn derive_subkey(
        repo: &mut ResourcesRepo,
        seed: &[u8],
        account_index_map: &wallet_utils::address::AccountIndexMap,
        instance: &wallet_chain_instance::instance::ChainObject,
        derivation_path: Option<&str>,
        wallet_address: &str,
        name: &str,
        is_default_name: bool,
    ) -> Result<(String, String, String), crate::ServiceError> {
        let account_name = if is_default_name {
            format!("{name}{}", account_index_map.account_id)
        } else {
            name.to_string()
        };

        let keypair = if let Some(derivation_path) = derivation_path {
            instance
                .gen_keypair_with_derivation_path(seed, derivation_path)
                .map_err(|e| crate::SystemError::Service(e.to_string()))?
        } else {
            instance
                .gen_keypair_with_index_address_type(seed, account_index_map.input_index)
                .map_err(|e| crate::SystemError::Service(e.to_string()))?
        };

        let derivation_path = keypair.derivation_path();
        let chain_code = keypair.chain_code().to_string();

        let address_type = instance.address_type();
        let address = keypair.address();
        let pubkey = keypair.pubkey();

        let mut req = wallet_database::entities::account::CreateAccountVo::new(
            account_index_map.account_id,
            &address,
            &pubkey,
            wallet_address,
            &derivation_path,
            &chain_code,
            &account_name,
        );

        match address_type {
            AddressType::Btc(address_type) => {
                req = req.with_address_type(address_type.as_ref());
            }
            AddressType::Ltc(address_type) => {
                req = req.with_address_type(address_type.as_ref());
            }
            AddressType::Dog(address_type) => {
                req = req.with_address_type(address_type.as_ref());
            }
            AddressType::Ton(address_type) => {
                req = req.with_address_type(address_type.as_ref());
            }
            _ => {}
        }
        repo.upsert_multi_account(vec![req]).await?;
        Ok((address, account_name, derivation_path))
    }

    pub async fn set_root_password(
        wallet_address: &str,
        old_password: &str,
        new_password: &str,
    ) -> Result<(), crate::ServiceError> {
        // let tx = &mut self.repo;

        let dirs = crate::manager::Context::get_global_dirs()?;
        let db = crate::manager::Context::get_global_sqlite_pool()?;

        let wallet = WalletEntity::detail(db.as_ref(), wallet_address)
            .await?
            .ok_or(crate::BusinessError::Wallet(crate::WalletError::NotFound))?;

        // Get the path to the root directory for the given wallet name.
        let root_dir = dirs.get_root_dir(&wallet.address)?;

        // Traverse the directory structure to obtain the current wallet tree.

        let wallet_tree_strategy = ConfigDomain::get_wallet_tree_strategy().await?;
        let wallet_tree = wallet_tree_strategy.get_wallet_tree(&dirs.wallet_dir)?;

        let algorithm = ConfigDomain::get_keystore_kdf_algorithm().await?;
        Ok(wallet_tree::api::KeystoreApi::update_root_password(
            root_dir,
            wallet_tree,
            wallet_address,
            old_password,
            new_password,
            algorithm,
        )
        .map_err(|e| crate::SystemError::Service(e.to_string()))?)
    }

    pub async fn set_account_password(
        wallet_address: &str,
        account_index_map: &wallet_utils::address::AccountIndexMap,
        old_password: &str,
        new_password: &str,
    ) -> Result<(), crate::ServiceError> {
        let dirs = crate::manager::Context::get_global_dirs()?;
        let subs_dir = dirs.get_subs_dir(wallet_address)?;

        let wallet_tree_strategy = ConfigDomain::get_wallet_tree_strategy().await?;
        let wallet_tree = wallet_tree_strategy.get_wallet_tree(&dirs.wallet_dir)?;

        let algorithm = ConfigDomain::get_keystore_kdf_algorithm().await?;
        wallet_tree::api::KeystoreApi::update_account_password(
            wallet_tree,
            &subs_dir,
            account_index_map,
            old_password,
            new_password,
            algorithm,
        )
        .map_err(|e| crate::SystemError::Service(e.to_string()))?;

        Ok(())
    }

    pub async fn set_verify_password(password: &str) -> Result<(), crate::ServiceError> {
        let dirs = crate::manager::Context::get_global_dirs()?;
        wallet_tree::api::KeystoreApi::remove_verify_file(&dirs.root_dir)?;
        let wallet_tree_strategy = ConfigDomain::get_wallet_tree_strategy().await?;
        let wallet_tree = wallet_tree_strategy.get_wallet_tree(&dirs.wallet_dir)?;

        wallet_tree::api::KeystoreApi::store_verify_file(&*wallet_tree, &dirs.root_dir, password)?;

        Ok(())
    }

    pub(crate) async fn generate_subkey(
        instance: &wallet_chain_instance::instance::ChainObject,
        seed: &[u8],
        address: &str,
        chain_code: &str,
        account_index_map: &wallet_utils::address::AccountIndexMap,
        derivation_path: &str,
    ) -> Result<wallet_tree::file_ops::BulkSubkey, crate::ServiceError> {
        let keypair =
            instance.gen_keypair_with_index_address_type(seed, account_index_map.input_index)?;
        let private_key = keypair.private_key_bytes()?;

        Ok(wallet_tree::file_ops::BulkSubkey::new(
            account_index_map.clone(),
            address,
            chain_code,
            derivation_path,
            private_key,
        ))
    }
}

pub async fn open_accounts_pk_with_password(
    account_index_map: &wallet_utils::address::AccountIndexMap,
    address: &str,
    password: &str,
) -> Result<
    std::collections::HashMap<wallet_tree::KeyMeta, wallet_chain_interact::types::ChainPrivateKey>,
    crate::ServiceError,
> {
    let db = crate::manager::Context::get_global_sqlite_pool()?;
    let dirs = crate::manager::Context::get_global_dirs()?;

    let subs_path = dirs.get_subs_dir(address)?;
    // let storage_path = subs_path.join(name);
    let wallet_tree_strategy = ConfigDomain::get_wallet_tree_strategy().await?;
    let wallet_tree = wallet_tree_strategy.get_wallet_tree(&dirs.wallet_dir)?;

    let account_data = wallet_tree::api::KeystoreApi::load_account_pk(
        &*wallet_tree,
        account_index_map,
        &subs_path,
        password,
    )?;
    let mut res = std::collections::HashMap::default();
    for (meta, key) in account_data.into_inner() {
        let chain_code = &meta.chain_code;
        let Some(chain) = ChainEntity::chain_node_info(db.as_ref(), chain_code).await? else {
            return Err(crate::ServiceError::Business(crate::BusinessError::Chain(
                crate::ChainError::NotFound(chain_code.to_string()),
            )));
        };
        let chain_code = chain_code.as_str().try_into()?;

        let private_key = match chain_code {
            ChainCode::Solana => {
                wallet_utils::parse_func::sol_keypair_from_bytes(&key)?.to_base58_string()
            }
            ChainCode::Bitcoin => {
                wallet_chain_interact::btc::wif_private_key(&key, chain.network.as_str().into())?
            }
            _ => hex::encode(key),
        };

        res.insert(meta, private_key.into());
    }
    Ok(res)
}

pub async fn open_subpk_with_password(
    chain_code: &str,
    address: &str,
    password: &str,
) -> Result<wallet_chain_interact::types::ChainPrivateKey, crate::ServiceError> {
    // super::wallet::WalletDomain::validate_password(password).await?;

    let db = crate::manager::Context::get_global_sqlite_pool()?;
    let dirs = crate::manager::Context::get_global_dirs()?;

    let req = wallet_database::entities::account::QueryReq::new_address_chain(address, chain_code);

    let account =
        AccountEntity::detail(db.as_ref(), &req)
            .await?
            .ok_or(crate::BusinessError::Account(
                crate::AccountError::NotFound(address.to_string()),
            ))?;

    let wallet = WalletEntity::detail(db.as_ref(), &account.wallet_address)
        .await?
        .ok_or(crate::BusinessError::Wallet(crate::WalletError::NotFound))?;
    let Some(chain) = ChainEntity::chain_node_info(db.as_ref(), chain_code).await? else {
        return Err(crate::ServiceError::Business(crate::BusinessError::Chain(
            crate::ChainError::NotFound(chain_code.to_string()),
        )));
    };

    let chain_code: ChainCode = chain_code.try_into()?;

    let subs_path = dirs.get_subs_dir(&wallet.address)?;
    let wallet_tree_strategy = ConfigDomain::get_wallet_tree_strategy().await?;
    let wallet_tree = wallet_tree_strategy.get_wallet_tree(&dirs.wallet_dir)?;

    let account_index_map =
        wallet_utils::address::AccountIndexMap::from_account_id(account.account_id)?;
    let key = wallet_tree::api::KeystoreApi::load_sub_pk(
        &*wallet_tree,
        Some(&account_index_map),
        &subs_path,
        address,
        &chain_code.to_string(),
        &account.derivation_path,
        password,
    )?;

    // TODO: 优化
    let private_key = match chain_code {
        ChainCode::Solana => {
            wallet_utils::parse_func::sol_keypair_from_bytes(&key)?.to_base58_string()
        }
        ChainCode::Bitcoin => {
            wallet_chain_interact::btc::wif_private_key(&key, chain.network.as_str().into())?
        }
        ChainCode::Dogcoin => {
            wallet_chain_interact::dog::wif_private_key(&key, chain.network.as_str().into())?
        }
        ChainCode::Litecoin => {
            wallet_chain_interact::ltc::wif_private_key(&key, chain.network.as_str().into())?
        }
        _ => hex::encode(key),
    };
    Ok(private_key.into())
}
