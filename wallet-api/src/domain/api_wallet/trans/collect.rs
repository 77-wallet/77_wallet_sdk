use crate::{
    domain::{
        api_wallet::{
            adapter_factory::{ApiChainAdapterFactory, API_ADAPTER_FACTORY},
            trans::ApiTransDomain,
            wallet::ApiWalletDomain,
        },
        chain::transaction::ChainTransDomain,
        coin::CoinDomain,
    },
    messaging::notify::{api_wallet::WithdrawFront, event::NotifyEvent, FrontendNotifyEvent},
    request::api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq, ApiWithdrawReq},
};
use wallet_database::{
    entities::{api_collect::ApiCollectStatus, api_wallet::ApiWalletType},
    repositories::{api_collect::ApiCollectRepo, api_wallet::ApiWalletRepo},
};
use wallet_transport_backend::request::api_wallet::{
    strategy::ChainConfig,
    transaction::{ServiceFeeUploadReq, TransAckType, TransEventAckReq, TransType},
};
use wallet_types::chain::chain::ChainCode;
use wallet_utils::{conversion, unit};
use wallet_database::repositories::api_withdraw::ApiWithdrawRepo;

pub struct ApiCollectDomain {}

impl ApiCollectDomain {
    pub(crate) async fn collect_v2(
        req: &ApiWithdrawReq,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let wallet = ApiWalletRepo::find_by_uid(&pool, &req.uid).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::ApiWalletError::NotFound,
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
                &req.chain_code,
                req.token_address.clone(),
                &req.symbol,
                &req.trade_no,
                req.trade_type,
                ApiCollectStatus::Init,
            )
                .await?;

            tracing::info!("upsert_api_collect  ------------------- 5: ",);

            let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
            let trans_event_req =
                TransEventAckReq::new(&req.trade_no, TransType::Col, TransAckType::Tx);
            backend.trans_event_ack(&trans_event_req).await?;

            let data = NotifyEvent::Withdraw(WithdrawFront {
                uid: req.uid.to_string(),
                from_addr: req.from.to_string(),
                to_addr: req.to.to_string(),
                value: req.value.to_string(),
            });
            FrontendNotifyEvent::new(data).send().await?;

            // 可能发交易
            if let Some(handles) = crate::context::CONTEXT.get().unwrap().get_global_handles().upgrade()
            {
                handles.get_global_processed_collect_tx_handle().submit_tx(&req.trade_no).await?;
            }
        }

        Ok(())
    }

    pub async fn confirm_tx(
        trade_no: &str,
        status: ApiCollectStatus,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        ApiCollectRepo::update_api_collect_status(&pool, trade_no, status, "confirm").await?;

        if let Some(handles) = crate::context::CONTEXT.get().unwrap().get_global_handles().upgrade()
        {
            handles
                .get_global_processed_collect_tx_handle()
                .submit_confirm_report_tx(trade_no)
                .await?;
        }
        Ok(())
    }
}
