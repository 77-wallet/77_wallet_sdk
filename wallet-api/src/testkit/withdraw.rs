//! Test-only entrypoints for withdraw worker and scanner steps.
//!
//! Integration tests use these wrappers when the behavior under test lives
//! behind crate-private workflow types.

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

pub async fn send_tx_ack_via_worker(
    tx_pool: ApiTransactionDbPool,
    core_pool: ApiWalletDbPool,
    trade_no: &str,
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
    worker.handle(WithdrawShadowSideEffectCommand::SendTxAck(trade_no.to_string())).await
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
    let intents = scan_withdraw_intent_summaries_once(tx_pool).await?;

    Ok(intents.into_iter().map(|(label, _)| label).collect())
}

pub async fn scan_withdraw_intent_labels_for_trade_once(
    tx_pool: ApiTransactionDbPool,
    trade_no: &str,
) -> Result<Vec<String>, ServiceError> {
    let intents = scan_withdraw_intent_summaries_once(tx_pool).await?;

    Ok(intents
        .into_iter()
        .filter(|(_, intent_trade_no)| intent_trade_no == trade_no)
        .map(|(label, _)| label)
        .collect())
}

async fn scan_withdraw_intent_summaries_once(
    tx_pool: ApiTransactionDbPool,
) -> Result<Vec<(String, String)>, ServiceError> {
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

    let mut intents = Vec::new();
    while let Ok(intent) = intent_rx.try_recv() {
        intents.push(withdraw_intent_summary(intent));
    }

    Ok(intents)
}

fn withdraw_intent_summary(intent: WithdrawIntent) -> (String, String) {
    match intent {
        WithdrawIntent::Chain(WithdrawChainIntent::EstimateFee(trade_no)) => {
            ("EstimateFee".to_string(), trade_no)
        }
        WithdrawIntent::Chain(WithdrawChainIntent::EvalResourceGate(trade_no)) => {
            ("EvalResourceGate".to_string(), trade_no)
        }
        WithdrawIntent::Chain(WithdrawChainIntent::ExecuteResourceDelegation(trade_no)) => {
            ("ExecuteResourceDelegation".to_string(), trade_no)
        }
        WithdrawIntent::Chain(WithdrawChainIntent::BuildTx(trade_no)) => {
            ("BuildTx".to_string(), trade_no)
        }
        WithdrawIntent::Chain(WithdrawChainIntent::BroadcastTx(trade_no)) => {
            ("BroadcastTx".to_string(), trade_no)
        }
        WithdrawIntent::Chain(WithdrawChainIntent::RecoverTx(trade_no)) => {
            ("RecoverTx".to_string(), trade_no)
        }
        WithdrawIntent::SideEffect(WithdrawSideEffectIntent::SendTxAck(trade_no)) => {
            ("SendTxAck".to_string(), trade_no)
        }
        WithdrawIntent::SideEffect(WithdrawSideEffectIntent::SendResourceResultAck(trade_no)) => {
            ("SendResourceResultAck".to_string(), trade_no)
        }
        WithdrawIntent::SideEffect(WithdrawSideEffectIntent::SendResourceTaskAck(trade_no)) => {
            ("SendResourceTaskAck".to_string(), trade_no)
        }
        WithdrawIntent::SideEffect(WithdrawSideEffectIntent::UploadResourceTxExecReceipt(
            trade_no,
        )) => ("UploadResourceTxExecReceipt".to_string(), trade_no),
        WithdrawIntent::SideEffect(WithdrawSideEffectIntent::SendTxResAck(trade_no)) => {
            ("SendTxResAck".to_string(), trade_no)
        }
        WithdrawIntent::SideEffect(WithdrawSideEffectIntent::UploadTxExecReceipt(trade_no)) => {
            ("UploadTxExecReceipt".to_string(), trade_no)
        }
    }
}
