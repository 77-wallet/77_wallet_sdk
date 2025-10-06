use crate::infrastructure::{
    process_fee_tx::ProcessFeeTxHandle, process_withdraw_tx::ProcessWithdrawTxHandle,
};
use std::sync::Arc;
use crate::infrastructure::inner_event::InnerEventHandle;
use crate::infrastructure::process_unconfirm_msg::{UnconfirmedMsgCollector, UnconfirmedMsgProcessor};
use crate::infrastructure::task_queue::task_manager::TaskManager;

#[derive(Debug, Clone)]
pub struct Handles {
    task_manager: Arc<TaskManager>,
    inner_event_handle: Arc<InnerEventHandle>,
    unconfirmed_msg_collector: Arc<UnconfirmedMsgCollector>,
    unconfirmed_msg_processor: Arc<UnconfirmedMsgProcessor>,
    process_withdraw_tx_handle: Arc<ProcessWithdrawTxHandle>,
    process_fee_tx_handle: Arc<ProcessFeeTxHandle>,
}

impl Handles {
    pub async fn new(client_id: &str) -> Self {
        let unconfirmed_msg_collector = UnconfirmedMsgCollector::new();
        // 创建 TaskManager 实例
        let notify = Arc::new(tokio::sync::Notify::new());
        let task_manager = TaskManager::new(notify.clone());

        let unconfirmed_msg_processor = UnconfirmedMsgProcessor::new(&client_id, notify);

        let inner_event_handle = InnerEventHandle::new();

        let process_withdraw_tx_handle = ProcessWithdrawTxHandle::new().await;
        let process_fee_tx_handle = ProcessFeeTxHandle::new().await;
        Self {
            task_manager: Arc::new(task_manager),
            inner_event_handle: Arc::new(inner_event_handle),
            unconfirmed_msg_collector: Arc::new(unconfirmed_msg_collector),
            unconfirmed_msg_processor: Arc::new(unconfirmed_msg_processor),
            process_withdraw_tx_handle: Arc::new(process_withdraw_tx_handle),
            process_fee_tx_handle: Arc::new(process_fee_tx_handle),
        }
    }

    pub(crate) fn get_global_processed_withdraw_tx_handle(&self) -> Arc<ProcessWithdrawTxHandle> {
        self.process_withdraw_tx_handle.clone()
    }

    pub(crate) fn get_global_processed_fee_tx_handle(&self) -> Arc<ProcessFeeTxHandle> {
        self.process_fee_tx_handle.clone()
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

    pub(crate) fn get_global_unconfirmed_msg_processor(&self) -> Arc<UnconfirmedMsgProcessor> {
        self.unconfirmed_msg_processor.clone()
    }
}
