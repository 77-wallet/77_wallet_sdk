use crate::{
    error::service::ServiceError,
    infrastructure::withdraw::{
        command::{ProcessWithdrawTxCommand, ProcessWithdrawTxConfirmReportCommand},
        process_withdraw_tx_confirm::ProcessWithdrawTxConfirmReport,
        process_withdraw_tx_report::ProcessWithdrawTxReport,
        process_withdraw_tx_send::ProcessWithdrawTx,
    },
};
use tokio::{
    sync::{Mutex, broadcast, mpsc},
    task::JoinHandle,
};

#[derive(Debug)]
pub(crate) struct ProcessWithdrawTxHandle {
    shutdown_tx: broadcast::Sender<()>,
    tx_tx: mpsc::Sender<ProcessWithdrawTxCommand>,
    confirm_report_tx: mpsc::Sender<ProcessWithdrawTxConfirmReportCommand>,
    tx_handle: Mutex<Option<JoinHandle<Result<(), ServiceError>>>>,
    tx_report_handle: Mutex<Option<JoinHandle<Result<(), ServiceError>>>>,
    tx_confirm_report_handle: Mutex<Option<JoinHandle<Result<(), ServiceError>>>>,
}

impl ProcessWithdrawTxHandle {
    pub(crate) async fn new() -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        let shutdown_rx1 = shutdown_tx.subscribe();
        let shutdown_rx2 = shutdown_tx.subscribe();
        let shutdown_rx3 = shutdown_tx.subscribe();
        let (report_tx, report_rx) = mpsc::channel(1);
        // 发交易
        let (tx_tx, tx_rx) = mpsc::channel(1);
        let mut tx = ProcessWithdrawTx::new(shutdown_rx1, tx_rx, report_tx);
        let handle = tokio::spawn(async move { tx.run().await });
        // 上报交易
        let mut tx_report = ProcessWithdrawTxReport::new(shutdown_rx2, report_rx);
        let tx_report_handle = tokio::spawn(async move { tx_report.run().await });
        // 上报已经确认交易
        let (confirm_report_tx, confirm_report_rx) = mpsc::channel(1);
        let mut tx_confirm_report =
            ProcessWithdrawTxConfirmReport::new(shutdown_rx3, confirm_report_rx);
        let tx_confirm_report_handle = tokio::spawn(async move { tx_confirm_report.run().await });
        Self {
            shutdown_tx,
            tx_tx,
            confirm_report_tx,
            tx_handle: Mutex::new(Some(handle)),
            tx_report_handle: Mutex::new(Some(tx_report_handle)),
            tx_confirm_report_handle: Mutex::new(Some(tx_confirm_report_handle)),
        }
    }

    pub(crate) async fn submit_tx(&self, trade_no: &str) -> Result<(), ServiceError> {
        let _ = self.tx_tx.send(ProcessWithdrawTxCommand::Tx(trade_no.to_string()));
        Ok(())
    }

    pub(crate) async fn submit_confirm_report_tx(
        &self,
        trade_no: &str,
    ) -> Result<(), ServiceError> {
        let _ = self
            .confirm_report_tx
            .send(ProcessWithdrawTxConfirmReportCommand::Tx(trade_no.to_string()));
        Ok(())
    }

    pub(crate) async fn close(&self) -> Result<(), ServiceError> {
        let _ = self.shutdown_tx.send(());
        if let Some(handle) = self.tx_handle.lock().await.take() {
            handle.await.map_err(|_| {
                ServiceError::System(crate::error::system::SystemError::BackendEndpointNotFound)
            })??;
        }
        if let Some(handle) = self.tx_report_handle.lock().await.take() {
            handle.await.map_err(|_| {
                ServiceError::System(crate::error::system::SystemError::BackendEndpointNotFound)
            })??;
        }
        if let Some(handle) = self.tx_confirm_report_handle.lock().await.take() {
            handle.await.map_err(|_| {
                ServiceError::System(crate::error::system::SystemError::BackendEndpointNotFound)
            })??;
        }
        Ok(())
    }
}
