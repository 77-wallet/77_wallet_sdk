use crate::{
    ApiWalletError, BusinessError, FrontendNotifyEvent, NotifyEvent,
    messaging::notify::api_wallet::WithdrawFront, request::api_wallet::trans::ApiWithdrawReq,
};
use wallet_database::{
    entities::{api_wallet::ApiWalletType, api_withdraw::ApiWithdrawStatus},
    repositories::{
        api_account::ApiAccountRepo, api_wallet::ApiWalletRepo, api_withdraw::ApiWithdrawRepo,
    },
};

pub struct ApiWithdrawDomain {}

impl ApiWithdrawDomain {
    pub(crate) async fn withdraw(req: &ApiWithdrawReq) -> Result<(), crate::ServiceError> {
        // 验证金额是否需要输入密码
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        // 获取钱包
        let wallet = ApiWalletRepo::find_by_uid(&pool, &req.uid)
            .await?
            .ok_or(BusinessError::ApiWallet(ApiWalletError::NotFound))?;

        // 获取账号
        let from_account =
            ApiAccountRepo::find_one_by_address_chain_code(&req.from, &req.chain_code, &pool)
                .await?
                .ok_or(BusinessError::ApiWallet(ApiWalletError::NotFoundAccount))?;

        let status = if req.trade_type == 1 {
            ApiWithdrawStatus::AuditPass
        } else {
            ApiWithdrawStatus::Init
        };
        ApiWithdrawRepo::upsert_api_withdraw(
            &pool,
            &req.uid,
            &wallet.name,
            &req.from,
            &req.to,
            &req.value,
            &req.chain_code,
            req.token_address.clone(),
            &req.symbol,
            &req.trade_no,
            req.trade_type,
            status,
        )
        .await?;
        tracing::info!("upsert_api_withdraw ------------------- 5:");

        let data = NotifyEvent::Withdraw(WithdrawFront {
            uid: req.uid.to_string(),
            from_addr: req.from.to_string(),
            to_addr: req.to.to_string(),
            value: req.value.to_string(),
        });
        FrontendNotifyEvent::new(data).send().await?;

        // 可能发交易

        crate::manager::Context::get_global_processed_withdraw_tx_handle()?
            .submit_tx(&req.trade_no)
            .await?;
        Ok(())
    }

    pub async fn sign_withdrawal_order(trade_no: &str) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        ApiWithdrawRepo::update_api_withdraw_status(&pool, trade_no, ApiWithdrawStatus::AuditPass)
            .await?;
        Ok(())
    }

    pub async fn reject_withdrawal_order(trade_no: &str) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        ApiWithdrawRepo::update_api_withdraw_status(
            &pool,
            trade_no,
            ApiWithdrawStatus::AuditReject,
        )
        .await?;
        Ok(())
    }

    pub async fn confirm_withdraw_tx(
        trade_no: &str,
        status: ApiWithdrawStatus,
    ) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        ApiWithdrawRepo::update_api_withdraw_status(&pool, trade_no, status).await?;

        crate::manager::Context::get_global_processed_withdraw_tx_handle()?
            .submit_confirm_report_tx(trade_no)
            .await?;
        Ok(())
    }
}
