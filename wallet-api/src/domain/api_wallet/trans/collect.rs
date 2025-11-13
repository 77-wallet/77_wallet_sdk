use crate::{
    error::{business::api_wallet::ApiWalletError, service::ServiceError},
    messaging::notify::{FrontendNotifyEvent, api_wallet::CollectFront, event::NotifyEvent},
    request::api_wallet::trans::ApiCollectReq,
};
use wallet_database::{
    entities::api_collect::ApiCollectStatus,
    repositories::api_wallet::{collect::ApiCollectRepo, wallet::ApiWalletRepo},
};
use wallet_transport_backend::request::api_wallet::transaction::{
    TransAckType, TransEventAckReq, TransType,
};

pub struct ApiCollectDomain {}

impl ApiCollectDomain {
    pub(crate) async fn collect_v2(
        req: &ApiCollectReq,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let wallet = ApiWalletRepo::find_by_uid(&pool, &req.uid).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::ApiWalletError::NotFoundAccount,
            ),
        )?;

        let res = ApiCollectRepo::get_api_collect_by_trade_no(&pool, &req.trade_no).await;
        if res.is_err() {
            ApiCollectRepo::upsert_api_collect(
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
                ApiCollectStatus::Init,
            )
            .await?;

            tracing::info!(trade_no=%req.trade_no, "upsert_api_collect  ------------------- 5: ",);

            let data = NotifyEvent::Collect(CollectFront {
                uid: req.uid.to_string(),
                from_addr: req.from.to_string(),
                to_addr: req.to.to_string(),
                value: req.value.to_string(),
            });
            FrontendNotifyEvent::new(data).send().await?;
        } else {
            tracing::warn!(trade_no=%req.trade_no, "collect tx found");
        }

        // fix: 2186
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let trans_event_req =
            TransEventAckReq::new(&req.trade_no, TransType::Col, TransAckType::Tx);
        backend.trans_event_ack(&trans_event_req).await?;

        // 可能发交易
        let handles = crate::context::CONTEXT.get().unwrap().get_global_handles().await;
        if let Some(handles) = handles.upgrade() {
            handles.get_global_processed_collect_tx_handle().submit_tx(&req.trade_no).await?;
        }
        Ok(())
    }

    pub async fn recover(trade_no: &str) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        ApiCollectRepo::update_api_collect_next_status_and_err(
            &pool,
            trade_no,
            ApiCollectStatus::InsufficientBalance,
            ApiCollectStatus::Init,
            "recover",
        )
        .await?;

        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let trans_event_req =
            TransEventAckReq::new(trade_no, TransType::Col, TransAckType::TxFeeRes);
        backend.trans_event_ack(&trans_event_req).await?;

        let handles = crate::context::CONTEXT.get().unwrap().get_global_handles().await;
        if let Some(handles) = handles.upgrade() {
            handles.get_global_processed_collect_tx_handle().submit_tx(trade_no).await?;
        };

        Ok(())
    }

    pub async fn confirm_tx(
        trade_no: &str,
        status: bool,
        fail_type: i32,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let tx = ApiCollectRepo::get_api_collect_by_trade_no(&pool, trade_no).await?;
        if status {
            if tx.status == ApiCollectStatus::Success
                || tx.status == ApiCollectStatus::ConfirmSuccessReport
            {
                tracing::warn!(trade_no=%trade_no, "collect confirmation repeat");
                return Ok(());
            }
            let rows_affected = ApiCollectRepo::update_api_collect_next_status(
                &pool,
                trade_no,
                ApiCollectStatus::SendingTxReport,
                ApiCollectStatus::Success,
            )
            .await?;
            if rows_affected != 1 {
                tracing::error!(
                    trade_no = trade_no,
                    "api_collect_next_status returned 1 rows_affected"
                );
                return Err(ServiceError::Business(ApiWalletError::StatusNotMatched.into()));
            }
        } else {
            if tx.status == ApiCollectStatus::Failure
                || tx.status == ApiCollectStatus::ConfirmFailureReport
            {
                tracing::warn!(trade_no=%trade_no, "collect confirmation repeat");
                return Ok(());
            }
            if tx.status == ApiCollectStatus::InsufficientBalance && fail_type == 2 {
                let rows_affected = ApiCollectRepo::update_api_collect_next_status_and_err(
                    &pool,
                    trade_no,
                    ApiCollectStatus::InsufficientBalance,
                    ApiCollectStatus::Failure,
                    "confirm transfer fee failed insufficient balance",
                )
                .await?;
                if rows_affected != 1 {
                    tracing::error!(
                        trade_no = trade_no,
                        "api_collect_next_status returned 1 rows_affected"
                    );
                    return Err(ServiceError::Business(ApiWalletError::StatusNotMatched.into()));
                }
            } else {
                let rows_affected = ApiCollectRepo::update_api_collect_next_status(
                    &pool,
                    trade_no,
                    ApiCollectStatus::SendingTxReport,
                    ApiCollectStatus::Failure,
                )
                .await?;
                if rows_affected != 1 {
                    tracing::error!(
                        trade_no = trade_no,
                        "api_collect_next_status returned 1 rows_affected"
                    );
                    return Err(ServiceError::Business(ApiWalletError::StatusNotMatched.into()));
                }
            }
        }

        let handles = crate::context::CONTEXT.get().unwrap().get_global_handles().await;
        if let Some(handles) = handles.upgrade() {
            handles
                .get_global_processed_collect_tx_handle()
                .submit_confirm_report_tx(trade_no)
                .await?;
        }

        Ok(())
    }
}
