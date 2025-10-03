use crate::infrastructure::process_withdraw_tx::ProcessWithdrawTxHandle;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Handles {
    process_withdraw_tx_handle: Arc<ProcessWithdrawTxHandle>,
}

impl Handles {
    pub async fn new(client_id: &str) -> Self {
        let process_withdraw_tx_handle = ProcessWithdrawTxHandle::new().await;
        Self { process_withdraw_tx_handle: Arc::new(process_withdraw_tx_handle) }
    }

    pub(crate) fn get_global_processed_withdraw_tx_handle(&self) -> Arc<ProcessWithdrawTxHandle> {
        self.process_withdraw_tx_handle.clone()
    }
}
