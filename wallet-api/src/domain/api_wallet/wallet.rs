use wallet_crypto::{
    EncryptedJsonDecryptor as _, EncryptedJsonGenerator as _, KeystoreJsonDecryptor,
    KeystoreJsonGenerator,
};
use wallet_database::{
    entities::api_wallet::ApiWalletType,
    repositories::{
        ResourcesRepo,
        api_account::ApiAccountRepo,
        api_wallet::ApiWalletRepo,
        chain::{ChainRepo, ChainRepoTrait as _},
        coin::{CoinRepo, CoinRepoTrait as _},
        wallet::WalletRepo,
    },
};
use wallet_transport_backend::request::{AddressBatchInitReq, TokenQueryPriceReq};

use crate::{
    domain::{app::config::ConfigDomain, chain::ChainDomain},
    infrastructure::task_queue::{BackendApiTask, BackendApiTaskData, CommonTask, task::Tasks},
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
        let pool = crate::Context::get_global_sqlite_pool()?;
        let phrase = wallet_utils::serde_func::serde_to_vec(&phrase)?;

        let rng = rand::thread_rng();
        let mut generator = KeystoreJsonGenerator::new(rng.clone(), algorithm.clone());
        let phrase = generator.generate(password.as_bytes(), &phrase)?;
        let phrase = wallet_utils::serde_func::serde_to_string(&phrase)?;
        let seed =
            KeystoreJsonGenerator::new(rng, algorithm).generate(password.as_bytes(), seed)?;
        let seed = wallet_utils::serde_func::serde_to_string(&seed)?;

        ApiWalletRepo::upsert(
            &pool,
            &uid,
            wallet_name,
            wallet_address,
            &phrase,
            &seed,
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
        let pool = crate::Context::get_global_sqlite_pool()?;

        Ok(WalletRepo::detail(&pool, address).await?.is_some())
    }

    pub(crate) async fn unbind_uid(uid: &str) -> Result<(), crate::ServiceError> {
        let pool = crate::Context::get_global_sqlite_pool()?;
        let api_wallet = ApiWalletRepo::find_by_uid(&pool, uid, Some(ApiWalletType::SubAccount))
            .await?
            .ok_or(crate::BusinessError::ApiWallet(
                crate::ApiWalletError::NotFound,
            ))?;
        ApiWalletRepo::upbind_uid(&pool, &api_wallet.address, ApiWalletType::SubAccount).await?;

        Ok(())
    }

    pub(crate) async fn create_account(
        wallet_address: &str,
        wallet_password: &str,
        index: Option<i32>,
        name: &str,
        is_default_name: bool,
        number: u32,
        api_wallet_type: ApiWalletType,
    ) -> Result<(), crate::ServiceError> {
        let pool = crate::Context::get_global_sqlite_pool()?;
        let api_wallet = ApiWalletRepo::find_by_address(&pool, wallet_address, api_wallet_type)
            .await?
            .ok_or(crate::BusinessError::ApiWallet(
                crate::ApiWalletError::NotFound,
            ))?;
        // 获取种子
        let seed = ApiWalletDomain::decrypt_seed(wallet_password, &api_wallet.seed).await?;

        // 获取默认链和币
        let default_chain_list = ChainRepo::get_chain_list(&pool).await?;
        let default_coins_list = CoinRepo::default_coin_list(&pool).await?;

        // 如果有指定派生路径，就获取该链的所有chain_code
        let chains: Vec<String> = default_chain_list
            .iter()
            .map(|chain| chain.chain_code.clone())
            .collect();

        let mut created_count = 0;
        let mut current_id = if let Some(idx) = index {
            wallet_utils::address::AccountIndexMap::from_input_index(idx)?.account_id
        } else {
            1
        };

        let mut req: TokenQueryPriceReq = TokenQueryPriceReq(Vec::new());
        let mut address_batch_init_task_data = AddressBatchInitReq(Vec::new());
        // let mut subkeys = Vec::<wallet_tree::file_ops::BulkSubkey>::new();
        while created_count < number {
            // 跳过已存在账户
            if ApiAccountRepo::has_account_id(&pool, wallet_address, current_id, api_wallet_type)
                .await?
            {
                current_id += 1;
                continue;
            }

            // 构造 index map
            let account_index_map =
                wallet_utils::address::AccountIndexMap::from_account_id(current_id)?;

            ChainDomain::init_chains_api_assets(
                &default_coins_list,
                &mut req,
                &mut address_batch_init_task_data,
                // &mut subkeys,
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
            current_id += 1;
        }

        let address_batch_init_task_data = BackendApiTaskData::new(
            wallet_transport_backend::consts::endpoint::ADDRESS_BATCH_INIT,
            &address_batch_init_task_data,
        )?;
        Tasks::new()
            .push(CommonTask::QueryCoinPrice(req))
            .push(BackendApiTask::BackendApi(address_batch_init_task_data))
            .send()
            .await?;

        Ok(())
    }
}
