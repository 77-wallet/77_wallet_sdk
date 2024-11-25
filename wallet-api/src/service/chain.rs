use crate::{
    domain::{self, coin::CoinDomain},
    response_vo::chain::ChainAssets,
};
use wallet_database::{
    entities::chain::{ChainCreateVo, ChainEntity},
    repositories::{
        account::AccountRepoTrait, assets::AssetsRepoTrait, chain::ChainRepoTrait,
        coin::CoinRepoTrait, ResourcesRepo, TransactionTrait as _,
    },
    sqlite::logic::chain::ChainWithNode,
};

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
        node_id: &str,
        protocols: &[String],
        main_symbol: &str,
    ) -> Result<(), crate::error::ServiceError> {
        let input = ChainCreateVo::new(name, chain_code, node_id, protocols, main_symbol);
        let mut tx = self.repo.begin_transaction().await?;

        let _res = tx.add(input).await?;

        tx.commit_transaction().await?;

        Ok(())
    }

    pub async fn set_chain_node(
        self,
        chain_code: &str,
        node_id: &str,
    ) -> Result<(), crate::error::ServiceError> {
        let mut tx = self.repo.begin_transaction().await?;
        tx.set_chain_node(chain_code, node_id).await?;

        tx.commit_transaction().await?;

        Ok(())
    }

    pub async fn get_chain_list(self) -> Result<Vec<ChainEntity>, crate::error::ServiceError> {
        let mut tx = self.repo.begin_transaction().await?;

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
        let mut tx = self.repo.begin_transaction().await?;
        let res = tx.get_chain_node_list().await?;

        tx.commit_transaction().await?;
        Ok(res)
    }

    pub async fn get_protocol_list(
        self,
        chain_code: &str,
    ) -> Result<Option<ChainEntity>, crate::error::ServiceError> {
        let mut tx = self.repo.begin_transaction().await?;
        let res = ChainRepoTrait::detail(&mut tx, chain_code).await?;

        tx.commit_transaction().await?;
        Ok(res)
    }

    // pub async fn calculate_chain_assets_list(
    //     &self,
    //     datas: Vec<AssetsEntity>,
    // ) -> Result<Vec<ChainAssets>, crate::ServiceError> {
    //     let mut res = Vec::new();

    //     for data in datas {
    //         let token_currency =
    //             super::get_current_coin_unit_price_option(&data.symbol, &data.chain_code).await?;

    //         let balance = (token_currency, &data).try_into()?;
    //         res.push(crate::response_vo::chain::ChainAssets {
    //             chain_code: data.chain_code,
    //             address: data.address,
    //             balance,
    //             symbol: data.symbol,
    //         })
    //     }
    //     Ok(res)
    // }

    // pub async fn get_chain_list(
    //     &self,
    // ) -> Result<Vec<ChainEntity>, crate::ServiceError> {
    //     crate::manager::Context::get_global_sqlite_context()?
    //         .chain_list()
    //         .await
    //         .map_err(|e| crate::ServiceError::System(crate::SystemError::Database(e)))
    // }

    // pub async fn get_chain_list_with_node_info(
    //     &self,
    // ) -> Result<Vec<ChainWithNode>, crate::ServiceError>
    // {
    //     crate::manager::Context::get_global_sqlite_context()?
    //         .chain_list_with_node_info()
    //         .await
    //         .map_err(|e| crate::ServiceError::System(crate::SystemError::Database(e)))
    // }

    pub async fn get_chain_list_by_address_account_id_symbol(
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
                    domain::multisig::MultisigDomain::account_by_address(address, &pool).await?;
                account_addresses.push(account.address);
            } else {
                // 获取钱包下的这个账户的所有地址
                let accounts = tx
                    .get_account_list_by_wallet_address_and_account_id(Some(address), account_id)
                    .await?;
                // let condition = Vec::new();
                // let multisig_account = MultisigAccountEntity::list(condition, pool.as_ref())
                //     .await
                //     .unwrap();
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

        tracing::info!("account_addresses: {:?}", account_addresses);
        let datas = tx
            .get_assets_by_address(account_addresses, None, Some(symbol), is_multisig)
            .await?;
        // let datas = service
        //     .asset_service
        //     .get_chain_assets_by_account_address_chain_code_symbol(
        //         account_addresses,
        //         None,
        //         Some(symbol),
        //     )
        //     .await?;
        tracing::info!("datas: {:?}", datas);
        let chains = tx.get_chain_list().await?;
        let res = token_currencies
            .calculate_chain_assets_list(datas, chains)
            .await?;

        // self.cx()
        //     .chain_list_by_chain_codes(chain_code)
        //     .await
        //     .map_err(|e| crate::Error::System(crate::SystemError::Database(e)))
        // res.mark_multichain_assets();

        Ok(res)
    }
    // pub async fn init_chains_info(self) -> Result<(), crate::ServiceError> {
    //     let tx = self.repo;
    //     let list = crate::default_data::chains::init_default_chains_list()?;
    //     for default_chain in &list.chains {
    //         let input = wallet_database::dao::chain::ChainCreateVo {
    //             name: "name".to_string(),
    //             chain_code: default_chain.chain_code.to_string(),
    //             node_id: default_chain.node_id.to_string(),
    //             status: 1,
    //         };
    //         let res = tx.add(input).await?;
    //     }
    //     // self.add_chain("name", "address", Some(1)).await
    //     Ok(())
    // }
}
