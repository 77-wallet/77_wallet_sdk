use crate::{
    domain::{self, app::config::ConfigDomain, chain::ChainDomain, coin::CoinDomain},
    infrastructure::task_queue::{BackendApiTask, BackendApiTaskData, CommonTask, Task, Tasks},
    response_vo::chain::ChainAssets,
};
use wallet_database::{
    entities::chain::{ChainCreateVo, ChainEntity, ChainWithNode},
    repositories::{
        account::AccountRepoTrait, assets::AssetsRepoTrait, chain::ChainRepoTrait,
        coin::CoinRepoTrait, ResourcesRepo, TransactionTrait as _,
    },
};
use wallet_transport_backend::request::{AddressBatchInitReq, TokenQueryPriceReq};
use wallet_tree::api::KeystoreApi;

pub struct ChainService {
    repo: ResourcesRepo,
    coin_domain: CoinDomain,
}

impl ChainService {
    pub fn new(repo: ResourcesRepo) -> Self {
        Self {
            repo,
            coin_domain: CoinDomain::new(),
        }
    }

    pub async fn add(
        self,
        name: &str,
        chain_code: &str,
        protocols: &[String],
        main_symbol: &str,
    ) -> Result<(), crate::error::ServiceError> {
        let input = ChainCreateVo::new(name, chain_code, protocols, main_symbol);
        let mut tx = self.repo;
        tx.begin_transaction().await?;

        let _res = tx.add(input).await?;

        tx.commit_transaction().await?;

        Ok(())
    }

    pub async fn set_chain_node(
        self,
        chain_code: &str,
        node_id: &str,
    ) -> Result<(), crate::error::ServiceError> {
        let mut tx = self.repo;
        tx.begin_transaction().await?;
        tx.set_chain_node(chain_code, node_id).await?;

        tx.commit_transaction().await?;

        Ok(())
    }

    pub async fn sync_chains(self) -> Result<bool, crate::error::ServiceError> {
        let backend = crate::manager::Context::get_global_backend_api()?;
        let cryptor = crate::Context::get_global_aes_cbc_cryptor()?;
        let app_version = ConfigDomain::get_app_version().await?;

        let req = wallet_transport_backend::request::ChainListReq::new(app_version.app_version);
        let chain_list = backend.chain_list(cryptor, req).await?;

        ChainDomain::upsert_multi_chain_than_toggle(chain_list).await
    }

    pub async fn sync_wallet_chain_data(
        self,
        wallet_password: &str,
    ) -> Result<(), crate::error::ServiceError> {
        let mut tx = self.repo;
        let dirs = crate::manager::Context::get_global_dirs()?;

        domain::wallet::WalletDomain::validate_password(wallet_password).await?;
        let chain_list: Vec<String> = ChainRepoTrait::get_chain_node_list(&mut tx)
            .await?
            .into_iter()
            .map(|chain| chain.chain_code)
            .collect();

        let account_wallet_mapping = tx.account_wallet_mapping().await?;
        let mut req = TokenQueryPriceReq(Vec::new());
        let coins = tx.default_coin_list().await?;

        let mut address_batch_init_task_data = AddressBatchInitReq(Vec::new());
        for wallet in account_wallet_mapping {
            let mut subkeys = Vec::<wallet_tree::file_ops::BulkSubkey>::new();
            let account_index_map =
                wallet_utils::address::AccountIndexMap::from_account_id(wallet.account_id)?;

            let seed = domain::wallet::WalletDomain::get_seed(
                dirs,
                &wallet.wallet_address,
                wallet_password,
            )
            .await?;

            ChainDomain::init_chains_assets(
                &mut tx,
                &coins,
                &mut req,
                &mut address_batch_init_task_data,
                &mut subkeys,
                &chain_list,
                &seed,
                &account_index_map,
                None,
                &wallet.uid,
                &wallet.wallet_address,
                &wallet.account_name,
                false,
            )
            .await?;

            let wallet_tree_strategy = ConfigDomain::get_wallet_tree_strategy().await?;
            let wallet_tree = wallet_tree_strategy.get_wallet_tree(&dirs.wallet_dir)?;
            let algorithm = ConfigDomain::get_keystore_kdf_algorithm().await?;
            KeystoreApi::initialize_child_keystores(
                wallet_tree,
                subkeys,
                dirs.get_subs_dir(&wallet.wallet_address)?,
                wallet_password,
                algorithm,
            )?;
        }

        // let device_bind_address_task_data =
        //     DeviceDomain::gen_device_bind_address_task_data().await?;

        let task = Task::Common(CommonTask::QueryCoinPrice(req));
        let address_init_task_data = BackendApiTaskData::new(
            wallet_transport_backend::consts::endpoint::ADDRESS_BATCH_INIT,
            &address_batch_init_task_data,
        )?;
        Tasks::new()
            .push(task)
            // .push(Task::BackendApi(BackendApiTask::BackendApi(
            //     device_bind_address_task_data,
            // )))
            .push(Task::BackendApi(BackendApiTask::BackendApi(
                address_init_task_data,
            )))
            .send()
            .await?;

        Ok(())
    }

    pub async fn get_hot_chain_list(self) -> Result<Vec<ChainEntity>, crate::error::ServiceError> {
        let mut tx = self.repo;
        tx.begin_transaction().await?;
        let res = tx.get_chain_list().await?;
        tx.commit_transaction().await?;

        Ok(res)
    }

    pub async fn get_market_chain_list(self) -> Result<Vec<String>, crate::error::ServiceError> {
        let mut tx = self.repo;
        let res = tx.get_market_chain_list().await?;
        Ok(res)
    }

    pub async fn get_chain_list_with_node_info(
        self,
    ) -> Result<Vec<ChainWithNode>, crate::error::ServiceError> {
        let mut tx = self.repo;
        tx.begin_transaction().await?;
        let res = tx.get_chain_node_list().await?;

        tx.commit_transaction().await?;
        Ok(res)
    }

    pub async fn get_protocol_list(
        self,
        chain_code: &str,
    ) -> Result<Option<ChainEntity>, crate::error::ServiceError> {
        let mut tx = self.repo;
        tx.begin_transaction().await?;
        let res = ChainRepoTrait::detail(&mut tx, chain_code).await?;

        tx.commit_transaction().await?;
        Ok(res)
    }

    pub async fn get_chain_assets_list(
        mut self,
        address: &str,
        account_id: Option<u32>,
        symbol: &str,
        is_multisig: Option<bool>,
    ) -> Result<Vec<ChainAssets>, crate::ServiceError> {
        let mut tx = self.repo;
        let token_currencies = self.coin_domain.get_token_currencies_v2(&mut tx).await?;

        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        let mut account_addresses = Vec::<String>::new();

        if let Some(is_multisig) = is_multisig {
            if is_multisig {
                // 查询多签账户下的资产
                let account =
                    domain::multisig::MultisigDomain::account_by_address(address, true, &pool)
                        .await?;
                account_addresses.push(account.address);
            } else {
                // 获取钱包下的这个账户的所有地址
                let accounts = tx
                    .get_account_list_by_wallet_address_and_account_id(Some(address), account_id)
                    .await?;
                for account in accounts {
                    if !account_addresses
                        .iter()
                        .any(|address| address == &account.address)
                    {
                        account_addresses.push(account.address);
                    }
                }
            }
        } else {
            // 获取钱包下的这个账户的所有地址
            let accounts = tx
                .get_account_list_by_wallet_address_and_account_id(Some(address), account_id)
                .await?;
            for account in accounts {
                if !account_addresses
                    .iter()
                    .any(|address| address == &account.address)
                {
                    account_addresses.push(account.address);
                }
            }
        }
        let datas = tx
            .get_assets_by_address(account_addresses, None, Some(symbol), is_multisig)
            .await?;
        let chains = tx.get_chain_list().await?;
        let res = token_currencies
            .calculate_chain_assets_list(datas, chains)
            .await?;

        Ok(res)
    }
}
