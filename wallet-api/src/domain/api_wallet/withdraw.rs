use crate::{
    domain::{api_wallet::bill::ApiBillDomain, coin::CoinDomain}, messaging::notify::api_wallet::WithdrawFront, request::{
        api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq, ApiWithdrawReq},
        transaction::{BaseTransferReq, TransferReq},
    }, ApiWalletError,
    BusinessError,
    FrontendNotifyEvent,
    NotifyEvent,
};
use rust_decimal::Decimal;
use std::str::FromStr;
use wallet_database::{
    entities::{api_bill::ApiBillKind, api_wallet::ApiWalletType, api_withdraw::ApiWithdrawStatus},
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
            params.with_token(coin.token_address.clone(), coin.decimals, &coin.symbol);

            let transfer_req = ApiTransferReq { base: params, password: "q1111111".to_string() };

            // 发交易
            let tx_hash = ApiBillDomain::transfer(transfer_req, ApiBillKind::Transfer).await?;
            ApiWithdrawRepo::update_api_withdraw_tx_status(
                &pool,
                &req.trade_no,
                &tx_hash,
                ApiWithdrawStatus::SendingTx,
            )
            .await?;
        }
        Ok(())
    }
}
