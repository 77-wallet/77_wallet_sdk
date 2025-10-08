use crate::{
    context::CONTEXT,
    domain::{
        account::AccountDomain,
        api_wallet::{chain::ApiChainDomain, wallet::ApiWalletDomain},
        app::config::ConfigDomain,
        coin::CoinDomain,
    },
    error::service::ServiceError,
    infrastructure::task_queue::{
        CommonTask,
        backend::{BackendApiTask, BackendApiTaskData},
        task::Tasks,
    },
    response_vo::{
        account::BalanceInfo,
        api_wallet::account::{ApiAccountInfo, ApiAccountInfos},
        chain::ChainCodeAndName,
    },
    service::api_wallet::asset::AddressChainCode,
};
use wallet_chain_interact::types::ChainPrivateKey;
use wallet_crypto::{
    EncryptedJsonDecryptor as _, EncryptedJsonGenerator as _, KeystoreJsonDecryptor,
    KeystoreJsonGenerator,
};
use wallet_database::{
    entities::{
        api_account::{ApiAccountEntity, CreateApiAccountVo},
        api_wallet::ApiWalletType,
        chain::ChainEntity,
    },
    repositories::{
        api_wallet::{account::ApiAccountRepo, chain::ApiChainRepo, wallet::ApiWalletRepo},
        coin::CoinRepo,
        device::DeviceRepo,
    },
};
use wallet_transport_backend::request::{
    AddressInitReq, TokenQueryPriceReq, api_wallet::address::ApiAddressInitReq,
};
use wallet_types::chain::{address::r#type::AddressType, chain::ChainCode};

pub(crate) struct ApiAccountDomain {}

impl ApiAccountDomain {
    pub(crate) async fn list_api_accounts(
        wallet_address: &str,
        account_id: Option<u32>,
        chain_code: Option<String>,
    ) -> Result<ApiAccountInfos, ServiceError> {
        let pool = CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        let chains = ApiChainRepo::get_chain_list(&pool).await?;
        let chain_codes = if let Some(chain_code) = chain_code {
            vec![chain_code]
        } else {
            chains.iter().map(|chain| chain.chain_code.clone()).collect()
        };

        let chains: ChainCodeAndName = chains.into();

        let mut res = ApiAccountInfos::new();
        let wallet = ApiWalletRepo::find_by_address(&pool, wallet_address).await?.ok_or(
            crate::error::service::ServiceError::Business(
                crate::error::business::BusinessError::ApiWallet(
                    crate::error::business::api_wallet::ApiWalletError::NotFound,
                ),
            ),
        )?;

        let account_list =
            ApiAccountRepo::api_account_list(&pool, Some(wallet.address), account_id, chain_codes)
                .await?;
        for account in account_list {
            let address_type =
                AccountDomain::get_show_address_type(&account.chain_code, account.address_type())?;

            let name = chains.get(&account.chain_code);
            if let Some(info) = res.iter_mut().find(|info| info.account_id == account.account_id) {
                info.chain.push(crate::response_vo::wallet::ChainInfo {
                    address: account.address,
                    wallet_address: account.wallet_address,
                    derivation_path: account.derivation_path,
                    chain_code: account.chain_code,
                    name: name.cloned(),
                    address_type,
                    created_at: account.created_at,
                    updated_at: account.updated_at,
                });
            } else {
                let account_index_map =
                    wallet_utils::address::AccountIndexMap::from_account_id(account.account_id)?;
                let balance = BalanceInfo::new_without_amount().await?;
                res.push(ApiAccountInfo {
                    account_id: account.account_id,
                    account_index_map,
                    name: account.name,
                    balance,
                    chain: vec![crate::response_vo::wallet::ChainInfo {
                        address: account.address,
                        wallet_address: account.wallet_address,
                        derivation_path: account.derivation_path,
                        chain_code: account.chain_code,
                        name: name.cloned(),
                        address_type,
                        created_at: account.created_at,
                        updated_at: account.updated_at,
                    }],
                    api_wallet_type: account.api_wallet_type,
                });
            }
        }

        Ok(res)
    }

    pub(crate) async fn get_private_key(
        address: &str,
        chain_code: &str,
        password: &str,
    ) -> Result<ChainPrivateKey, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let account = ApiAccountRepo::find_one_by_address_chain_code(address, chain_code, &pool)
            .await?
            .ok_or(crate::error::business::BusinessError::Account(
                crate::error::business::account::AccountError::NotFound(address.to_string()),
            ))?;

        let key = KeystoreJsonDecryptor.decrypt(password.as_ref(), &account.private_key)?;

        tracing::info!("get_private_key ------------------- 6: {chain_code}");
        let chain = ChainEntity::chain_node_info(pool.as_ref(), chain_code).await?.ok_or(
            crate::error::service::ServiceError::Business(
                crate::error::business::BusinessError::Chain(
                    crate::error::business::chain::ChainError::NotFound(chain_code.to_string()),
                ),
            ),
        )?;

        tracing::info!("chain_code ------------------- 7: {chain_code}");
        let chain_code: ChainCode = chain_code.try_into()?;
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

    // pub(crate) async fn decrypt_phrase(
    //     password: &str,
    //     phrase: &str,
    // ) -> Result<String, crate::ServiceError> {
    //     let data = KeystoreJsonDecryptor.decrypt(password.as_ref(), phrase)?;
    //     let phrase = wallet_utils::conversion::vec_to_string(&data)?;
    //     Ok(phrase)
    // }

    pub(crate) async fn derive_subkey(
        uid: &str,
        seed: &[u8],
        wallet_address: &str,
        account_index_map: &wallet_utils::address::AccountIndexMap,
        instance: &wallet_chain_instance::instance::ChainObject,
        account_name: &str,
        is_default_name: bool,
        wallet_password: &str,
        api_wallet_type: ApiWalletType,
    ) -> Result<(String, Option<AddressInitReq>), crate::error::service::ServiceError> {
        let account_name = if is_default_name {
            format!("{account_name}{}", account_index_map.account_id)
        } else {
            account_name.to_string()
        };

        let (address, pubkey, private_key, chain_code, derivation_path) = {
            let keypair = instance
                .gen_keypair_with_index_address_type(seed, account_index_map.input_index)
                .map_err(|e| crate::error::system::SystemError::Service(e.to_string()))?;
            (
                keypair.address(),
                keypair.pubkey(),
                keypair.private_key_bytes()?,
                keypair.chain_code().to_string(),
                keypair.derivation_path(),
            )
        };

        // let keypair = instance
        //     .gen_keypair_with_index_address_type(seed, account_index_map.input_index)
        //     .map_err(|e| crate::SystemError::Service(e.to_string()))?;

        // let derivation_path = keypair.derivation_path();
        // let chain_code = keypair.chain_code().to_string();

        let address_type = instance.address_type();
        // let address = keypair.address();
        // let pubkey = keypair.pubkey();
        // let private_key = keypair.private_key()?;
        // let private_key = keypair.private_key_bytes()?;

        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let account = ApiAccountRepo::find_one(
            &pool,
            &address,
            &chain_code,
            &address_type.to_string(),
            api_wallet_type,
        )
        .await?;
        let Some(device) = DeviceRepo::get_device_info(pool.clone()).await? else {
            return Err(crate::error::business::BusinessError::Device(
                crate::error::business::device::DeviceError::Uninitialized,
            )
            .into());
        };

        // let private_key = wallet_utils::serde_func::serde_to_vec(&private_key)?;

        let private_key = {
            let algorithm = ConfigDomain::get_keystore_kdf_algorithm().await?;
            let rng = rand::thread_rng();
            let mut generator = KeystoreJsonGenerator::new(rng.clone(), algorithm.clone());
            generator.generate(wallet_password.as_bytes(), &private_key)?
        };

        let private_key = wallet_utils::serde_func::serde_to_string(&private_key)?;

        let mut req = CreateApiAccountVo::new(
            account_index_map.account_id,
            &address,
            &pubkey,
            &private_key,
            wallet_address,
            &derivation_path,
            &account_index_map.input_index.to_string(),
            &chain_code,
            &account_name,
            api_wallet_type,
        );

        let address_init_req = if let Some(account) = account {
            tracing::info!("已存在: {}", account.address);
            None
        } else {
            Some(wallet_transport_backend::request::AddressInitReq::new(
                uid,
                &address,
                account_index_map.input_index,
                &instance.chain_code().to_string(),
                &device.sn,
                vec!["".to_string()],
                &account_name,
            ))
        };

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

        ApiAccountRepo::upsert(&pool, vec![req]).await?;

        Ok((address, address_init_req))
    }

    pub(crate) async fn address_used(
        chain_code: &str,
        index: i32,
        uid: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let api_wallet = ApiWalletRepo::find_by_uid(&pool, uid).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::ApiWalletError::NotFound,
            ),
        )?;
        let index = wallet_utils::address::AccountIndexMap::from_input_index(index)?;

        let accounts = ApiAccountRepo::find_all_by_wallet_address_index(
            &pool,
            &api_wallet.address,
            chain_code,
            index.account_id,
        )
        .await?;
        for account in accounts {
            ApiAccountRepo::mark_as_used(
                &pool,
                &api_wallet.address,
                account.account_id,
                chain_code,
            )
            .await?;
        }

        Ok(())
    }

    pub async fn account(
        chain_code: &str,
        address: &str,
    ) -> Result<ApiAccountEntity, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let account = ApiAccountRepo::find_one_by_address_chain_code(address, chain_code, &pool)
            .await?
            .ok_or(crate::error::business::BusinessError::Account(
                crate::error::business::account::AccountError::NotFound(address.to_string()),
            ))?;
        Ok(account)
    }

    pub fn next_account_indices(mut existing: Vec<u32>, count: u32) -> Vec<u32> {
        existing.sort();
        let set: std::collections::HashSet<u32> = existing.into_iter().collect();

        let mut result = Vec::new();
        let mut candidate = 1;

        while result.len() < count as usize {
            if !set.contains(&candidate) {
                result.push(candidate);
            }
            candidate += 1;
        }

        result
    }

    pub async fn get_addresses(
        address: &str,
        account_id: Option<u32>,
        chain_codes: Vec<String>,
    ) -> Result<Vec<AddressChainCode>, ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let mut account_addresses = Vec::new();

        // 获取钱包下的这个账户的所有地址
        let accounts = ApiAccountRepo::api_account_list(
            &pool,
            Some(address.to_string()),
            account_id,
            chain_codes,
        )
        .await?;

        for account in accounts {
            if !account_addresses.iter().any(|address: &AddressChainCode| {
                address.address == account.address && address.chain_code == account.chain_code
            }) {
                account_addresses.push(AddressChainCode {
                    address: account.address,
                    chain_code: account.chain_code,
                });
            }
        }

        tracing::debug!("[get addresses] account_addresses: {account_addresses:?}");
        Ok(account_addresses)
    }

    pub(crate) async fn create_sub_account(
        wallet_address: &str,
        password: &str,
        chains: Vec<String>,
        account_name: &str,
        is_default_name: bool,
        number: u32,
    ) -> Result<(), ServiceError> {
        let pool = CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        // 查询已有的账户
        let account_indices =
            ApiAccountRepo::get_all_account_indices(&pool, wallet_address).await?;
        let account_indices = ApiAccountDomain::next_account_indices(account_indices, number);
        let mut input_indices = Vec::new();
        for account_id in account_indices {
            input_indices.push(
                wallet_utils::address::AccountIndexMap::from_account_id(account_id)?.input_index,
            );
        }

        Self::create_api_account(
            wallet_address,
            password,
            chains,
            input_indices,
            account_name,
            is_default_name,
            ApiWalletType::SubAccount,
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn create_withdrawal_account(
        wallet_address: &str,
        password: &str,
        chains: Vec<String>,
        account_name: &str,
        is_default_name: bool,
    ) -> Result<(), ServiceError> {
        Self::create_api_account(
            wallet_address,
            password,
            chains,
            vec![0, 1],
            account_name,
            is_default_name,
            ApiWalletType::Withdrawal,
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn create_api_account(
        wallet_address: &str,
        wallet_password: &str,
        chains: Vec<String>,
        input_indices: Vec<i32>,
        name: &str,
        is_default_name: bool,
        api_wallet_type: ApiWalletType,
    ) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let api_wallet = ApiWalletRepo::find_by_address(&pool, wallet_address).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::ApiWalletError::NotFound,
            ),
        )?;
        // 获取种子
        let seed = ApiWalletDomain::decrypt_seed(wallet_password, &api_wallet.seed).await?;

        // 获取默认链和币
        // let default_chain_list = ChainRepo::get_chain_list(&pool).await?;
        let default_coins_list = CoinRepo::default_coin_list(&pool).await?;

        // // 如果有指定派生路径，就获取该链的所有chain_code
        // let chains: Vec<String> =
        //     default_chain_list.iter().map(|chain| chain.chain_code.clone()).collect();

        let mut created_count = 0;
        // let mut current_id = if let Some(idx) = index {
        //     wallet_utils::address::AccountIndexMap::from_input_index(idx)?.account_id
        // } else {
        //     1
        // };

        let mut req: TokenQueryPriceReq = TokenQueryPriceReq(Vec::new());
        let mut api_address_init_req = ApiAddressInitReq::new();
        // let mut expand_address_req = ApiAddressInitReq::new_sdk(&api_wallet.uid);
        // let mut subkeys = Vec::<wallet_tree::file_ops::BulkSubkey>::new();
        for input_index in input_indices {
            // 构造 index map
            let account_index_map =
                wallet_utils::address::AccountIndexMap::from_input_index(input_index)?;

            // 跳过已存在账户
            if ApiAccountRepo::has_account_id(
                &pool,
                wallet_address,
                account_index_map.account_id,
                api_wallet_type,
            )
            .await?
            {
                // current_id += 1;
                continue;
            }

            ApiChainDomain::init_chains_api_assets(
                &default_coins_list,
                &mut req,
                &mut api_address_init_req,
                // &mut subkeys,
                // &mut expand_address_req,
                &chains,
                &seed,
                &account_index_map,
                &api_wallet.uid,
                &api_wallet.address,
                name,
                is_default_name,
                wallet_password,
                api_wallet_type,
            )
            .await?;

            created_count += 1;
            // current_id += 1;
        }
        if created_count > 0 {
            // let address_batch_init_task_data = BackendApiTaskData::new(
            //     wallet_transport_backend::consts::endpoint::old_wallet::OLD_ADDRESS_BATCH_INIT,
            //     &address_batch_init_task_data,
            // )?;

            // let backend_api = crate::Context::get_global_backend_api()?;
            // backend_api.expand_address(&expand_address_req).await?;
            // let expand_address_task_data = BackendApiTaskData::new(
            //     wallet_transport_backend::consts::endpoint::api_wallet::ADDRESS_POOL_EXPAND,
            //     &expand_address_req,
            // )?;
            let api_address_init_task_data = BackendApiTaskData::new(
                wallet_transport_backend::consts::endpoint::api_wallet::ADDRESS_INIT,
                &api_address_init_req,
            )?;

            Tasks::new()
                .push(CommonTask::QueryCoinPrice(req))
                .push(BackendApiTask::BackendApi(api_address_init_task_data))
                // .push(BackendApiTask::BackendApi(expand_address_task_data))
                .send()
                .await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use alloy::signers::local::PrivateKeySigner;
    use wallet_crypto::{EncryptedJsonDecryptor, KeystoreJsonDecryptor};

    async fn test_keystore_key() -> Result<(), Box<dyn std::error::Error>> {
        let key = KeystoreJsonDecryptor.decrypt("q1111111".as_bytes(),r#"{"crypto":{"cipher":"aes-128-ctr","cipherparams":{"iv":"cafaaf94330ae23b8a8eb64660d42740"},"ciphertext":"19e4fee3686f858bc45946665ee751a9964ef956d06ecee2f7a90021bd946529","kdf":"argon2id","kdfparams":{"dklen":32,"time_cost":5,"memory_cost":131072,"parallelism":8,"salt":[63,15,27,159,163,164,60,107,41,155,135,165,52,165,224,219,52,197,122,0,161,45,75,23,49,198,4,140,1,67,182,207]},"mac":"faf334de5be2b30526a8755980372718aad9b477b52753bde820cb6673bba7a9"},"id":"83577d8c-af30-44e6-9f06-5e616b0ac2be","version":3}"#)?;
        let h = hex::encode(key);
        let signer: PrivateKeySigner = h.parse().map_err(|_| {
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::ApiWalletError::NotFound,
            )
        })?;
        Ok(())
    }

    #[tokio::test]
    async fn test_keystore() {
        let res = test_keystore_key().await;
        assert!(res.is_ok());
    }
}
