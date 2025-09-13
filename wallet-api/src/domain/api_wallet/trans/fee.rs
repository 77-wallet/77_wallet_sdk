use crate::{
    ApiWalletError, BusinessError, FrontendNotifyEvent, NotifyEvent,
    messaging::notify::api_wallet::FeeFront, request::api_wallet::trans::ApiTransferFeeReq,
};
use wallet_database::{
    entities::{api_fee::ApiFeeStatus, api_wallet::ApiWalletType},
    repositories::{api_fee::ApiFeeRepo, api_wallet::ApiWalletRepo},
};

pub struct ApiFeeDomain {}

impl ApiFeeDomain {
    pub(crate) async fn transfer_fee(req: &ApiTransferFeeReq) -> Result<(), crate::ServiceError> {
        // 验证金额是否需要输入密码
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        // 获取钱包
        let wallet = ApiWalletRepo::find_by_uid(&pool, &req.uid, Some(ApiWalletType::Withdrawal))
            .await?
            .ok_or(BusinessError::ApiWallet(ApiWalletError::NotFound))?;

        // 获取账号
        // let from_account =
        //     ApiAccountRepo::find_one_by_address_chain_code(&req.from, &req.chain_code, &pool)
        //         .await?
        //         .ok_or(BusinessError::ApiWallet(ApiWalletError::NotFoundAccount))?;

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

        let data = NotifyEvent::Fee(FeeFront {
            uid: req.uid.to_string(),
            from_addr: req.from.to_string(),
            to_addr: req.to.to_string(),
            value: req.value.to_string(),
        });
        FrontendNotifyEvent::new(data).send().await?;

        let _ = crate::context::CONTEXT
            .get()
            .unwrap()
            .get_global_processed_fee_tx_handle()?
            .submit_tx(&req.trade_no)
            .await;
        Ok(())
    }

    pub async fn confirm_withdraw_tx(trade_no: &str) -> Result<(), crate::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        ApiFeeRepo::update_api_fee_status(&pool, trade_no, ApiFeeStatus::Success).await?;
        let _ = crate::context::CONTEXT
            .get()
            .unwrap()
            .get_global_processed_withdraw_tx_handle()?
            .submit_confirm_report_tx(trade_no)
            .await;
        Ok(())
    }
}
