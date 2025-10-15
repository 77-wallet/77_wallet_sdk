use crate::{
    error::business::{BusinessError, api_wallet::ApiWalletError},
    messaging::notify::{FrontendNotifyEvent, api_wallet::FeeFront, event::NotifyEvent},
    request::api_wallet::trans::ApiTransferFeeReq,
};
use wallet_database::{
    entities::{api_fee::ApiFeeStatus, api_wallet::ApiWalletType},
    repositories::{api_wallet::fee::ApiFeeRepo, api_wallet::wallet::ApiWalletRepo},
};
use wallet_transport_backend::request::api_wallet::transaction::{TransAckType, TransEventAckReq, TransType};

pub struct ApiFeeDomain {}

impl ApiFeeDomain {
    pub(crate) async fn transfer_fee(
        req: &ApiTransferFeeReq,
    ) -> Result<(), crate::error::service::ServiceError> {
        // 验证金额是否需要输入密码
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        // 获取钱包
        let wallet = ApiWalletRepo::find_by_uid(&pool, &req.uid)
            .await?
            .ok_or(BusinessError::ApiWallet(ApiWalletError::NotFound))?;

        let res = ApiFeeRepo::get_api_fee_by_trade_no(&pool, &req.trade_no).await;
        if res.is_err() {
            ApiFeeRepo::upsert_api_fee(
                &pool,
                &req.uid,
                &wallet.name,
                &req.from,
                &req.to,
                &req.value,
                &req.validate,
                &req.chain_code,
                req.token_address.clone(),
                &req.symbol,
                &req.trade_no,
                req.trade_type,
            ).await?;
            tracing::info!("upsert_api_fee ------------------- 5:");

            let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
            let trans_event_req = TransEventAckReq::new(&req.trade_no, TransType::ColFee, TransAckType::Tx);
            backend.trans_event_ack(&trans_event_req).await?;

            if let Some(handles) = crate::context::CONTEXT.get().unwrap().get_global_handles().upgrade()
            {
                handles.get_global_processed_fee_tx_handle().submit_tx(&req.trade_no).await?;
            }
        }
        Ok(())
    }

    pub async fn confirm_tx(
        trade_no: &str,
        status: ApiFeeStatus,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        ApiFeeRepo::update_api_fee_status(&pool, trade_no, status, "confirm").await?;
        if let Some(handles) = crate::context::CONTEXT.get().unwrap().get_global_handles().upgrade()
        {
            handles.get_global_processed_fee_tx_handle().submit_confirm_report_tx(trade_no).await?;
        }
        Ok(())
    }
}
