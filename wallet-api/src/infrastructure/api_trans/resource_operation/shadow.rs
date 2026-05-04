use tracing::{error, info, trace, warn};
use wallet_database::{
    ApiTransactionDbPool, repositories::api_wallet::resource_operation::ApiResourceOperationRepo,
};
use wallet_transport_backend::request::api_wallet::transaction::{
    TransAckType, TransEventAckReq, TransType,
};

use crate::{context::CONTEXT, error::service::ServiceError};

/// 独立平台资源质押/解质押流程。
///
/// 这里只处理 `api_resource_operation` / `tradeType=4`。
/// 打能量/回收能量属于 `api_resource_delegation`，服务归集/提币 gate，
/// 不得接入本流程。
#[derive(Debug, Clone)]
pub enum ResourceOperationIntent {
    SendTaskAck(String),
    ClaimBuildSlot(String),
}

#[derive(Debug, Clone)]
pub struct ResourceOperationScanner {
    pool: ApiTransactionDbPool,
    max_items_per_scan: usize,
}

impl ResourceOperationScanner {
    pub fn new(pool: ApiTransactionDbPool) -> Self {
        Self { pool, max_items_per_scan: 20 }
    }

    pub async fn scan_round(&self) -> Vec<ResourceOperationIntent> {
        let mut intents = Vec::new();

        match ApiResourceOperationRepo::scan_need_task_ack(&self.pool, self.max_items_per_scan)
            .await
        {
            Ok(records) => {
                for record in records {
                    intents.push(ResourceOperationIntent::SendTaskAck(record.resource_trade_no));
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to scan resource operation task ACK records");
            }
        }

        match ApiResourceOperationRepo::scan_can_build(&self.pool, self.max_items_per_scan).await {
            Ok(records) => {
                for record in records {
                    intents.push(ResourceOperationIntent::ClaimBuildSlot(record.resource_trade_no));
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to scan resource operation build-slot records");
            }
        }

        intents
    }
}

#[derive(Debug, Clone)]
pub struct ResourceOperationWorker {
    pool: ApiTransactionDbPool,
}

impl ResourceOperationWorker {
    pub fn new(pool: ApiTransactionDbPool) -> Self {
        Self { pool }
    }

    pub async fn handle(&self, intent: ResourceOperationIntent) -> Result<(), ServiceError> {
        match intent {
            ResourceOperationIntent::SendTaskAck(resource_trade_no) => {
                self.send_task_ack(resource_trade_no).await
            }
            ResourceOperationIntent::ClaimBuildSlot(resource_trade_no) => {
                self.claim_build_slot(resource_trade_no).await
            }
        }
    }

    async fn send_task_ack(&self, resource_trade_no: String) -> Result<(), ServiceError> {
        info!(resource_trade_no = %resource_trade_no, "Processing resource operation task ACK");

        let resource_task =
            ApiResourceOperationRepo::get_by_resource_trade_no(&self.pool, &resource_trade_no)
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;

        if resource_task.task_ack_sent_at.is_some() {
            trace!(resource_trade_no = %resource_trade_no, "Resource operation task ACK already sent");
            return Ok(());
        }

        let backend_api = CONTEXT.get().unwrap().get_global_backend_api();
        backend_api
            .trans_event_ack(&TransEventAckReq::new(
                &resource_trade_no,
                // tradeType=4 平台资源质押/解锁任务，对应后端 ACK type=PLT_RSC_STK。
                TransType::PltRscStk,
                TransAckType::Tx,
            ))
            .await?;

        let affected = ApiResourceOperationRepo::mark_task_ack_sent(&self.pool, &resource_trade_no)
            .await
            .map_err(|e| ServiceError::Database(e.into()))?;
        if affected == 0 {
            warn!(resource_trade_no = %resource_trade_no, "Resource operation task ACK marked 0 rows");
        }

        Ok(())
    }

    async fn claim_build_slot(&self, resource_trade_no: String) -> Result<(), ServiceError> {
        info!(resource_trade_no = %resource_trade_no, "Claiming resource operation build slot");

        let affected = ApiResourceOperationRepo::claim_building_at(&self.pool, &resource_trade_no)
            .await
            .map_err(|e| ServiceError::Database(e.into()))?;
        if affected == 0 {
            trace!(resource_trade_no = %resource_trade_no, "Resource operation build slot not claimed");
        }

        Ok(())
    }
}

pub async fn scan_and_process_once(pool: ApiTransactionDbPool) -> Result<(), ServiceError> {
    let scanner = ResourceOperationScanner::new(pool.clone());
    let worker = ResourceOperationWorker::new(pool);

    for intent in scanner.scan_round().await {
        worker.handle(intent).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wallet_database::{
        SqliteContext, entities::api_resource_operation::NewApiResourceOperation,
        repositories::api_wallet::resource_operation::ApiResourceOperationRepo,
    };

    #[tokio::test]
    async fn scanner_owns_resource_operation_ack_and_build_intents() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_root = dir.path().to_string_lossy().to_string();
        let pool = SqliteContext::new(&db_root, Some("api_transaction.db"))
            .await
            .expect("init api_transaction.db")
            .into_transaction_db_pool()
            .expect("transaction pool");

        ApiResourceOperationRepo::upsert(
            &pool,
            NewApiResourceOperation::backend_stake("uid_1", "op_need_ack", "owner", "1"),
        )
        .await
        .unwrap();
        ApiResourceOperationRepo::upsert(
            &pool,
            NewApiResourceOperation::backend_stake("uid_1", "op_can_build", "owner", "1"),
        )
        .await
        .unwrap();
        ApiResourceOperationRepo::mark_task_ack_sent(&pool, "op_can_build").await.unwrap();

        let scanner = ResourceOperationScanner::new(pool);
        let intents = scanner.scan_round().await;

        assert!(intents.iter().any(|intent| {
            matches!(intent, ResourceOperationIntent::SendTaskAck(trade_no) if trade_no == "op_need_ack")
        }));
        assert!(intents.iter().any(|intent| {
            matches!(intent, ResourceOperationIntent::ClaimBuildSlot(trade_no) if trade_no == "op_can_build")
        }));
    }
}
