use wallet_database::{
    entities::api_wallet::ApiWalletType,
    repositories::{
        ResourcesRepo, api_account::ApiAccountRepo, api_wallet::ApiWalletRepo,
        chain::ChainRepoTrait as _, coin::CoinRepoTrait as _,
    },
};
use wallet_transport_backend::request::{
    AddressBatchInitReq, TokenQueryPriceReq, api_wallet::address::UploadAllocatedAddressesReq,
};

use crate::domain::{
    api_wallet::wallet::ApiWalletDomain, chain::ChainDomain, wallet::WalletDomain,
};

pub struct ApiAccountService {
    pub repo: ResourcesRepo,
}

impl ApiAccountService {
    pub fn new(repo: ResourcesRepo) -> Self {
        Self { repo }
    }

    pub async fn upload_allocated_addresses(
        self,
        wallet_address: &str,
        addresses: Vec<String>,
    ) -> Result<(), crate::ServiceError> {
        let backend_api = crate::Context::get_global_backend_api()?;

        let req = UploadAllocatedAddressesReq::new(wallet_address, addresses);
        backend_api.upload_allocated_addresses(&req).await?;

        Ok(())
    }

    pub async fn create_account(
        self,
        wallet_address: &str,
        wallet_password: &str,
        // derivation_path: Option<String>,
        index: Option<i32>,
        name: &str,
        is_default_name: bool,
        api_wallet_type: ApiWalletType,
    ) -> Result<(), crate::ServiceError> {
        let mut tx = self.repo;
        let pool = crate::Context::get_global_sqlite_pool()?;

        let dirs = crate::manager::Context::get_global_dirs()?;

        WalletDomain::validate_password(wallet_password).await?;
        // 根据钱包地址查询是否有钱包
        let api_wallet = ApiWalletRepo::find_by_address(&pool, wallet_address, api_wallet_type)
            .await?
            .ok_or(crate::BusinessError::ApiWallet(
                crate::ApiWalletError::NotFound,
            ))?;

        // 获取种子

        let seed = ApiWalletDomain::decrypt_seed(wallet_password, &api_wallet.seed).await?;

        // 获取默认链和币
        let default_chain_list = tx.get_chain_list().await?;
        let default_coins_list = tx.default_coin_list().await?;

        // 如果有指定派生路径，就获取该链的所有chain_code
        let chains: Vec<String> = default_chain_list
            .iter()
            .map(|chain| chain.chain_code.clone())
            .collect();

        // 获取该账户的最大索引，并加一
        let account_index_map = if let Some(index) = index {
            let index = wallet_utils::address::AccountIndexMap::from_input_index(index)?;
            if ApiAccountRepo::has_account_id(
                &pool,
                &api_wallet.address,
                index.account_id,
                api_wallet_type,
            )
            .await?
            {
                return Err(crate::ServiceError::Business(
                    crate::BusinessError::Account(crate::AccountError::AlreadyExist),
                ));
            };
            index
        } else if let Some(max_account) =
            ApiAccountRepo::account_detail_by_max_id_and_wallet_address(
                &pool,
                &api_wallet.address,
                api_wallet_type,
            )
            .await?
        {
            wallet_utils::address::AccountIndexMap::from_account_id(max_account.account_id + 1)?
        } else {
            wallet_utils::address::AccountIndexMap::from_account_id(1)?
        };

        let mut req: TokenQueryPriceReq = TokenQueryPriceReq(Vec::new());
        let mut subkeys = Vec::<wallet_tree::file_ops::BulkSubkey>::new();

        let mut address_batch_init_task_data = AddressBatchInitReq(Vec::new());

        ChainDomain::init_chains_api_assets(
            &mut tx,
            &default_coins_list,
            &mut req,
            &mut address_batch_init_task_data,
            &mut subkeys,
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

        // let wallet_tree_strategy = ConfigDomain::get_wallet_tree_strategy().await?;
        // let wallet_tree = wallet_tree_strategy.get_wallet_tree(&dirs.wallet_dir)?;
        // let algorithm = ConfigDomain::get_keystore_kdf_algorithm().await?;

        // let tron_address = subkeys
        //     .iter()
        //     .find(|s| s.chain_code == chain_code::TRON)
        //     .map(|s| s.address.clone());

        // KeystoreApi::initialize_child_keystores(
        //     wallet_tree,
        //     subkeys,
        //     dirs.get_subs_dir(wallet_address)?,
        //     wallet_password,
        //     algorithm,
        // )?;

        // // let device_bind_address_task_data =
        // //     domain::app::DeviceDomain::gen_device_bind_address_task_data().await?;

        // // 恢复多签账号、多签队列
        // let mut body = RecoverDataBody::new(&wallet.uid);
        // if let Some(tron_address) = tron_address {
        //     body.tron_address = Some(tron_address);
        // };
        // let task = Task::Common(CommonTask::QueryCoinPrice(req));

        // let address_batch_init_task_data = BackendApiTaskData::new(
        //     wallet_transport_backend::consts::endpoint::ADDRESS_BATCH_INIT,
        //     &address_batch_init_task_data,
        // )?;
        // Tasks::new()
        //     .push(task)
        //     // .push(Task::BackendApi(BackendApiTask::BackendApi(
        //     //     device_bind_address_task_data,
        //     // )))
        //     .push(Task::Common(CommonTask::RecoverMultisigAccountData(body)))
        //     .push(Task::BackendApi(BackendApiTask::BackendApi(
        //         address_batch_init_task_data,
        //     )))
        //     .send()
        //     .await?;

        // for task in address_init_task_data {
        //     Tasks::new()
        //         .push(Task::BackendApi(BackendApiTask::BackendApi(task)))
        //         .send()
        //         .await?;
        // }

        Ok(())
    }
}
