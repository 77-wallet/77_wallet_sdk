use crate::{
    api::ReturnType,
    manager::WalletManager,
    service::{api_wallet::coin::ApiCoinService, coin::CoinService},
};

impl WalletManager {
    // 热门币种列表,排除传入钱包已经添加的币种
    pub async fn api_hot_coin_list(
        &self,
        wallet_address: &str,
        account_id: u32,
        chain_code: Option<String>,
        keyword: Option<&str>,
        page: i64,
        page_size: i64,
    ) -> ReturnType<wallet_database::pagination::Pagination<crate::response_vo::coin::CoinInfo>>
    {
        ApiCoinService::get_hot_coin_list(
            wallet_address,
            Some(account_id),
            chain_code,
            keyword,
            None,
            page,
            page_size,
        )
        .await
    }

    // api出款钱包自定义币种
    pub async fn api_customize_coin(
        &self,
        address: &str,
        account_id: Option<u32>,
        chain_code: &str,
        token_address: &str,
        protocol: Option<String>,
    ) -> ReturnType<()> {
        CoinService::new(self.repo_factory.resource_repo())
            .customize_coin(
                address,
                account_id,
                chain_code,
                token_address.to_string(),
                protocol,
                false,
            )
            .await
    }
}
