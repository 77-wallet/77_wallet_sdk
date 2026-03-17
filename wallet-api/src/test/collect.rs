use std::sync::Arc;

use wallet_database::{
    ApiFundsDbPool, ApiWalletDbPool, entities::api_collect::ApiCollectEntity,
    repositories::api_wallet::collect::ApiCollectRepo,
};
use wallet_transport_backend::request::api_wallet::transaction::{
    TransStatus, TransType, TxExecReceiptUploadReq,
};

use crate::{
    error::service::ServiceError,
    infrastructure::api_trans::collect::{
        AddressLockManager,
        shadow::{
            DispatcherConfig, ScannerConfig, ShadowAdvancer, ShadowCollectWorker, ShadowDispatcher,
            ShadowScanner, SideEffectCommand, SideEffectWorker,
        },
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

    let remark = if matches!(upload_status, TransStatus::Success)
        || req.err_msg.as_deref().unwrap_or("").is_empty()
    {
        ""
    } else {
        req.err_msg.as_deref().unwrap_or("")
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

pub async fn scan_and_dispatch_collect_tx_exec_receipt_once(
    collect_pool: ApiFundsDbPool,
    core_pool: ApiWalletDbPool,
) -> Result<Option<String>, ServiceError> {
    let (intent_tx, mut intent_rx) = tokio::sync::mpsc::channel(8);
    let scanner_intent_tx = intent_tx.clone();
    let (diagnose_tx, _diagnose_rx) = tokio::sync::mpsc::channel(8);
    let advancer =
        Arc::new(ShadowAdvancer::new(collect_pool.clone(), intent_tx.clone(), Some(diagnose_tx)));
    let side_effect_worker =
        Arc::new(SideEffectWorker::new(collect_pool.clone(), core_pool.clone(), advancer.clone()));
    let shadow_worker = Arc::new(ShadowCollectWorker::new(
        collect_pool.clone(),
        core_pool,
        Arc::new(AddressLockManager::new()),
        advancer,
    ));
    let dispatcher = ShadowDispatcher::new(
        collect_pool.clone(),
        DispatcherConfig::default(),
        shadow_worker,
        side_effect_worker,
        intent_tx,
    );
    let scanner = ShadowScanner::new(
        collect_pool.clone(),
        ScannerConfig { scan_interval: std::time::Duration::from_secs(60), max_items_per_scan: 8 },
        scanner_intent_tx,
        None,
    );

    scanner.scan_round().await;

    let mut matched: Option<(
        crate::infrastructure::api_trans::collect::shadow::CollectIntent,
        String,
    )> = None;
    while let Ok(intent) = intent_rx.try_recv() {
        let trade_no = match &intent {
            crate::infrastructure::api_trans::collect::shadow::CollectIntent::SideEffect(
                crate::infrastructure::api_trans::collect::shadow::SideEffectIntent::UploadTxExecReceipt(
                    trade_no,
                ),
            ) => trade_no.clone(),
            _ => continue,
        };
        matched = Some((intent, trade_no));
        break;
    }

    let Some((intent, trade_no)) = matched else {
        return Ok(None);
    };

    dispatcher
        .handle_intent(intent)
        .await
        .map_err(|e| ServiceError::Parameter(format!("dispatcher test helper failed: {e}")))?;

    for _ in 0..40 {
        let rec = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
            .await
            .map_err(|e| ServiceError::Database(e.into()))?;
        if rec.tx_exec_receipt_uploaded_at.is_some() {
            return Ok(Some(trade_no));
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    Ok(Some(trade_no))
}
