use crate::{
    error::{service::ServiceError, system::SystemError},
    infrastructure::api_trans::collect::{
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

/// ProcessCollectTxHandle
///
/// ⚠️ Architectural note:
/// This handle is no longer the execution entry of collect tx.
/// The real entry point is the Shadow Scanner system.
///
/// This handle only hosts legacy workers:
/// - ProcessCollectTx
/// - ProcessCollectTxReport
/// - ProcessCollectTxConfirmReport
///
/// All execution is fact-driven and dispatched by Shadow.
#[deprecated(note = "legacy process_collect_tx handle; use Shadow scanner/actor flow")]
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
        let api_wallet_pool = ctx.api_wallet_pool()?;
        let api_transaction_pool = ctx.api_transaction_pool()?;

        let (tx_tx, tx_rx) = mpsc::channel(1);
        let (report_tx, report_rx) = mpsc::channel(1);

        // LEGACY COLLECT WORKERS
        // NOTE:
        // These workers are legacy and MUST NOT be auto-started.
        // Collect execution is now fully driven by Shadow system.
        //
        // Kept temporarily for safe migration.

        // 发交易
        let _tx = ProcessCollectTx::new(
            api_wallet_pool.clone(),
            api_transaction_pool.clone(),
            shutdown_rx1,
            tx_rx,
            report_tx.clone(),
        );
        // 注释掉自动启动，旧工作者不再运行
        // let tx_handle = tokio::spawn(async move { tx.run().await });

        // 上报交易
        let _tx_report =
            ProcessCollectTxReport::new(api_transaction_pool.clone(), shutdown_rx2, report_rx);
        // 注释掉自动启动，旧工作者不再运行
        // let tx_report_handle = tokio::spawn(async move { tx_report.run().await });

        // 上报已经确认交易
        let (confirm_report_tx, confirm_report_rx) = mpsc::channel(1);
        let _tx_confirm_report = ProcessCollectTxConfirmReport::new(
            api_transaction_pool.clone(),
            shutdown_rx3,
            confirm_report_rx,
        );
        // 注释掉自动启动，旧工作者不再运行
        // let tx_confirm_report_handle = tokio::spawn(async move { tx_confirm_report.run().await });

        // 由于旧工作者不再启动，我们不需要它们的handle
        let tx_handle = Mutex::new(None);
        let tx_report_handle = Mutex::new(None);
        let tx_confirm_report_handle = Mutex::new(None);

        // 初始化Shadow系统
        shadow::enable();
        let shadow_system =
            shadow::init(api_transaction_pool.clone(), api_wallet_pool.clone()).await;

        Ok(Self {
            shutdown_tx,
            tx_tx,
            confirm_report_tx,
            tx_handle,
            tx_report_handle,
            tx_confirm_report_handle,
            shadow_system,
        })
    }

    /// LEGACY ENTRY.
    /// This method is NOT an execution entry anymore.
    /// All collect execution MUST be driven by Shadow system.
    ///
    /// ⚠️ LEGACY API
    /// This method is kept for backward compatibility only.
    /// New collect execution MUST be driven by Shadow Scanner.
    /// DO NOT call this method from new code.
    #[deprecated(
        note = "v2 架构已不再使用该 API。调用该方法不会触发任何实际归集推进，请使用 Shadow Scanner"
    )]
    pub(crate) async fn submit_tx(&self, trade_no: &str) -> Result<(), ServiceError> {
        self.tx_tx
            .send(ProcessCollectTxCommand::Tx(trade_no.to_string()))
            .await
            .map_err(|e| ServiceError::System(SystemError::ChannelSendFailed(e.to_string())))?;
        Ok(())
    }

    /// LEGACY ENTRY.
    /// This method is NOT an execution entry anymore.
    /// All collect execution MUST be driven by Shadow system.
    ///
    /// ⚠️ LEGACY API
    /// This method is kept for backward compatibility only.
    /// New collect execution MUST be driven by Shadow Scanner.
    /// DO NOT call this method from new code.
    #[deprecated(
        note = "v2 架构已不再使用该 API。调用该方法不会触发任何实际归集推进，请使用 Shadow Scanner"
    )]
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
            if let Err(err) = handle.await {
                tracing::warn!(error = %err, "collect tx task join failed during close");
            }
        }
        if let Some(handle) = self.tx_report_handle.lock().await.take() {
            if let Err(err) = handle.await {
                tracing::warn!(error = %err, "collect tx report task join failed during close");
            }
        }
        if let Some(handle) = self.tx_confirm_report_handle.lock().await.take() {
            if let Err(err) = handle.await {
                tracing::warn!(
                    error = %err,
                    "collect tx confirm report task join failed during close"
                );
            }
        }

        // 关闭Shadow系统
        // 注意：Shadow系统的停止逻辑已经在Actor内部处理，不需要外部调用
        // if let Some(shadow_system) = &self.shadow_system {
        //     shadow_system.stop().await;
        // }

        Ok(())
    }

    /// 获取 Shadow 系统句柄
    ///
    /// 注意：仅用于触发快速通道，不应该在其他地方使用
    pub(crate) fn get_shadow_system(&self) -> Option<&CollectorShadowActorSystem> {
        self.shadow_system.as_ref()
    }
}
