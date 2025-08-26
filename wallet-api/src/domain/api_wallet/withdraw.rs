use std::str::FromStr;
use alloy::primitives::U256;
use rust_decimal::Decimal;
use wallet_chain_interact::eth::FeeSetting;
use wallet_database::entities::api_bill::ApiBillKind;
use wallet_database::entities::api_wallet::ApiWalletType;
use wallet_database::repositories::api_account::ApiAccountRepo;
use wallet_database::repositories::api_wallet::ApiWalletRepo;
use wallet_database::repositories::api_withdraw::ApiWithdrawRepo;
use crate::messaging::notify::api_wallet::WithdrawFront;
use crate::{ApiWalletError, BusinessError, FrontendNotifyEvent, NotifyEvent};
use crate::domain::api_wallet::transaction::ApiChainTransDomain;
use crate::request::{
    transaction::{BaseTransferReq, TransferReq, },
    api_wallet::trans::ApiTransReq,
};
use crate::ServiceError::Business;

pub struct ApiWithdrawDomain {}

impl ApiWithdrawDomain {
    pub(crate) async fn withdraw(req: &ApiTransReq) -> Result<(), crate::ServiceError> {
        // 验证金额是否需要输入密码

        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        // 获取钱包
        let wallet = ApiWalletRepo::find_by_uid(&pool, &req.uid, Some(ApiWalletType::Withdrawal)).await?.ok_or(
            BusinessError::ApiWallet(ApiWalletError::NotFound)
        )?;

        // 获取账号
        let from_account = ApiAccountRepo::find_one_by_address_chain_code(&req.from, &req.chain_code, &pool).await?.ok_or(
            BusinessError::ApiWallet(ApiWalletError::NotFoundAccount)
        )?;

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
        ).await?;
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
            let mut params = BaseTransferReq::new(
                &req.from,
                &req.to.to_string(),
                &req.value.to_string(),
                &req.chain_code.to_string(),
                &req.symbol.to_string(),
            );
            params.with_token(req.token_address.clone());

            let req = TransferReq {
                base: params,
                password: "q1111111".to_string(),
                fee_setting: "".to_string(),
                signer: None,
            };
            // 发交易
            let tx_hash =
                ApiChainTransDomain::transfer(req, ApiBillKind::Transfer).await?;
        }
        Ok(())
    }
}
