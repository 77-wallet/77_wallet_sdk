use crate::{
    error::{service::ServiceError, system::SystemError},
    infrastructure::collect::{
        command::{ProcessCollectTxCommand, ProcessCollectTxConfirmReportCommand},
        process_collect_tx_confirm::ProcessCollectTxConfirmReport,
        process_collect_tx_report::ProcessCollectTxReport,
        process_collect_tx_send::ProcessCollectTx,
        shadow::{self, CollectorShadowActorSystem},
    },
};
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
    /// Shadow系统句柄
    shadow_system: Option<CollectorShadowActorSystem>,
}

impl ProcessCollectTxHandle {
    pub(crate) async fn new() -> Result<Self, crate::error::service::ServiceError> {
        let (shutdown_tx, _) = broadcast::channel(1);
        let shutdown_rx1 = shutdown_tx.subscribe();
        let shutdown_rx2 = shutdown_tx.subscribe();
        let shutdown_rx3 = shutdown_tx.subscribe();

        // 获取 collect 数据库连接池
        let ctx = crate::context::get_context()?;
        let core_pool = ctx.core_pool()?;
        let api_funds_pool = ctx.api_funds_pool()?;

        let (tx_tx, tx_rx) = mpsc::channel(1);
        let (report_tx, report_rx) = mpsc::channel(1);

        // 发交易
        let mut tx = ProcessCollectTx::new(
            core_pool.clone(),
            api_funds_pool.clone(),
            shutdown_rx1,
            tx_rx,
            report_tx.clone(),
        );
        let tx_handle = tokio::spawn(async move { tx.run().await });
        // 上报交易
        let mut tx_report =
            ProcessCollectTxReport::new(api_funds_pool.clone(), shutdown_rx2, report_rx);
        let tx_report_handle = tokio::spawn(async move { tx_report.run().await });
        // 上报已经确认交易
        let (confirm_report_tx, confirm_report_rx) = mpsc::channel(1);
        let mut tx_confirm_report = ProcessCollectTxConfirmReport::new(
            api_funds_pool.clone(),
            shutdown_rx3,
            confirm_report_rx,
        );
        let tx_confirm_report_handle = tokio::spawn(async move { tx_confirm_report.run().await });

        // 初始化Shadow系统
        let shadow_system = shadow::init(api_funds_pool.clone(), core_pool.clone()).await;

        Ok(Self {
            shutdown_tx,
            tx_tx,
            confirm_report_tx,
            tx_handle: Mutex::new(Some(tx_handle)),
            tx_report_handle: Mutex::new(Some(tx_report_handle)),
            tx_confirm_report_handle: Mutex::new(Some(tx_confirm_report_handle)),
            shadow_system,
        })
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

        // 关闭Shadow系统
        // 注意：Shadow系统的停止逻辑已经在Actor内部处理，不需要外部调用
        // if let Some(shadow_system) = &self.shadow_system {
        //     shadow_system.stop().await;
        // }

        Ok(())
    }
}
