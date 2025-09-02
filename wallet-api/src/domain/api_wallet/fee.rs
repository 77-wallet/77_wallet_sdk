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
    request::api_wallet::trans::{ApiTransferFeeReq, ApiTransferReq},
};
use rust_decimal::Decimal;
use std::str::FromStr;
use wallet_database::{
    entities::{api_fee::ApiFeeStatus, api_wallet::ApiWalletType},
    repositories::{api_account::ApiAccountRepo, api_fee::ApiFeeRepo, api_wallet::ApiWalletRepo},
};

pub struct ApiFeeDomain {}

impl ApiFeeDomain {
    pub(crate) async fn transfer_fee(req: &ApiTransferFeeReq) -> Result<(), crate::ServiceError> {
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

        ApiFeeRepo::upsert_api_fee(
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
        tracing::info!("upsert_api_fee ------------------- 5:");

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
            ApiFeeRepo::update_api_fee_status(&pool, &req.trade_no, ApiFeeStatus::Init).await?;
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

        tracing::info!("transfer ------------------- 10:");

        if let Some(request_id) = params.base.request_resource_id {
            let backend = crate::manager::Context::get_global_backend_api()?;
            let _ = backend.delegate_complete(&request_id).await;
        }

        Ok(resp)
    }

    pub async fn confirm_transfer_fee_tx_report(trade_no: &str) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        ApiFeeRepo::update_api_fee_status(&pool, trade_no, ApiFeeStatus::ReceivedTxReport).await?;
        Ok(())
    }

    pub async fn confirm_withdraw_tx(trade_no: &str) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        ApiFeeRepo::update_api_fee_status(&pool, trade_no, ApiFeeStatus::Success).await?;
        Ok(())
    }
}
