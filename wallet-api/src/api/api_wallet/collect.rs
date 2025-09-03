use wallet_database::entities::api_collect::ApiCollectEntity;

use crate::{api::ReturnType, service::api_wallet::collect::CollectService};

impl crate::WalletManager {
    pub async fn get_api_collect_order_list(&self) -> ReturnType<Vec<ApiCollectEntity>> {
        CollectService::new().get_collect_order_list().await?.into()
    }

    pub async fn api_collect_order(
        &self,
        from: &str,
        to: &str,
        value: &str,
        chain_code: &str,
        token_address: Option<String>,
        symbol: &str,
        trade_no: &str,
        trade_type: u8,
        uid: &str,
    ) -> ReturnType<()> {
        CollectService::new()
            .collect_order(
                from,
                to,
                value,
                chain_code,
                token_address,
                symbol,
                trade_no,
                trade_type,
                uid,
            )
            .await?
            .into()
    }
}

#[cfg(test)]
mod test {

    // #[tokio::test]
    // async fn test_create_api_account() -> Result<()> {
    //     wallet_utils::init_test_log();
    //     // 修改返回类型为Result<(), anyhow::Error>
    //     let (wallet_manager, _test_params) = get_manager().await?;

    //     let wallet_address = "0x6F0e4B9F7dD608A949138bCE4A29e076025b767F";
    //     let wallet_password = "q1111111";
    //     let index = None;
    //     let name = "666";
    //     let is_default_name = true;
    //     let api_wallet_type = ApiWalletType::SubAccount;

    //     let req = CreateApiAccountReq::new(
    //         wallet_address,
    //         wallet_password,
    //         index,
    //         name,
    //         is_default_name,
    //         api_wallet_type,
    //     );
    //     let res = wallet_manager.create_api_account(req).await;
    //     tracing::info!("res: {res:?}");
    //     Ok(())
    // }
}
