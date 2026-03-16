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
        let pool = self.ctx.api_wallet_pool()?;
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
                    .is_some_and(|token_address| data.token_address.as_db_str() == token_address)
            })
            .collect();

        let chains = ApiChainRepo::get_chain_list(&pool).await?;

        let res = token_currencies.calculate_api_chain_assets_list(datas, chains).await?;
        tracing::info!("[get_chain_assets_list] res: {res:?}");
        Ok(res)
    }

    pub async fn get_chain_assets_list_v2(
        self,
        address: &str,
        account_id: Option<u32>,
        chain_list: HashMap<String, String>,
    ) -> Result<Vec<ChainAssets>, crate::error::service::ServiceError> {
        let pool = self.ctx.api_wallet_pool()?;
        let token_currencies = ApiCoinDomain::get_api_token_currencies().await?;

        let mut account_addresses = std::collections::HashSet::<String>::new();

        // 获取钱包下的这个账户的所有地址
        let accounts =
            ApiAccountRepo::list_by_wallet_address_account_id(&pool, Some(address), account_id)
                .await?;
        for account in accounts {
            account_addresses.insert(account.address);
        }

        let account_addresses: Vec<_> = account_addresses.into_iter().collect();

        let datas =
            ApiAssetsRepo::get_api_assets_by_address(&pool, account_addresses, None).await?;

        // 过滤 datas
        let filtered_datas: Vec<_> = datas
            .into_iter()
            .filter(|data| {
                chain_list
                    .get(&data.chain_code)
                    .is_some_and(|token_address| data.token_address.as_db_str() == token_address)
            })
            .collect();

        let chains = ApiChainRepo::get_chain_list(&pool).await?;

        // 使用原有计算层
        let calculated =
            token_currencies.calculate_api_chain_assets_list(filtered_datas, chains).await?;

        // 构建分组映射
        let mut group_map: HashMap<(String, String), Vec<ChainAssets>> = HashMap::new();
        for asset in calculated {
            let key = (asset.chain_code.clone(), asset.token_address.clone());
            group_map.entry(key).or_default().push(asset);
        }

        // 构建请求代币集合
        let request_tokens: std::collections::HashSet<(String, String)> =
            chain_list.iter().map(|(chain, token)| (chain.clone(), token.clone())).collect();

        // 构建 token_meta_map
        let mut token_meta_map: HashMap<
            (String, String),
            wallet_transport_backend::response_vo::coin::TokenCurrency,
        > = HashMap::new();
        for (id, currency) in token_currencies.iter() {
            let token_address = id.token_address.as_db_str().to_string();
            let key = (id.chain_code.clone(), token_address);
            if request_tokens.contains(&key) {
                token_meta_map.insert(key, currency.clone());
            }
        }

        // 对请求进行排序，确保顺序稳定
        let mut reqs: Vec<_> = chain_list.iter().map(|(c, t)| (c.clone(), t.clone())).collect();
        reqs.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

        // 构建结果
        let mut res = Vec::with_capacity(reqs.len());
        let currency = crate::domain::app::config::ConfigDomain::get_currency().await?;

        for (chain_code, token_address) in reqs {
            let key = (chain_code.clone(), token_address.clone());

            if let Some(assets) = group_map.remove(&key) {
                res.extend(assets);
            } else {
                // 构建零资产实例
                let (name, symbol, unit_price) = if let Some(token_meta) = token_meta_map.get(&key)
                {
                    (token_meta.name.clone(), token_meta.code.clone(), token_meta.price)
                } else {
                    // 未同步的代币
                    ("Unknown Token".to_string(), "UNKNOWN".to_string(), None)
                };

                let balance = crate::response_vo::standard_wallet::account::BalanceInfo {
                    amount: 0.0,
                    currency: currency.clone(),
                    unit_price: unit_price,
                    fiat_value: Some(0.0),
                };

                res.push(ChainAssets {
                    chain_code: chain_code.clone(),
                    name,
                    symbol,
                    address: "".to_string(),
                    token_address: token_address.clone(),
                    balance,
                    is_multisig: 0,
                    asset_quantity_ratio: 0.0,
                });
            }
        }

        // 统一计算资产比例
        recompute_asset_ratios(&mut res);

        tracing::info!("[get_chain_assets_list] assets_count={}", res.len());
        Ok(res)
    }

    pub async fn get_hot_chain_list(
        self,
    ) -> Result<Vec<ApiChainEntity>, crate::error::service::ServiceError> {
        let pool = self.ctx.api_wallet_pool()?;
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

/// 统一计算资产比例
fn recompute_asset_ratios(assets: &mut [crate::response_vo::standard_wallet::chain::ChainAssets]) {
    let total_fiat: f64 = assets.iter().map(|asset| asset.balance.fiat_value.unwrap_or(0.0)).sum();

    if total_fiat > 0.0 {
        for asset in assets {
            let fiat_value = asset.balance.fiat_value.unwrap_or(0.0);
            asset.asset_quantity_ratio = fiat_value / total_fiat;
        }
    }
}
