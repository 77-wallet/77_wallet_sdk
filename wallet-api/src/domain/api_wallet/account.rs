use std::cmp::Ordering;
use std::collections::HashSet;
use crate::{
    context::CONTEXT,
    domain::{
        account::AccountDomain,
        api_wallet::{chain::ApiChainDomain, wallet::ApiWalletDomain},
        app::config::ConfigDomain,
        chain::ChainDomain,
    },
    error::service::ServiceError,
    infrastructure::task_queue::{
        CommonTask, EncryptPrivateKeyTask,
        backend::{BackendApiTask, BackendApiTaskData},
        task::Tasks,
    },
    messaging::{
        mqtt::topics::api_wallet::cmd::address_allock::ExpandStatus,
        notify::{FrontendNotifyEvent, api_wallet::AwmCmdAddrExpandMsgFront, event::NotifyEvent},
    },
    response_vo::{
        api_wallet::account::ApiAccountInfo,
        standard_wallet::{account::BalanceInfo, chain::ChainCodeAndName, wallet::ChainInfo},
    },
    service::api_wallet::asset::AddressChainCode,
};
use wallet_chain_interact::types::ChainPrivateKey;
use wallet_crypto::{
    EncryptedJsonDecryptor as _, EncryptedJsonGenerator as _, KeystoreJsonDecryptor,
};
use wallet_database::{
    entities::{
        api_account::CreateApiAccountVo, api_wallet::ApiWalletType, chain::ChainEntity,
        exchange_rate::ExchangeRateEntity,
    },
    pagination::Pagination,
    repositories::{
        api_wallet::{
            account::ApiAccountRepo, chain::ApiChainRepo, coin::ApiCoinRepo, wallet::ApiWalletRepo,
        },
        device::DeviceRepo,
        exchange_rate::ExchangeRateRepo,
        task_queue::TaskQueueRepo,
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
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<ApiAccountInfo>, ServiceError> {
        let pool = CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        let chains = ApiChainRepo::get_chain_list(&pool).await?;
        let chain_codes = if let Some(ref chain_code) = chain_code {
            vec![chain_code.to_string()]
        } else {
            chains.iter().map(|chain| chain.chain_code.clone()).collect()
        };

        let chains: ChainCodeAndName = chains.into();

        let wallet = ApiWalletRepo::find_by_address(&pool, wallet_address).await?.ok_or(
            crate::error::service::ServiceError::Business(
                crate::error::business::BusinessError::ApiWallet(
                    crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
                ),
            ),
        )?;

        let account_list =
            ApiAccountRepo::api_account_list(&pool, Some(wallet.address), account_id, chain_codes)
                .await?;

        // let balance_list =
        //     crate::infrastructure::asset_calc::get_balance_summary(wallet_address, chain_code)
        //         .await?;

        // tracing::info!("list_api_accounts balance_list: {balance_list:#?}");

        let mut filtered_accounts: Vec<ApiAccountInfo> = Vec::new();
        for account in account_list {
            let address_type =
                AccountDomain::get_show_address_type(&account.chain_code, account.address_type())?;

            let name = chains.get(&account.chain_code);
            // let balance = if let Some(balance) = balance_list.get(&account.address) {
            //     balance.clone()
            // } else {
            //     BalanceInfo::new_without_amount().await?
            // };
            let asset_calc_actor_manager =
                CONTEXT.get().unwrap().get_global_asset_calc_actor_manager().await?;
            let balance = asset_calc_actor_manager
                .get_balance_summary(
                    Some(wallet_address),
                    Some(account.account_id),
                    chain_code.as_deref(),
                )
                .await?;
            // let balance = crate::infrastructure::asset_calc::get_balance_summary(
            //     Some(wallet_address),
            //     Some(account.account_id),
            //     chain_code.as_deref(),
            // )
            // .await?;

            // tracing::info!("list_api_accounts balance: {balance:#?}");
            // if balance.amount.is_zero() {
            //     continue;
            // }

            if let Some(info) =
                filtered_accounts.iter_mut().find(|info| info.account_id == account.account_id)
            {
                info.chain.push(crate::response_vo::standard_wallet::wallet::ChainInfo {
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
                filtered_accounts.push(ApiAccountInfo {
                    account_id: account.account_id,
                    account_index_map,
                    name: account.name,
                    balance,
                    chain: vec![crate::response_vo::standard_wallet::wallet::ChainInfo {
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

        filtered_accounts
            .sort_by(|a, b| a.account_id.partial_cmp(&b.account_id).unwrap_or(Ordering::Equal));

        let total_count = filtered_accounts.len() as i64;
        let start = (page * page_size).max(0) as usize;
        let end = (start + page_size as usize).min(filtered_accounts.len());

        let data = if start < filtered_accounts.len() {
            filtered_accounts[start..end].to_vec()
        } else {
            Vec::new()
        };

        Ok(Pagination { page, page_size, total_count, data })
    }

    pub(crate) async fn list_api_accounts_v2(
        wallet_address: &str,
        account_id: Option<u32>,
        chain_code: Option<String>,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<ApiAccountInfo>, ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let account_assert = ApiAccountRepo::lists_by_wallet_address_v2(
            &pool,
            wallet_address,
            account_id,
            chain_code.clone(),
            page,
            page_size,
        )
        .await?;
        let account_assert_total = ApiAccountRepo::count_by_wallet_address_v2(
            &pool,
            wallet_address,
            account_id,
            chain_code,
        )
        .await?;

        let currency = ConfigDomain::get_currency().await?;
        let exchange_rate =
            ExchangeRateRepo::get_by_target_currency_or_default(&pool, &currency).await?;
        let cal_exchange_rate = |value: f64| {
            if exchange_rate.target_currency.to_uppercase() == "USD" {
                value
            } else {
                value * exchange_rate.rate
            }
        };

        let mut result: Vec<_> = vec![];
        for acc in account_assert {
            let account_index_map =
                wallet_utils::address::AccountIndexMap::from_account_id(acc.account_id)?;

            let mut chain_vec = vec![];
            let mut has_chain =HashSet::new();
            for one in acc.get_chain_info_list()?.into_iter() {
                let address_type =
                    AccountDomain::get_show_address_type(&one.chain_code, one.address_type())?;
                let r = ChainInfo {
                    address: one.account_address,
                    wallet_address: one.wallet_address,
                    derivation_path: one.derivation_path,
                    chain_code: one.chain_code,
                    name: one.chain_name,
                    address_type,
                    created_at: one.created_at,
                    updated_at: one.updated_at,
                };
                if !has_chain.contains(&r.chain_code) {
                    continue;
                }
                has_chain.insert(r.chain_code.clone());
                chain_vec.push(r);

                break;
            }

            result.push(ApiAccountInfo {
                chain: chain_vec,
                account_id: acc.account_id,
                account_index_map,
                name: acc.account_name,
                balance: BalanceInfo {
                    amount: acc.total_coins_quantity,
                    currency: currency.clone(),
                    unit_price: acc.coin_unit_price.map(cal_exchange_rate),
                    fiat_value: acc.total_account_amount.map(cal_exchange_rate),
                },
                api_wallet_type: ApiWalletType::InvalidValue,
            })
        }

        Ok(Pagination { page, page_size, total_count: account_assert_total, data: result })
    }

    /// 从种子生成私钥的公共函数
    pub(crate) async fn generate_private_key_from_seed(
        pool: &wallet_database::DbPool,
        wallet_address: &str,
        password: &str,
        chain_code: &str,
        address_type: &AddressType,
        account_id: u32,
    ) -> Result<Vec<u8>, crate::error::service::ServiceError> {
        // 获取钱包信息
        let api_wallet =
            ApiWalletRepo::find_by_address(pool, wallet_address).await?.ok_or_else(|| {
                crate::error::business::BusinessError::ApiWallet(
                    crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
                )
            })?;

        // 解密种子
        let seed = ApiWalletDomain::decrypt_seed(password, &api_wallet.seed).await?;

        // 转换链码
        let code: ChainCode = chain_code.try_into()?;

        // 获取节点信息
        let node = ChainDomain::get_node(chain_code).await?;

        // 创建链实例
        let instance: wallet_chain_instance::instance::ChainObject =
            (&code, address_type, node.network.as_str().into()).try_into()?;

        // 解析账户索引
        let account_index_map =
            wallet_utils::address::AccountIndexMap::from_account_id(account_id)?;

        // 生成密钥对
        let keypair =
            instance.gen_keypair_with_index_address_type(&seed, account_index_map.input_index)?;

        // 获取私钥字节
        let res = keypair.private_key_bytes()?;
        Ok(res)
    }

    /// 加密私钥的公共函数
    pub(crate) async fn encrypt_private_key(
        password: &str,
        private_key_bytes: &[u8],
    ) -> Result<String, crate::error::service::ServiceError> {
        use crate::domain::app::config::ConfigDomain;
        use rand::rngs::OsRng;
        use wallet_crypto::KeystoreJsonGenerator;

        // 获取加密算法
        let algorithm = ConfigDomain::get_keystore_kdf_algorithm().await?;
        let rng = OsRng;
        let mut generator = KeystoreJsonGenerator::new(rng, algorithm.clone());
        let encrypted_private_key = generator.generate(password.as_bytes(), private_key_bytes)?;

        // 序列化为字符串
        Ok(wallet_utils::serde_func::serde_to_string(&encrypted_private_key)?)
    }

    pub(crate) async fn get_private_key(
        address: &str,
        chain_code: &str,
        password: &str,
    ) -> Result<ChainPrivateKey, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        // 查找账户信息
        let account = ApiAccountRepo::find_one_by_address_chain_code(address, chain_code, &pool)
            .await?
            .ok_or_else(|| {
                crate::error::business::BusinessError::Account(
                    crate::error::business::account::AccountError::NotFound(address.to_string()),
                )
            })?;

        // 获取链信息
        let chain =
            ChainEntity::chain_node_info(pool.as_ref(), chain_code).await?.ok_or_else(|| {
                crate::error::business::BusinessError::Chain(
                    crate::error::business::chain::ChainError::NotFound(chain_code.to_string()),
                )
            })?;

        let key = if let Some(encrypted_private_key) = account.private_key {
            // 如果有加密的私钥，直接解密
            KeystoreJsonDecryptor.decrypt(password.as_ref(), &encrypted_private_key)?
        } else {
            // 当private_key为None时，动态派生出私钥
            let address_type: AddressType = account.address_type().try_into()?;

            // 调用公共函数生成私钥
            let private_key_bytes = Self::generate_private_key_from_seed(
                &pool,
                &account.wallet_address,
                password,
                chain_code,
                &address_type,
                account.account_id,
            )
            .await?;

            // 加密私钥
            let encrypted_private_key_str =
                Self::encrypt_private_key(password, &private_key_bytes).await?;

            // 更新数据库中的私钥
            ApiAccountRepo::update_private_key(&pool, address, &encrypted_private_key_str).await?;

            private_key_bytes
        };

        // 转换链码用于后续处理
        let code: ChainCode = chain_code.try_into()?;

        // 根据链类型格式化私钥
        let private_key = match code {
            ChainCode::Solana => {
                let keypair = wallet_utils::parse_func::sol_keypair_from_bytes(&key)?;
                keypair.to_base58_string()
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
        api_wallet_type: ApiWalletType,
    ) -> Result<(String, Option<AddressInitReq>), crate::error::service::ServiceError> {
        let account_name = if is_default_name {
            format!("{account_name}{}", account_index_map.account_id)
        } else {
            account_name.to_string()
        };

        let (address, pubkey, chain_code, derivation_path) = {
            let keypair = instance
                .gen_keypair_with_index_address_type(seed, account_index_map.input_index)
                .map_err(|e| crate::error::system::SystemError::Service(e.to_string()))?;
            (
                keypair.address(),
                keypair.pubkey(),
                keypair.chain_code().to_string(),
                keypair.derivation_path(),
            )
        };

        let address_type = instance.address_type();

        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let account = ApiAccountRepo::find_one(
            &pool,
            &address,
            &chain_code,
            &address_type.to_string(),
            api_wallet_type,
        )
        .await?;
        let sn = CONTEXT.get().unwrap().get_sn();
        let Some(device) = DeviceRepo::get_device_info(pool.clone(), sn).await? else {
            return Err(crate::error::business::BusinessError::Device(
                crate::error::business::device::DeviceError::Uninitialized,
            )
            .into());
        };

        // 将私钥加密任务加入队列异步处理
        let address_type = instance.address_type();
        let encrypted_private_key_task = EncryptPrivateKeyTask::new(
            &address,
            address_type,
            account_index_map.account_id,
            wallet_address,
            &chain_code,
            api_wallet_type,
        );

        Tasks::new().push(CommonTask::EncryptPrivateKey(encrypted_private_key_task)).send().await?;

        let mut req = CreateApiAccountVo::new(
            account_index_map.account_id,
            &address,
            &pubkey,
            None, // 暂时不存储私钥
            wallet_address,
            &derivation_path,
            account_index_map.input_index,
            &chain_code,
            &account_name,
            api_wallet_type,
        );

        let address_init_req = if let Some(account) = account
            && account.is_init == 1
        {
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

        let asset_calc_actor_manager =
            CONTEXT.get().unwrap().get_global_asset_calc_actor_manager().await?;
        asset_calc_actor_manager
            .add_account_to_cache(&address, account_index_map.account_id, wallet_address)
            .await;

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
                crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
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

    pub fn next_account_indices(mut existing: Vec<u32>, count: u32) -> Vec<u32> {
        // 去重并排序，确保我们只处理唯一的索引
        existing.sort();
        existing.dedup();
        let set: std::collections::HashSet<u32> = existing.into_iter().collect();

        let mut result = Vec::new();
        let mut candidate = 1; // 从1开始寻找可用索引，优先填充空洞

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
        uid: &str,
        password: &str,
        chain_code: &str,
        account_name: &str,
        is_default_name: bool,
        number: u32,
        input_indices: Vec<i32>,
        batch_id: Option<String>,
    ) -> Result<(), ServiceError> {
        const BATCH_SIZE: usize = 10;

        // let pool = CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        // // 查询已有的账户
        // let account_indices =
        //     ApiAccountRepo::get_all_account_indices(&pool, wallet_address, chain_code).await?;
        // let account_indices = ApiAccountDomain::next_account_indices(account_indices, number);

        let done_num = 0;
        for batch in input_indices.chunks(BATCH_SIZE) {
            // let mut input_indices = Vec::with_capacity(batch.len());
            // for account_id in batch {
            //     input_indices.push(
            //         wallet_utils::address::AccountIndexMap::from_account_id(*account_id)?
            //             .input_index,
            //     );
            // }

            // 每批创建一次
            Self::create_api_account(
                wallet_address,
                password,
                vec![chain_code.to_string()],
                batch,
                account_name,
                is_default_name,
                ApiWalletType::SubAccount,
                batch_id.clone(),
            )
            .await?;

            let data = AwmCmdAddrExpandMsgFront {
                uid: uid.to_string(),
                number,
                done_number: done_num + batch.len() as u32,
            };
            let data = NotifyEvent::AwmCmdAddrExpand(data);
            FrontendNotifyEvent::new(data).send().await?;
        }

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
            &[0, 1],
            account_name,
            is_default_name,
            ApiWalletType::Withdrawal,
            None,
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn create_api_account(
        wallet_address: &str,
        wallet_password: &str,
        chains: Vec<String>,
        input_indices: &[i32],
        name: &str,
        is_default_name: bool,
        api_wallet_type: ApiWalletType,
        batch_id: Option<String>,
    ) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let api_wallet = ApiWalletRepo::find_by_address(&pool, wallet_address).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
            ),
        )?;
        // 获取种子
        let seed = ApiWalletDomain::decrypt_seed(wallet_password, &api_wallet.seed).await?;

        // 获取默认链和币
        // let default_chain_list = ChainRepo::get_chain_list(&pool).await?;
        let default_coins_list = ApiCoinRepo::coin_list(&pool).await?;

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
        // 收集所有需要更新的资产键
        let mut all_asset_keys = Vec::new();
        // let mut expand_address_req = ApiAddressInitReq::new_sdk(&api_wallet.uid);
        // let mut subkeys = Vec::<wallet_tree::file_ops::BulkSubkey>::new();

        if let Some(batch_id) = &batch_id {
            api_address_init_req = api_address_init_req.with_batch_id(batch_id);
        }
        for input_index in input_indices {
            // 构造 index map
            let account_index_map =
                wallet_utils::address::AccountIndexMap::from_input_index(*input_index)?;

            // // 跳过已存在账户
            // if ApiAccountRepo::has_account_id(
            //     &pool,
            //     wallet_address,
            //     account_index_map.account_id,
            //     api_wallet_type,
            // )
            // .await?
            // {
            //     // current_id += 1;
            //     continue;
            // }

            // 收集每个index创建的资产键
            let asset_keys = ApiChainDomain::init_chains_api_assets(
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
                api_wallet_type,
            )
            .await?;

            all_asset_keys.extend(asset_keys);

            // 实时更新任务的created_indices（如果有batch_id）
            if let Some(batch_id) = &batch_id {
                // 查找相关任务
                let tasks = TaskQueueRepo::get_tasks_with_request_body_and_task_name(
                    &pool,
                    wallet_database::entities::task_queue::TaskName::Known(
                        wallet_database::entities::task_queue::KnownTaskName::AwmCmdAddrExpand,
                    ),
                    &format!("\"batchId\":\"{}\"", batch_id),
                    &[0, 1], // 查找等待中和处理中的任务
                )
                .await?;

                for task in tasks {
                    if let Ok(mut remark) = ExpandStatus::load_or_fix_remark(&task).await {
                        // 将当前索引添加到created_indices
                        remark.created_indices.insert(*input_index);

                        // 更新任务备注
                        let updated_remark = wallet_utils::serde_func::serde_to_string(&remark)?;
                        TaskQueueRepo::update_task_remark(&pool, &task.id, &updated_remark).await?;

                        tracing::info!(
                            "实时更新扩容任务created_indices: task_id={}, batch_id={}, index={}",
                            task.id,
                            batch_id,
                            *input_index
                        );
                    }
                }
            }

            created_count += 1;
        }
        if created_count > 0 {
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

            // 最后一次性更新所有资产
            if !all_asset_keys.is_empty() {
                let asset_calc_actor_manager = crate::context::CONTEXT
                    .get()
                    .unwrap()
                    .get_global_asset_calc_actor_manager()
                    .await?;

                tracing::info!("批量更新所有资产，共 {} 个资产", all_asset_keys.len());
                asset_calc_actor_manager.update_assets(&all_asset_keys).await?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use alloy::signers::local::PrivateKeySigner;
    use wallet_crypto::{EncryptedJsonDecryptor, KeystoreJsonDecryptor};

    async fn test_keystore_key() -> Result<(), Box<dyn std::error::Error>> {
        let key = KeystoreJsonDecryptor.decrypt("q1111111".as_bytes(), r#"{"crypto":{"cipher":"aes-128-ctr","cipherparams":{"iv":"cafaaf94330ae23b8a8eb64660d42740"},"ciphertext":"19e4fee3686f858bc45946665ee751a9964ef956d06ecee2f7a90021bd946529","kdf":"argon2id","kdfparams":{"dklen":32,"time_cost":5,"memory_cost":131072,"parallelism":8,"salt":[63,15,27,159,163,164,60,107,41,155,135,165,52,165,224,219,52,197,122,0,161,45,75,23,49,198,4,140,1,67,182,207]},"mac":"faf334de5be2b30526a8755980372718aad9b477b52753bde820cb6673bba7a9"},"id":"83577d8c-af30-44e6-9f06-5e616b0ac2be","version":3}"#)?;
        let h = hex::encode(key);
        let _: PrivateKeySigner = h.parse().map_err(|_| {
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
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
