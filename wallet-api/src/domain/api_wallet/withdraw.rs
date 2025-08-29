use crate::{
    ApiWalletError, BusinessError, FrontendNotifyEvent, NotifyEvent,
    domain::{
        api_wallet::{
            account::ApiAccountDomain,
            adapter_factory::{API_ADAPTER_FACTORY, ApiChainAdapterFactory},
        },
        chain::TransferResp,
        coin::CoinDomain,
    },
    messaging::notify::api_wallet::WithdrawFront,
    request::api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq, ApiWithdrawReq},
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
        if (value < Decimal::from(10)) {
            tracing::info!("transfer ------------------- 9:");
            let coin =
                CoinDomain::get_coin(&req.chain_code, &req.symbol, req.token_address.clone())
                    .await?;

            let mut params = ApiBaseTransferReq::new(
                &req.from,
                &req.to.to_string(),
                &req.value.to_string(),
                &req.chain_code.to_string(),
            );
            let token_address = if coin.token_address.is_none() {
                None
            } else {
                let s = coin.token_address.unwrap();
                if s.is_empty() { None } else { Some(s) }
            };
            params.with_token(token_address, coin.decimals, &coin.symbol);

            let transfer_req = ApiTransferReq { base: params, password: "q1111111".to_string() };

            // 发交易
            let tx_resp = Self::transfer(transfer_req).await?;

            let resource_consume = if tx_resp.consumer.is_none() {
                "0".to_string()
            } else {
                tx_resp.consumer.unwrap().energy_used.to_string()
            };
            ApiWithdrawRepo::update_api_withdraw_tx_status(
                &pool,
                &req.trade_no,
                &tx_resp.tx_hash,
                &resource_consume,
                &tx_resp.fee,
                ApiWithdrawStatus::SendingTx,
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

        tracing::info!("transfer ------------------- 10:");

        if let Some(request_id) = params.base.request_resource_id {
            let backend = crate::manager::Context::get_global_backend_api()?;
            let _ = backend.delegate_complete(&request_id).await;
        }

        Ok(resp)
    }
}
