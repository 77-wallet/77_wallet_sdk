use crate::{
    error::{service::ServiceError, system::SystemError},
    infrastructure::collect::{
        command::{ProcessCollectTxCommand, ProcessCollectTxConfirmReportCommand},
        process_collect_tx_confirm::ProcessCollectTxConfirmReport,
        process_collect_tx_report::ProcessCollectTxReport,
        process_collect_tx_send::ProcessCollectTx,
    },
};
use std::sync::Arc;
use tokio::{
    sync::{Mutex, broadcast, mpsc},
    task::JoinHandle,
};

#[derive(Debug)]
pub(crate) struct ProcessCollectTxHandle {
    shutdown_tx: broadcast::Sender<()>,
    tx_tx: mpsc::Sender<ProcessCollectTxCommand>,
    confirm_report_tx: mpsc::Sender<ProcessCollectTxConfirmReportCommand>,
    tx_handle: Mutex<Option<JoinHandle<()>>>,
    tx_report_handle: Mutex<Option<JoinHandle<()>>>,
    tx_confirm_report_handle: Mutex<Option<JoinHandle<()>>>,
}

impl ProcessCollectTxHandle {
    pub(crate) async fn new(pool: Arc<sqlx::SqlitePool>) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        let shutdown_rx1 = shutdown_tx.subscribe();
        let shutdown_rx2 = shutdown_tx.subscribe();
        let shutdown_rx3 = shutdown_tx.subscribe();

        let (tx_tx, tx_rx) = mpsc::channel(1);
        let (report_tx, report_rx) = mpsc::channel(1);

        // 发交易
        let mut tx = ProcessCollectTx::new(pool.clone(), shutdown_rx1, tx_rx, report_tx);
        let tx_handle = tokio::spawn(async move { tx.run().await });
        // 上报交易
        let mut tx_report = ProcessCollectTxReport::new(pool.clone(), shutdown_rx2, report_rx);
        let tx_report_handle = tokio::spawn(async move { tx_report.run().await });
        // 上报已经确认交易
        let (confirm_report_tx, confirm_report_rx) = mpsc::channel(1);
        let mut tx_confirm_report =
            ProcessCollectTxConfirmReport::new(pool, shutdown_rx3, confirm_report_rx);
        let tx_confirm_report_handle = tokio::spawn(async move { tx_confirm_report.run().await });
        Self {
            shutdown_tx,
            tx_tx,
            confirm_report_tx,
            tx_handle: Mutex::new(Some(tx_handle)),
            tx_report_handle: Mutex::new(Some(tx_report_handle)),
            tx_confirm_report_handle: Mutex::new(Some(tx_confirm_report_handle)),
        }
    }

    pub(crate) async fn submit_tx(&self, trade_no: &str) -> Result<(), ServiceError> {
        self.tx_tx
            .send(ProcessCollectTxCommand::Tx(trade_no.to_string()))
            .await
            .map_err(|e| ServiceError::System(SystemError::ChannelSendFailed(e.to_string())))?;
        Ok(())
    }

    pub(crate) async fn submit_confirm_report_tx(
        &self,
        trade_no: &str,
    ) -> Result<(), ServiceError> {
        self.confirm_report_tx
            .send(ProcessCollectTxConfirmReportCommand::Tx(trade_no.to_string()))
            .await
            .map_err(|e| ServiceError::System(SystemError::ChannelSendFailed(e.to_string())))?;
        Ok(())
    }

    pub(crate) async fn close(&self) -> Result<(), ServiceError> {
        let _ = self.shutdown_tx.send(());
        if let Some(handle) = self.tx_handle.lock().await.take() {
            handle.await;
        }
        if let Some(handle) = self.tx_report_handle.lock().await.take() {
            handle.await;
        }
        if let Some(handle) = self.tx_confirm_report_handle.lock().await.take() {
            handle.await;
        }
        Ok(())
    }
}
