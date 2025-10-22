use crate::{
    infrastructure,
    infrastructure::{
        collector_unconfirm_msg::UnconfirmedMsgCollector, inner_event::InnerEventHandle,
        log::upload_log::UploadLogHandle, process_collect_tx::ProcessCollectTxHandle,
        process_fee_tx::ProcessFeeTxHandle, process_unconfirm_msg::UnconfirmedMsgProcessorHandle,
        process_withdraw_tx::ProcessWithdrawTxHandle, task_queue::task_manager::TaskManager,
    },
};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Handles {
    task_manager: Arc<TaskManager>,
    inner_event_handle: Arc<InnerEventHandle>,
    unconfirmed_msg_collector: Arc<UnconfirmedMsgCollector>,
    unconfirmed_msg_processor: Arc<UnconfirmedMsgProcessorHandle>,
    process_withdraw_tx_handle: Arc<ProcessWithdrawTxHandle>,
    process_fee_tx_handle: Arc<ProcessFeeTxHandle>,
    process_collect_tx_handle: Arc<ProcessCollectTxHandle>,
    upload_log: Arc<UploadLogHandle>,
}

impl Handles {
    pub async fn new(client_id: &str) -> Self {
        let unconfirmed_msg_collector = UnconfirmedMsgCollector::new();
        // 创建 TaskManager 实例
        let notify = Arc::new(tokio::sync::Notify::new());
        let task_manager = TaskManager::new(notify.clone());

        let unconfirmed_msg_processor =
            UnconfirmedMsgProcessorHandle::new(&client_id, notify).await;

        let inner_event_handle = InnerEventHandle::new();

        let process_withdraw_tx_handle = ProcessWithdrawTxHandle::new().await;
        let process_fee_tx_handle = ProcessFeeTxHandle::new().await;
        let process_collect_tx_handle = ProcessCollectTxHandle::new().await;
        let context = crate::context::CONTEXT.get().unwrap();
        let dirs = context.get_global_dirs();
        let base_path = infrastructure::log::format::LogBasePath(dirs.get_log_dir());
        let upload_log_handle =
            UploadLogHandle::new(base_path, 5 * 60, context.get_global_oss_client()).await;
        Self {
            task_manager: Arc::new(task_manager),
            inner_event_handle: Arc::new(inner_event_handle),
            unconfirmed_msg_collector: Arc::new(unconfirmed_msg_collector),
            unconfirmed_msg_processor: Arc::new(unconfirmed_msg_processor),
            process_withdraw_tx_handle: Arc::new(process_withdraw_tx_handle),
            process_fee_tx_handle: Arc::new(process_fee_tx_handle),
            process_collect_tx_handle: Arc::new(process_collect_tx_handle),
            upload_log: Arc::new(upload_log_handle),
        }
    }

    pub(crate) async fn close(&self) -> Result<(), crate::error::service::ServiceError> {
        self.process_withdraw_tx_handle.close().await?;
        self.process_fee_tx_handle.close().await?;
        self.process_collect_tx_handle.close().await?;
        self.upload_log.close().await?;
        self.unconfirmed_msg_processor.close().await?;
        Ok(())
    }

    pub(crate) fn get_global_processed_withdraw_tx_handle(&self) -> Arc<ProcessWithdrawTxHandle> {
        self.process_withdraw_tx_handle.clone()
    }

    pub(crate) fn get_global_processed_fee_tx_handle(&self) -> Arc<ProcessFeeTxHandle> {
        self.process_fee_tx_handle.clone()
    }

    pub(crate) fn get_global_processed_collect_tx_handle(&self) -> Arc<ProcessCollectTxHandle> {
        self.process_collect_tx_handle.clone()
    }

    pub(crate) fn get_global_task_manager(&self) -> Arc<TaskManager> {
        self.task_manager.clone()
    }

    pub(crate) fn get_global_inner_event_handle(&self) -> Arc<InnerEventHandle> {
        self.inner_event_handle.clone()
    }

    pub(crate) fn get_global_notify(&self) -> Arc<tokio::sync::Notify> {
        self.task_manager.notify.clone()
    }

    pub(crate) fn get_global_unconfirmed_msg_collector(&self) -> Arc<UnconfirmedMsgCollector> {
        self.unconfirmed_msg_collector.clone()
    }

    pub(crate) fn get_global_unconfirmed_msg_processor(
        &self,
    ) -> Arc<UnconfirmedMsgProcessorHandle> {
        self.unconfirmed_msg_processor.clone()
    }
}
