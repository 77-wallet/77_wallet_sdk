use crate::{
    ApiWalletError, BusinessError, FrontendNotifyEvent, NotifyEvent,
    domain::{
        api_wallet::{
            account::ApiAccountDomain,
            adapter_factory::{API_ADAPTER_FACTORY, ApiChainAdapterFactory},
        },
        chain::TransferResp,
    },
    messaging::notify::api_wallet::WithdrawFront,
    request::api_wallet::trans::{ApiTransferReq, ApiWithdrawReq},
};
use rust_decimal::Decimal;
use std::str::FromStr;
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
        let wallet = ApiWalletRepo::find_by_uid(&pool, &req.uid, Some(ApiWalletType::Withdrawal))
            .await?
            .ok_or(BusinessError::ApiWallet(ApiWalletError::NotFound))?;

        // 获取账号
        let from_account =
            ApiAccountRepo::find_one_by_address_chain_code(&req.from, &req.chain_code, &pool)
                .await?
                .ok_or(BusinessError::ApiWallet(ApiWalletError::NotFoundAccount))?;

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
        let value = Decimal::from_str(&req.value).unwrap();
        if value < Decimal::from(10) {
            tracing::info!("transfer ------------------- 9:");
            ApiWithdrawRepo::update_api_withdraw_status(
                &pool,
                &req.trade_no,
                ApiWithdrawStatus::AuditPass,
            )
            .await?;
        }
        Ok(())
    }

    /// transfer
    pub async fn transfer(params: ApiTransferReq) -> Result<TransferResp, crate::ServiceError> {
        tracing::info!("transfer ------------------- 7:");
        let private_key = ApiAccountDomain::get_private_key(
            &params.base.from,
            &params.base.chain_code,
            &params.password,
        )
        .await?;

        tracing::info!("transfer ------------------- 8:");

        let adapter = API_ADAPTER_FACTORY
            .get_or_init(|| async { ApiChainAdapterFactory::new().await.unwrap() })
            .await
            .get_transaction_adapter(params.base.chain_code.as_str())
            .await?;

        let resp = adapter.transfer(&params, private_key).await?;

        Ok(resp)
    }

    pub async fn confirm_withdraw_tx_report(trade_no: &str) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        ApiWithdrawRepo::update_api_withdraw_status(&pool, trade_no, ApiWithdrawStatus::ReceivedTxReport).await?;
        Ok(())
    }

    pub async fn confirm_withdraw_tx(trade_no: &str) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        ApiWithdrawRepo::update_api_withdraw_status(&pool, trade_no, ApiWithdrawStatus::Success).await?;
        Ok(())
    }
}
