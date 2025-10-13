use crate::{
    api::ReturnType, manager::WalletManager, service::api_wallet::withdraw::WithdrawService,
};
use wallet_database::{entities::api_withdraw::ApiWithdrawEntity, pagination::Pagination};

impl WalletManager {
    pub async fn list_api_withdraw_order(&self, uid: &str) -> ReturnType<Vec<ApiWithdrawEntity>> {
        WithdrawService::new().list_withdraw_order(uid).await
    }

    pub async fn page_api_withdraw_order(
        &self,
        uid: &str,
        status: Option<u8>,
        page: i64,
        page_size: i64,
    ) -> ReturnType<Pagination<ApiWithdrawEntity>> {
        WithdrawService::new().page_withdraw_order(uid, status, page, page_size).await
    }

    // 测试
    pub async fn api_withdrawal_order(
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
        WithdrawService::new()
            .withdrawal_order(
                from,
                to,
                value,
                chain_code,
                token_address,
                symbol,
                trade_no,
                trade_type,
                uid,
                1,
            )
            .await
    }

    pub async fn sign_api_withdrawal_order(&self, order_id: &str) -> ReturnType<()> {
        WithdrawService::new().sign_withdrawal_order(order_id).await
    }

    pub async fn reject_api_withdrawal_order(&self, order_id: &str) -> ReturnType<()> {
        WithdrawService::new().reject_withdrawal_order(order_id).await
    }
}

#[cfg(test)]
mod test {
    use anyhow::Result;

    use crate::test::env::get_manager;

    #[tokio::test]
    async fn test_api_withdrawal_order() -> Result<()> {
        wallet_utils::init_test_log();
        let (wallet_manager, _test_params) = get_manager().await?;

        wallet_manager
            .api_withdrawal_order(
                "0x6F0e4B9F7dD608A949138bCE4A29e076025b767F",
                "0x6F0e4B9F7dD608A949138bCE4A29e076025b767F",
                "10000000000000000000000000000000000000000000000000000000000000000",
                "ETH",
                None,
                "ETH",
                "1234567890",
                1,
                "cf43155d5b80eb73beb6ce3c7224214f3ed33fcc2d4ebfe5764d36e1ffac8cce",
            )
            .await?;

        tracing::info!("test_api_withdrawal_order success");
        Ok(())
    }

    #[tokio::test]
    async fn test_page_api_withdrawal_order() -> Result<()> {
        wallet_utils::init_test_log();
        let (wallet_manager, _test_params) = get_manager().await?;

        let res = wallet_manager
            .page_api_withdraw_order(
                "cf43155d5b80eb73beb6ce3c7224214f3ed33fcc2d4ebfe5764d36e1ffac8cce",
                None,
                0,
                10,
            )
            .await;

        tracing::info!("test_page_api_withdrawal_order success: {:?}", res);
        Ok(())
    }
}
