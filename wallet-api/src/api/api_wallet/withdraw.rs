use crate::{
    api::ReturnType, manager::WalletManager, service::api_wallet::withdraw::WithdrawService,
};
use wallet_database::{
    entities::api_withdraw::{ApiWithdrawEntity, ApiWithdrawStatus},
    pagination::Pagination,
};

impl WalletManager {
    pub async fn list_api_withdraw_order(&self, uid: &str) -> ReturnType<Vec<ApiWithdrawEntity>> {
        WithdrawService::new(self.ctx).list_withdraw_order(uid).await
    }

    pub async fn page_api_withdraw_order_with_init_status(
        &self,
        uid: &str,
        init_status: u8,
        status: Vec<u8>,
        page: i64,
        page_size: i64,
    ) -> ReturnType<Pagination<ApiWithdrawEntity>> {
        let s = status.iter().map(|it| ApiWithdrawStatus::try_from(it.clone()).unwrap()).collect();
        let init_status = ApiWithdrawStatus::try_from(init_status)?;
        WithdrawService::new(self.ctx)
            .page_withdraw_order_with_init_status(uid, init_status, s, page, page_size)
            .await
    }

    pub async fn page_api_withdraw_order(
        &self,
        uid: &str,
        status: Vec<u8>,
        page: i64,
        page_size: i64,
    ) -> ReturnType<Pagination<ApiWithdrawEntity>> {
        let s = status.iter().map(|it| ApiWithdrawStatus::try_from(it.clone()).unwrap()).collect();
        WithdrawService::new(self.ctx).page_withdraw_order(uid, s, page, page_size).await
    }

    // 测试
    pub async fn api_withdrawal_order(
        &self,
        from: &str,
        to: &str,
        value: &str,
        validate: &str,
        chain_code: &str,
        token_address: Option<String>,
        symbol: &str,
        trade_no: &str,
        trade_type: u8,
        uid: &str,
    ) -> ReturnType<()> {
        WithdrawService::new(self.ctx)
            .withdrawal_order(
                from,
                to,
                value,
                validate,
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
        WithdrawService::new(self.ctx).sign_withdrawal_order(order_id).await
    }

    pub async fn reject_api_withdrawal_order(&self, order_id: &str) -> ReturnType<()> {
        WithdrawService::new(self.ctx).reject_withdrawal_order(order_id).await
    }
}

#[cfg(test)]
mod test {
    use crate::test::env::get_manager;
    use anyhow::Result;
    use wallet_database::entities::api_withdraw::ApiWithdrawStatus;

    #[tokio::test]
    async fn test_reject_api_withdrawal_order() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        wallet_manager.init_api_swap().await?;

        let order_id = "W2019474521680683008";

        let res = wallet_manager.reject_api_withdrawal_order(order_id).await;
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_page_api_withdraw_order() -> Result<()> {
        wallet_utils::init_test_log();
        let (wallet_manager, _test_params) = get_manager().await?;
        wallet_manager.init_api_swap().await?;

        let uid = "5bdb1b748bb617d6683f57565b1493cfa5f9e45f3086daf265ca2e0cd325c15e";
        let res = wallet_manager
            .page_api_withdraw_order(
                uid,
                vec![ApiWithdrawStatus::AuditReject as u8, ApiWithdrawStatus::SendingTxFailed as u8],
                0,
                10,
            )
            .await?;
        for e in &res.data {
            let res = serde_json::to_string(e).unwrap();
            tracing::info!("-------- {:?}", res);
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_sign_api_withdrawal_order() -> Result<()> {
        wallet_utils::init_test_log();
        let (wallet_manager, _test_params) = get_manager().await?;
        wallet_manager.init_api_swap().await?;

        let order_id = "W2019486010038722560";
        let res = wallet_manager.sign_api_withdrawal_order(order_id).await;
        tracing::info!("sign_api_withdrawal_order result: {:?}", res);
        Ok(())
    }
}
