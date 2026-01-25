use std::collections::HashMap;

use wallet_database::{
    entities::api_chain::ApiChainEntity,
    repositories::api_wallet::{
        account::ApiAccountRepo, assets::ApiAssetsRepo, chain::ApiChainRepo,
    },
};

use crate::{
    context::Context,
    domain::api_wallet::{chain::ApiChainDomain, coin::ApiCoinDomain, wallet::ApiWalletDomain},
    response_vo::standard_wallet::chain::ChainAssets,
};

pub struct ApiChainService {
    ctx: &'static Context,
}

impl ApiChainService {
    pub fn new(ctx: &'static Context) -> Self {
        Self { ctx }
    }

    pub async fn get_chain_assets_list(
        self,
        address: &str,
        account_id: Option<u32>,
        chain_list: HashMap<String, String>,
    ) -> Result<Vec<ChainAssets>, crate::error::service::ServiceError> {
        let pool = self.ctx.core_pool()?;
        let token_currencies = ApiCoinDomain::get_api_token_currencies().await?;

        let mut account_addresses = Vec::<String>::new();

        // 获取钱包下的这个账户的所有地址
        let accounts =
            ApiAccountRepo::list_by_wallet_address_account_id(&pool, Some(address), account_id)
                .await?;
        for account in accounts {
            if !account_addresses.iter().any(|address| address == &account.address) {
                account_addresses.push(account.address);
            }
        }

        let datas =
            ApiAssetsRepo::get_api_assets_by_address(&pool, account_addresses, None).await?;
        let datas = datas
            .into_iter()
            .filter(|data| {
                chain_list
                    .get(&data.chain_code)
                    .is_some_and(|token_address| data.token_address == *token_address)
            })
            .collect();

        let chains = ApiChainRepo::get_chain_list(&pool).await?;

        let res = token_currencies.calculate_api_chain_assets_list(datas, chains).await?;
        tracing::info!("[get_chain_assets_list] res: {res:?}");
        Ok(res)
    }

    pub async fn get_hot_chain_list(
        self,
    ) -> Result<Vec<ApiChainEntity>, crate::error::service::ServiceError> {
        let pool = self.ctx.core_pool()?;
        let res = ApiChainRepo::get_chain_list(&pool).await?;

        Ok(res)
    }

    pub async fn sync_chains(&self) -> Result<Vec<String>, crate::error::service::ServiceError> {
        ApiChainDomain::sync_chains().await
        // let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        // let app_version = ConfigDomain::get_app_version().await?;
        // let chain_list = backend.api_wallet_chain_list(&app_version.app_version).await?;
        // ApiChainDomain::upsert_multi_api_chain_than_toggle(chain_list).await
    }

    pub async fn sync_wallet_chain_data(&self) -> Result<(), crate::error::service::ServiceError> {
        let password = ApiWalletDomain::get_passwd().await?;
        ApiChainDomain::sync_wallet_chain_data(&password).await
    }
}
