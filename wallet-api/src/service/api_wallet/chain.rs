use std::collections::HashMap;

use wallet_database::repositories::api_wallet::{
    account::ApiAccountRepo, assets::ApiAssetsRepo, chain::ApiChainRepo,
};

use crate::{context::Context, domain::coin::CoinDomain, response_vo::chain::ChainAssets};

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
        let pool = self.ctx.get_global_sqlite_pool()?;
        let token_currencies = CoinDomain::get_token_currencies_v2().await?;

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

        Ok(res)
    }
}
