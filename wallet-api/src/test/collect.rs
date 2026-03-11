use std::sync::Arc;

use wallet_database::{ApiFundsDbPool, ApiWalletDbPool, entities::api_collect::ApiCollectEntity};
use wallet_transport_backend::request::api_wallet::transaction::{
    TransStatus, TransType, TxExecReceiptUploadReq,
};

use crate::{
    error::service::ServiceError,
    infrastructure::api_trans::collect::shadow::{
        ShadowAdvancer, SideEffectCommand, SideEffectWorker,
    },
};

pub fn build_collect_tx_exec_receipt_payload(
    req: &ApiCollectEntity,
    trade_no: &str,
) -> TxExecReceiptUploadReq {
    let upload_status = if req.transaction_time.is_some() || req.last_broadcast_at.is_some() {
        TransStatus::Success
    } else {
        TransStatus::Fail
    };

    let remark = if matches!(upload_status, TransStatus::Success) || req.err_msg.is_empty() {
        ""
    } else {
        &req.err_msg
    };

    TxExecReceiptUploadReq::new(
        Some(&req.from_addr),
        Some(&req.to_addr),
        trade_no,
        TransType::Col,
        req.tx_hash.as_deref(),
        upload_status,
        remark,
    )
}

pub async fn upload_collect_tx_exec_receipt_via_worker(
    collect_pool: ApiFundsDbPool,
    core_pool: ApiWalletDbPool,
    trade_no: &str,
) -> Result<(), ServiceError> {
    let (intent_tx, _intent_rx) = tokio::sync::mpsc::channel(8);
    let (diagnose_tx, _diagnose_rx) = tokio::sync::mpsc::channel(8);
    let advancer =
        Arc::new(ShadowAdvancer::new(collect_pool.clone(), intent_tx, Some(diagnose_tx)));
    let worker = SideEffectWorker::new(collect_pool, core_pool, advancer);
    worker.handle(SideEffectCommand::UploadTxExecReceipt(trade_no.to_string())).await
}

pub async fn upload_collect_tx_exec_receipt_via_backend(
    req: &ApiCollectEntity,
    trade_no: &str,
) -> Result<(), ServiceError> {
    let payload = build_collect_tx_exec_receipt_payload(req, trade_no);
    let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
    backend_api.upload_tx_exec_receipt(&payload).await?;
    Ok(())
}
