use std::sync::Arc;

use wallet_database::{ApiTransactionDbPool, ApiWalletDbPool};

use crate::{
    error::service::ServiceError,
    infrastructure::api_trans::withdraw::{
        WithdrawChainIntent, WithdrawIntent, WithdrawShadowScanner, WithdrawShadowScannerConfig,
        WithdrawShadowSideEffectCommand, WithdrawShadowSideEffectWorker, WithdrawSideEffectIntent,
    },
};

pub async fn send_resource_result_ack_via_worker(
    tx_pool: ApiTransactionDbPool,
    core_pool: ApiWalletDbPool,
    resource_trade_no: &str,
) -> Result<(), ServiceError> {
    let (intent_tx, _intent_rx) = tokio::sync::mpsc::channel(8);
    let scanner = Arc::new(WithdrawShadowScanner::new(
        tx_pool.clone(),
        WithdrawShadowScannerConfig {
            scan_interval: std::time::Duration::from_secs(60),
            max_items_per_scan: 8,
        },
        intent_tx,
        None,
    ));
    let worker = WithdrawShadowSideEffectWorker::new(tx_pool, core_pool, scanner);
    worker
        .handle(WithdrawShadowSideEffectCommand::SendResourceResultAck(
            resource_trade_no.to_string(),
        ))
        .await
}

pub async fn upload_resource_tx_exec_receipt_via_worker(
    tx_pool: ApiTransactionDbPool,
    core_pool: ApiWalletDbPool,
    resource_trade_no: &str,
) -> Result<(), ServiceError> {
    let (intent_tx, _intent_rx) = tokio::sync::mpsc::channel(8);
    let scanner = Arc::new(WithdrawShadowScanner::new(
        tx_pool.clone(),
        WithdrawShadowScannerConfig {
            scan_interval: std::time::Duration::from_secs(60),
            max_items_per_scan: 8,
        },
        intent_tx,
        None,
    ));
    let worker = WithdrawShadowSideEffectWorker::new(tx_pool, core_pool, scanner);
    worker
        .handle(WithdrawShadowSideEffectCommand::UploadResourceTxExecReceipt(
            resource_trade_no.to_string(),
        ))
        .await
}

pub async fn scan_withdraw_intent_labels_once(
    tx_pool: ApiTransactionDbPool,
) -> Result<Vec<String>, ServiceError> {
    let (intent_tx, mut intent_rx) = tokio::sync::mpsc::channel(8);
    let scanner = WithdrawShadowScanner::new(
        tx_pool,
        WithdrawShadowScannerConfig {
            scan_interval: std::time::Duration::from_secs(60),
            max_items_per_scan: 8,
        },
        intent_tx,
        None,
    );

    scanner.scan_round().await;

    let mut labels = Vec::new();
    while let Ok(intent) = intent_rx.try_recv() {
        let label = match intent {
            WithdrawIntent::Chain(WithdrawChainIntent::EvalResourceGate(_)) => {
                "EvalResourceGate".to_string()
            }
            WithdrawIntent::Chain(WithdrawChainIntent::ExecuteResourceDelegation(_)) => {
                "ExecuteResourceDelegation".to_string()
            }
            WithdrawIntent::Chain(WithdrawChainIntent::BuildTx(_)) => "BuildTx".to_string(),
            WithdrawIntent::Chain(WithdrawChainIntent::BroadcastTx(_)) => "BroadcastTx".to_string(),
            WithdrawIntent::Chain(WithdrawChainIntent::RecoverTx(_)) => "RecoverTx".to_string(),
            WithdrawIntent::SideEffect(WithdrawSideEffectIntent::SendTxAck(_)) => {
                "SendTxAck".to_string()
            }
            WithdrawIntent::SideEffect(WithdrawSideEffectIntent::SendResourceResultAck(_)) => {
                "SendResourceResultAck".to_string()
            }
            WithdrawIntent::SideEffect(WithdrawSideEffectIntent::SendResourceTaskAck(_)) => {
                "SendResourceTaskAck".to_string()
            }
            WithdrawIntent::SideEffect(WithdrawSideEffectIntent::UploadResourceTxExecReceipt(
                _,
            )) => "UploadResourceTxExecReceipt".to_string(),
            WithdrawIntent::SideEffect(WithdrawSideEffectIntent::SendTxResAck(_)) => {
                "SendTxResAck".to_string()
            }
            WithdrawIntent::SideEffect(WithdrawSideEffectIntent::UploadTxExecReceipt(_)) => {
                "UploadTxExecReceipt".to_string()
            }
        };
        labels.push(label);
    }

    Ok(labels)
}
