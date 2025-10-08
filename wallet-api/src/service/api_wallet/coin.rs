use crate::domain::coin::CoinDomain;
use wallet_database::{
    entities::coin::CoinId,
    repositories::{
        api_wallet::{account::ApiAccountRepo, assets::ApiAssetsRepo},
        coin::CoinRepo,
    },
};

pub struct ApiCoinService;

impl ApiCoinService {
    // 热门币种列表 排除某个钱包已经添加的币种
    pub async fn get_hot_coin_list(
        wallet_address: &str,
        account_id: Option<u32>,
        chain_code: Option<String>,
        keyword: Option<&str>,
        is_multisig: Option<bool>,
        page: i64,
        page_size: i64,
    ) -> Result<
        wallet_database::pagination::Pagination<crate::response_vo::coin::CoinInfo>,
        crate::error::service::ServiceError,
    > {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        // 地址里列表
        let accounts =
            ApiAccountRepo::list_by_wallet_address(&pool, wallet_address, account_id, None).await?;
        let addresses =
            accounts.into_iter().map(|address| address.address).collect::<Vec<String>>();

        // 获取资产
        let assets = ApiAssetsRepo::get_chain_assets_by_address_chain_code_symbol(
            &pool,
            addresses,
            chain_code.clone(),
            None,
            is_multisig,
        )
        .await?;

        let exclude = assets
            .iter()
            .map(|asset| CoinId {
                symbol: asset.symbol.clone(),
                chain_code: asset.chain_code.clone(),
                token_address: asset.token_address(),
            })
            .collect::<Vec<CoinId>>();

        let list = CoinRepo::hot_coin_list_symbol_not_in(
            &pool, &exclude, chain_code, keyword, page, page_size,
        )
        .await?;

        let data = CoinDomain::merge_coin_to_list(list.data, keyword.is_some())?;

        let res = wallet_database::pagination::Pagination {
            page,
            page_size,
            total_count: list.total_count,
            data: data.0,
        };

        Ok(res)
    }
}
