use wallet_crypto::{
    EncryptedJsonDecryptor as _, EncryptedJsonGenerator as _, KeystoreJsonDecryptor,
    KeystoreJsonGenerator,
};
use wallet_database::{
    entities::api_wallet::ApiWalletType,
    repositories::{
        api_account::ApiAccountRepo, api_wallet::ApiWalletRepo, coin::CoinRepo, wallet::WalletRepo,
    },
};
use wallet_transport_backend::request::{
    AddressBatchInitReq, TokenQueryPriceReq, api_wallet::address::ExpandAddressReq,
};

use crate::{
    domain::{
        api_wallet::account::ApiAccountDomain, app::config::ConfigDomain, chain::ChainDomain,
    },
    infrastructure::task_queue::{BackendApiTask, BackendApiTaskData, CommonTask, task::Tasks},
    messaging::mqtt::topics::api_wallet::AddressAllockType,
};

pub struct ApiWalletDomain {}

impl ApiWalletDomain {
    pub(crate) async fn upsert_api_wallet(
        uid: &str,
        wallet_name: &str,
        wallet_address: &str,
        password: &str,
        phrase: &str,
        seed: &[u8],
        api_wallet_type: ApiWalletType,
    ) -> Result<(), crate::ServiceError> {
        let algorithm = ConfigDomain::get_keystore_kdf_algorithm().await?;
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        // let phrase = wallet_utils::serde_func::serde_to_vec(&phrase)?;

        // let rng = rand::thread_rng();
        // let mut generator = KeystoreJsonGenerator::new(rng.clone(), algorithm.clone());
        // let phrase = generator.generate(password.as_bytes(), &phrase)?;
        // let phrase = wallet_utils::serde_func::serde_to_string(&phrase)?;
        // let seed =
        //     KeystoreJsonGenerator::new(rng, algorithm).generate(password.as_bytes(), seed)?;
        // let seed = wallet_utils::serde_func::serde_to_string(&seed)?;

        let (phrase_enc, seed_enc) = {
            // rng 在这个 block 内创建并使用，block 结束时 rng 被 drop
            let rng = rand::thread_rng();

            // 用 rng 生成 phrase
            let mut gen1 = KeystoreJsonGenerator::new(rng.clone(), algorithm.clone());
            let phrase_keystore = gen1.generate(password.as_bytes(), phrase.as_bytes())?;
            let phrase_enc = wallet_utils::serde_func::serde_to_string(&phrase_keystore)?;

            // 用 rng（或其 clone）生成 seed
            let mut gen2 = KeystoreJsonGenerator::new(rng, algorithm.clone());
            let seed_keystore = gen2.generate(password.as_bytes(), seed)?;
            let seed_enc = wallet_utils::serde_func::serde_to_string(&seed_keystore)?;

            (phrase_enc, seed_enc)
        };

        ApiWalletRepo::upsert(
            &pool,
            &uid,
            wallet_name,
            wallet_address,
            &phrase_enc,
            &seed_enc,
            api_wallet_type,
        )
        .await?;

        Ok(())
    }

    pub(crate) async fn decrypt_seed(
        password: &str,
        seed: &str,
    ) -> Result<Vec<u8>, crate::ServiceError> {
        let data = KeystoreJsonDecryptor.decrypt(password.as_ref(), seed)?;
        Ok(data)
    }

    pub(crate) async fn check_normal_wallet_exist(
        address: &str,
    ) -> Result<bool, crate::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        Ok(WalletRepo::detail(&pool, address).await?.is_some())
    }

    pub(crate) async fn bind_uid(
        uid: &str,
        merchain_id: &str,
        org_app_id: &str,
    ) -> Result<(), crate::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let api_wallet = ApiWalletRepo::find_by_uid(&pool, uid)
            .await?
            .ok_or(crate::BusinessError::ApiWallet(crate::ApiWalletError::NotFound))?;
        ApiWalletRepo::update_merchant_id(&pool, &api_wallet.address, merchain_id).await?;
        ApiWalletRepo::update_app_id(&pool, &api_wallet.address, org_app_id).await?;

        Ok(())
    }

    pub(crate) async fn bind_withdraw_and_subaccount_relation(
        subaccount_uid: &str,
        withdraw_uid: &str,
    ) -> Result<(), crate::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let subaccount_uid = ApiWalletRepo::find_by_uid(&pool, subaccount_uid)
            .await?
            .ok_or(crate::BusinessError::ApiWallet(crate::ApiWalletError::NotFound))?;
        let withdraw_uid = ApiWalletRepo::find_by_uid(&pool, withdraw_uid)
            .await?
            .ok_or(crate::BusinessError::ApiWallet(crate::ApiWalletError::NotFound))?;
        ApiWalletRepo::bind_withdraw_and_subaccount_relation(
            pool.clone(),
            &subaccount_uid.address,
            &withdraw_uid.address,
        )
        .await?;

        ApiWalletRepo::bind_withdraw_and_subaccount_relation(
            pool,
            &withdraw_uid.address,
            &subaccount_uid.address,
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn unbind_uid(uid: &str) -> Result<(), crate::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let api_wallet = ApiWalletRepo::find_by_uid(&pool, uid)
            .await?
            .ok_or(crate::BusinessError::ApiWallet(crate::ApiWalletError::NotFound))?;
        ApiWalletRepo::upbind_uid(&pool, &api_wallet.address, ApiWalletType::SubAccount).await?;

        Ok(())
    }

    pub(crate) async fn create_sub_account(
        wallet_address: &str,
        password: &str,
        chains: Vec<String>,
        account_name: &str,
        is_default_name: bool,
    ) -> Result<(), crate::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        // 查询已有的账户
        let num = 3;
        let account_indices = ApiAccountRepo::get_all_account_indices(&pool).await?;
        let account_indices = ApiAccountDomain::next_account_indices(account_indices, num);
        let mut input_indices = Vec::new();
        for account_id in account_indices {
            input_indices.push(
                wallet_utils::address::AccountIndexMap::from_account_id(account_id)?.input_index,
            );
        }

        ApiWalletDomain::create_account(
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
    ) -> Result<(), crate::ServiceError> {
        ApiWalletDomain::create_account(
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

    pub(crate) async fn expand_address(
        address_allock_type: &AddressAllockType,
        index: Option<i32>,
        uid: &str,
        chain_code: &str,
    ) -> Result<(), crate::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let api_wallet = ApiWalletRepo::find_by_uid(&pool, &uid)
            .await?
            .ok_or(crate::BusinessError::ApiWallet(crate::ApiWalletError::NotFound))?;

        let password = ApiWalletDomain::get_passwd().await?;

        match address_allock_type {
            AddressAllockType::ChaBatch => {
                ApiWalletDomain::create_sub_account(
                    &api_wallet.address,
                    &password,
                    vec![chain_code.to_string()],
                    "name",
                    true,
                )
                .await?; // 查询已有的账户
            }
            AddressAllockType::ChaIndex => {
                // 扩容一个链地址
                if let Some(index) = index {
                    ApiWalletDomain::create_account(
                        &api_wallet.address,
                        &password,
                        vec![chain_code.to_string()],
                        vec![index],
                        "name",
                        true,
                        ApiWalletType::SubAccount,
                    )
                    .await?;
                }
            }
        }
        Ok(())
    }

    pub(crate) async fn create_account(
        wallet_address: &str,
        wallet_password: &str,
        chains: Vec<String>,
        input_indices: Vec<i32>,
        name: &str,
        is_default_name: bool,
        api_wallet_type: ApiWalletType,
    ) -> Result<(), crate::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let api_wallet = ApiWalletRepo::find_by_address(&pool, wallet_address)
            .await?
            .ok_or(crate::BusinessError::ApiWallet(crate::ApiWalletError::NotFound))?;
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
        let mut address_batch_init_task_data = AddressBatchInitReq(Vec::new());
        let mut expand_address_req = ExpandAddressReq::new_sdk(&api_wallet.uid);
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

            ChainDomain::init_chains_api_assets(
                &default_coins_list,
                &mut req,
                &mut address_batch_init_task_data,
                // &mut subkeys,
                &mut expand_address_req,
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
            let address_batch_init_task_data = BackendApiTaskData::new(
                wallet_transport_backend::consts::endpoint::ADDRESS_BATCH_INIT,
                &address_batch_init_task_data,
            )?;

            // let backend_api = crate::Context::get_global_backend_api()?;
            // backend_api.expand_address(&expand_address_req).await?;
            // let expand_address_task_data = BackendApiTaskData::new(
            //     wallet_transport_backend::consts::endpoint::api_wallet::ADDRESS_POOL_EXPAND,
            //     &expand_address_req,
            // )?;

            Tasks::new()
                .push(CommonTask::QueryCoinPrice(req))
                .push(BackendApiTask::BackendApi(address_batch_init_task_data))
                // .push(BackendApiTask::BackendApi(expand_address_task_data))
                .send()
                .await?;
        }

        Ok(())
    }

    pub(crate) async fn get_passwd() -> Result<String, crate::ServiceError> {
        let password = crate::infrastructure::GLOBAL_CACHE
            .get::<String>(crate::infrastructure::WALLET_PASSWORD)
            .await
            .ok_or(crate::BusinessError::ApiWallet(crate::ApiWalletError::PasswordNotCached))?;
        Ok(password)
    }

    pub(crate) async fn set_passwd(wallet_password: &str) -> Result<(), crate::ServiceError> {
        crate::infrastructure::GLOBAL_CACHE
            .set(crate::infrastructure::WALLET_PASSWORD, wallet_password)
            .await?;
        Ok(())
    }
}
