use std::{sync::Arc, time::Duration};

use tokio::sync::{broadcast, mpsc};
use tracing::{error, info, trace, warn};
use wallet_chain_interact::{
    BillResourceConsume,
    tron::{
        self,
        operations::{
            RawTransactionParams, TronTxOperation,
            stake::{FreezeBalanceArgs, UnFreezeBalanceArgs},
        },
    },
};
use wallet_database::{
    ApiTransactionDbPool,
    entities::{
        api_resource_operation::{ApiResourceOperationEntity, ApiResourceOperationType},
        api_resource_type::ApiResourceType,
    },
    repositories::api_wallet::resource_operation::ApiResourceOperationRepo,
};
use wallet_transport_backend::request::api_wallet::transaction::{
    TransAckType, TransEventAckReq, TransType,
};

use crate::{
    context::{CONTEXT, get_context},
    domain::{api_wallet::adapter::tx::RawTx, chain::adapter::ChainAdapterFactory},
    error::service::ServiceError,
    infrastructure::{api_trans::shadow_rpc_policy, runtime::time::new_production_interval},
};

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
pub struct ResourceOperationScannerConfig {
    pub scan_interval: Duration,
    pub max_items_per_scan: usize,
}

impl Default for ResourceOperationScannerConfig {
    fn default() -> Self {
        let scan_interval_secs = shadow_rpc_policy::read_u64_env(
            "RESOURCE_OPERATION_SHADOW_SCAN_INTERVAL_SECS",
            30,
            10,
            120,
        );
        let max_items_per_scan = shadow_rpc_policy::read_u64_env(
            "RESOURCE_OPERATION_SHADOW_MAX_ITEMS_PER_SCAN",
            20,
            1,
            200,
        ) as usize;

        Self { scan_interval: Duration::from_secs(scan_interval_secs), max_items_per_scan }
    }
}

#[derive(Debug, Clone)]
pub struct ResourceOperationScanner {
    pool: ApiTransactionDbPool,
    config: ResourceOperationScannerConfig,
}

impl ResourceOperationScanner {
    pub fn new(pool: ApiTransactionDbPool) -> Self {
        Self::with_config(pool, ResourceOperationScannerConfig::default())
    }

    pub fn with_config(pool: ApiTransactionDbPool, config: ResourceOperationScannerConfig) -> Self {
        Self { pool, config }
    }

    pub async fn scan_round(&self) -> Vec<ResourceOperationIntent> {
        let mut intents = Vec::new();

        match ApiResourceOperationRepo::scan_need_task_ack(
            &self.pool,
            self.config.max_items_per_scan,
        )
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

        match ApiResourceOperationRepo::scan_can_build(&self.pool, self.config.max_items_per_scan)
            .await
        {
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

pub struct ResourceOperationScannerActor {
    scanner: Arc<ResourceOperationScanner>,
    intent_tx: mpsc::Sender<ResourceOperationIntent>,
    shutdown_rx: broadcast::Receiver<()>,
}

impl ResourceOperationScannerActor {
    pub fn new(
        scanner: Arc<ResourceOperationScanner>,
        intent_tx: mpsc::Sender<ResourceOperationIntent>,
        shutdown_rx: broadcast::Receiver<()>,
    ) -> Self {
        Self { scanner, intent_tx, shutdown_rx }
    }

    pub async fn run(mut self) {
        crate::infrastructure::system_ready::wait_system_ready().await;
        info!("Resource operation shadow scanner actor running");

        let mut interval = new_production_interval(self.scanner.config.scan_interval);

        loop {
            tokio::select! {
                _ = self.shutdown_rx.recv() => {
                    info!("Received shutdown signal for resource operation scanner actor");
                    break;
                }
                _ = interval.tick() => {
                    for intent in self.scanner.scan_round().await {
                        if let Err(e) = self.intent_tx.send(intent).await {
                            error!(error = %e, "Failed to enqueue resource operation intent");
                            break;
                        }
                    }
                }
            }
        }

        info!("Resource operation shadow scanner actor stopped");
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
            return Ok(());
        }

        if let Err(err) = self.build_resource_operation(&resource_trade_no).await {
            if let Err(db_err) =
                ApiResourceOperationRepo::clear_building_at(&self.pool, &resource_trade_no).await
            {
                error!(
                    resource_trade_no = %resource_trade_no,
                    error = %db_err,
                    "Failed to release resource operation build slot after build failure"
                );
            }
            return Err(err);
        }

        Ok(())
    }

    async fn build_resource_operation(&self, resource_trade_no: &str) -> Result<(), ServiceError> {
        let operation =
            ApiResourceOperationRepo::get_by_resource_trade_no(&self.pool, resource_trade_no)
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;

        if operation.raw_tx.is_some() {
            trace!(
                resource_trade_no = %resource_trade_no,
                "Resource operation raw_tx already exists, skipping build"
            );
            return Ok(());
        }

        let (tx_hash, raw_tx, transaction_fee) = self.build_tron_resource_raw(&operation).await?;
        let raw_tx_str = wallet_utils::serde_func::serde_to_string(&raw_tx)?;
        let affected = ApiResourceOperationRepo::update_after_build(
            &self.pool,
            resource_trade_no,
            &tx_hash,
            &raw_tx_str,
            &transaction_fee,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;

        if affected == 0 {
            trace!(
                resource_trade_no = %resource_trade_no,
                "Resource operation build fact was already committed"
            );
        } else {
            info!(
                resource_trade_no = %resource_trade_no,
                tx_hash = %tx_hash,
                transaction_fee = %transaction_fee,
                "Resource operation raw transaction built and persisted"
            );
        }

        Ok(())
    }

    async fn build_tron_resource_raw(
        &self,
        operation: &ApiResourceOperationEntity,
    ) -> Result<(String, RawTx, String), ServiceError> {
        if operation.chain_code != "tron" {
            return Err(ServiceError::Parameter(format!(
                "resource operation only supports tron, got {}",
                operation.chain_code
            )));
        }

        let amount = Self::parse_trx_amount(&operation.amount)?;
        let resource = Self::tron_resource_name(operation.resource_type);
        let chain = ChainAdapterFactory::get_tron_adapter().await?;

        let _chain_rpc_guard =
            crate::infrastructure::chain_rpc_guard::acquire_if_guarded(&operation.chain_code).await;

        let raw = match operation.operation_type {
            ApiResourceOperationType::Stake => {
                let args =
                    FreezeBalanceArgs::new(&operation.owner_address, resource, amount, None)?;
                args.build_raw_transaction(chain.get_provider()).await?
            }
            ApiResourceOperationType::Unstake => {
                let args =
                    UnFreezeBalanceArgs::new(&operation.owner_address, resource, amount, None)?;
                args.build_raw_transaction(chain.get_provider()).await?
            }
        };

        self.sign_tron_resource_raw(operation, raw).await
    }

    async fn sign_tron_resource_raw(
        &self,
        operation: &ApiResourceOperationEntity,
        mut raw: RawTransactionParams,
    ) -> Result<(String, RawTx, String), ServiceError> {
        let chain = ChainAdapterFactory::get_tron_adapter().await?;
        let provider = chain.get_provider();
        let consumer =
            provider.transfer_fee(&operation.owner_address, None, &raw.raw_data_hex, 1).await?;
        let balance = chain.balance(&operation.owner_address, None).await?;
        let stake_amount_sun = match operation.operation_type {
            ApiResourceOperationType::Stake => {
                Self::parse_trx_amount(&operation.amount)? * tron::consts::TRX_VALUE
            }
            ApiResourceOperationType::Unstake => 0,
        };
        let need_sun = consumer.transaction_fee_i64().saturating_add(stake_amount_sun);
        if balance.to::<i64>() < need_sun {
            return Err(ServiceError::Parameter(format!(
                "resource operation balance is insufficient: balance={}, need={}",
                balance, need_sun
            )));
        }

        let handles = get_context()?.get_handles_arc().await?;
        let private_key_manager = handles.get_global_private_key_manager();
        let private_key = private_key_manager
            .get_private_key(&operation.owner_address, &operation.chain_code)
            .await?;
        let sign = wallet_utils::sign::sign_tron(&raw.tx_id, &private_key, None)?;
        raw.signature.push(sign);

        let tx_hash = raw.tx_id.clone();
        let transaction_fee = consumer.transaction_fee();
        let raw_tx = RawTx::Tron(
            raw,
            BillResourceConsume::new_tron(consumer.act_bandwidth() as u64, 0),
            transaction_fee.clone(),
        );

        Ok((tx_hash, raw_tx, transaction_fee))
    }

    fn parse_trx_amount(amount: &str) -> Result<i64, ServiceError> {
        let parsed = amount
            .trim()
            .parse::<i64>()
            .map_err(|_| ServiceError::Parameter(format!("invalid resource amount: {amount}")))?;
        if parsed <= 0 {
            return Err(ServiceError::Parameter(format!(
                "resource amount must be positive: {amount}"
            )));
        }
        Ok(parsed)
    }

    fn tron_resource_name(resource_type: ApiResourceType) -> &'static str {
        match resource_type {
            ApiResourceType::Bandwidth => "bandwidth",
            ApiResourceType::Energy => "energy",
        }
    }
}

pub struct ResourceOperationDispatcherActor {
    worker: ResourceOperationWorker,
    intent_rx: mpsc::Receiver<ResourceOperationIntent>,
    shutdown_rx: broadcast::Receiver<()>,
}

impl ResourceOperationDispatcherActor {
    pub fn new(
        worker: ResourceOperationWorker,
        intent_rx: mpsc::Receiver<ResourceOperationIntent>,
        shutdown_rx: broadcast::Receiver<()>,
    ) -> Self {
        Self { worker, intent_rx, shutdown_rx }
    }

    pub async fn run(mut self) {
        crate::infrastructure::system_ready::wait_system_ready().await;
        info!("Resource operation shadow dispatcher actor running");

        loop {
            tokio::select! {
                _ = self.shutdown_rx.recv() => {
                    info!("Received shutdown signal for resource operation dispatcher actor");
                    break;
                }
                Some(intent) = self.intent_rx.recv() => {
                    if let Err(e) = self.worker.handle(intent).await {
                        error!(error = %e, "Failed to handle resource operation intent");
                    }
                }
                else => {
                    info!("Resource operation intent channel closed");
                    break;
                }
            }
        }

        info!("Resource operation shadow dispatcher actor stopped");
    }
}

#[derive(Debug)]
pub struct ResourceOperationShadowActorSystem {
    shutdown_tx: broadcast::Sender<()>,
    scanner_handle: Option<tokio::task::JoinHandle<()>>,
    dispatcher_handle: Option<tokio::task::JoinHandle<()>>,
}

impl ResourceOperationShadowActorSystem {
    pub fn new(pool: ApiTransactionDbPool) -> Self {
        let (shutdown_tx, shutdown_rx1) = broadcast::channel(1);
        let shutdown_rx2 = shutdown_tx.subscribe();
        let (intent_tx, intent_rx) = mpsc::channel(100);

        let scanner = Arc::new(ResourceOperationScanner::new(pool.clone()));
        let worker = ResourceOperationWorker::new(pool);

        info!(
            scan_interval_secs = scanner.config.scan_interval.as_secs(),
            max_items_per_scan = scanner.config.max_items_per_scan,
            "Resource operation shadow runtime config"
        );

        let scanner_clone = scanner.clone();
        let intent_tx_clone = intent_tx.clone();
        tokio::spawn(async move {
            for intent in scanner_clone.scan_round().await {
                if let Err(e) = intent_tx_clone.send(intent).await {
                    error!(error = %e, "Failed to enqueue warm resource operation intent");
                    break;
                }
            }
            info!("Warm resource operation shadow scan completed");
        });

        let scanner_actor =
            ResourceOperationScannerActor::new(scanner.clone(), intent_tx, shutdown_rx1);
        let scanner_handle = Some(tokio::spawn(async move {
            scanner_actor.run().await;
        }));

        let dispatcher_actor =
            ResourceOperationDispatcherActor::new(worker, intent_rx, shutdown_rx2);
        let dispatcher_handle = Some(tokio::spawn(async move {
            dispatcher_actor.run().await;
        }));

        Self { shutdown_tx, scanner_handle, dispatcher_handle }
    }

    pub async fn stop(&mut self) {
        info!("Stopping resource operation shadow system");

        let _ = self.shutdown_tx.send(());

        if let Some(handle) = self.scanner_handle.take() {
            if let Err(err) = handle.await {
                warn!(error = %err, "resource operation scanner join failed during stop");
            }
        }

        if let Some(handle) = self.dispatcher_handle.take() {
            if let Err(err) = handle.await {
                warn!(error = %err, "resource operation dispatcher join failed during stop");
            }
        }

        info!("Resource operation shadow system stopped");
    }
}

pub(crate) async fn init(pool: ApiTransactionDbPool) -> ResourceOperationShadowActorSystem {
    ResourceOperationShadowActorSystem::new(pool)
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

    #[test]
    fn resource_operation_amount_requires_positive_integer_trx() {
        assert_eq!(ResourceOperationWorker::parse_trx_amount("1000").unwrap(), 1000);
        assert!(ResourceOperationWorker::parse_trx_amount("0").is_err());
        assert!(ResourceOperationWorker::parse_trx_amount("-1").is_err());
        assert!(ResourceOperationWorker::parse_trx_amount("1.5").is_err());
    }

    #[test]
    fn resource_operation_resource_type_maps_to_tron_names() {
        assert_eq!(ResourceOperationWorker::tron_resource_name(ApiResourceType::Energy), "energy");
        assert_eq!(
            ResourceOperationWorker::tron_resource_name(ApiResourceType::Bandwidth),
            "bandwidth"
        );
    }
}
