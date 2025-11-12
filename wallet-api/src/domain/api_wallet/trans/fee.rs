use crate::{
    error::{
        business::{
            BusinessError,
            api_wallet::{ApiWalletError, wallet::WalletError},
        },
        service::ServiceError,
    },
    messaging::notify::{FrontendNotifyEvent, api_wallet::FeeFront, event::NotifyEvent},
    request::api_wallet::trans::ApiTransferFeeReq,
};
use wallet_database::{
    entities::api_fee::ApiFeeStatus,
    repositories::api_wallet::{fee::ApiFeeRepo, wallet::ApiWalletRepo},
};
use wallet_transport_backend::request::api_wallet::transaction::{
    TransAckType, TransEventAckReq, TransType,
};

pub struct ApiFeeDomain {}

impl ApiFeeDomain {
    pub(crate) async fn transfer_fee(
        req: &ApiTransferFeeReq,
    ) -> Result<(), crate::error::service::ServiceError> {
        // 验证金额是否需要输入密码
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        // 获取钱包
        let wallet = ApiWalletRepo::find_by_uid(&pool, &req.uid).await?.ok_or(
            BusinessError::ApiWallet(ApiWalletError::Wallet(WalletError::NotFound.into())),
        )?;

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
                &req.symbol.to_uppercase(),
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
        } else {
            tracing::warn!(trade_no=%req.trade_no, "fee tx found");
        }

        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let trans_event_req =
            TransEventAckReq::new(&req.trade_no, TransType::ColFee, TransAckType::Tx);
        backend.trans_event_ack(&trans_event_req).await?;

        let handles = crate::context::CONTEXT.get().unwrap().get_global_handles().await;
        if let Some(handles) = handles.upgrade() {
            handles.get_global_processed_fee_tx_handle().submit_tx(&req.trade_no).await?;
        }
        Ok(())
    }

    pub async fn confirm_tx(trade_no: &str, status: bool) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let tx = ApiFeeRepo::get_api_fee_by_trade_no(&pool, trade_no).await?;
        if status {
            if tx.status == ApiFeeStatus::Success || tx.status == ApiFeeStatus::ConfirmSuccessReport
            {
                tracing::warn!(trade_no=%trade_no, "fee confirmation repeat");
                return Ok(());
            }
        } else {
            if tx.status == ApiFeeStatus::Failure || tx.status == ApiFeeStatus::ConfirmFailureReport
            {
                tracing::warn!(trade_no=%trade_no, "fee confirmation repeat");
                return Ok(());
            }
        }
        let next_status: ApiFeeStatus =
            if status { ApiFeeStatus::Success } else { ApiFeeStatus::Failure };
        let rows_affected = ApiFeeRepo::update_api_fee_next_status(
            &pool,
            trade_no,
            ApiFeeStatus::SendingTxReport,
            next_status,
        )
        .await?;
        if rows_affected != 1 {
            return Err(ServiceError::Business(ApiWalletError::StatusNotMatched.into()));
        }

        let handles = crate::context::CONTEXT.get().unwrap().get_global_handles().await;
        if let Some(handles) = handles.upgrade() {
            handles.get_global_processed_fee_tx_handle().submit_confirm_report_tx(trade_no).await?;
        }
        Ok(())
    }
}
