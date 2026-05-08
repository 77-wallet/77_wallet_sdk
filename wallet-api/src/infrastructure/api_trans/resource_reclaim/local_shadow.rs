use std::{sync::Arc, time::Duration};

use tokio::sync::{broadcast, mpsc};
use tracing::{error, info, warn};
use wallet_chain_interact::{
    BillResourceConsume,
    tron::operations::{RawTransactionParams, TronTxOperation, stake::UnDelegateArgs},
};
use wallet_database::{
    ApiTransactionDbPool,
    entities::{
        api_resource_delegation::{
            ApiResourceDelegationEntity, ApiResourceDelegationOperationType,
            ApiResourceDelegationResultStatus, ApiResourceDelegationSource,
        },
        api_resource_type::ApiResourceType,
        api_trade_type::ApiTradeType,
    },
    repositories::api_wallet::resource_delegation::ApiResourceDelegationRepo,
};
use wallet_utils::RetryableError as _;

use crate::{
    context::get_context,
    domain::{
        api_wallet::{adapter::tx::RawTx, trans::ApiTransDomain},
        chain::adapter::ChainAdapterFactory,
    },
    error::{service::ServiceError, system::SystemError},
    infrastructure::{api_trans::shadow_rpc_policy, runtime::time::new_production_interval},
};

#[derive(Debug, Clone)]
pub enum LocalResourceReclaimIntent {
    ExecuteLocalUndelegation(String),
    RecoverLocalUndelegation(String),
}

#[derive(Debug, Clone)]
pub struct LocalResourceReclaimScannerConfig {
    pub scan_interval: Duration,
    pub max_items_per_scan: usize,
}

impl Default for LocalResourceReclaimScannerConfig {
    fn default() -> Self {
        let scan_interval_secs = shadow_rpc_policy::read_u64_env(
            "LOCAL_RESOURCE_RECLAIM_SHADOW_SCAN_INTERVAL_SECS",
            30,
            10,
            120,
        );
        let max_items_per_scan = shadow_rpc_policy::read_u64_env(
            "LOCAL_RESOURCE_RECLAIM_SHADOW_MAX_ITEMS_PER_SCAN",
            20,
            1,
            200,
        ) as usize;

        Self { scan_interval: Duration::from_secs(scan_interval_secs), max_items_per_scan }
    }
}

#[derive(Debug, Clone)]
pub struct LocalResourceReclaimScanner {
    pool: ApiTransactionDbPool,
    config: LocalResourceReclaimScannerConfig,
}

impl LocalResourceReclaimScanner {
    pub fn new(pool: ApiTransactionDbPool) -> Self {
        Self::with_config(pool, LocalResourceReclaimScannerConfig::default())
    }

    pub fn with_config(
        pool: ApiTransactionDbPool,
        config: LocalResourceReclaimScannerConfig,
    ) -> Self {
        Self { pool, config }
    }

    pub async fn scan_round(&self) -> Vec<LocalResourceReclaimIntent> {
        let mut intents = Vec::new();

        match ApiResourceDelegationRepo::scan_can_execute_for_origin_type_source_and_operation(
            &self.pool,
            ApiTradeType::Collect as i64,
            ApiResourceDelegationSource::Local,
            ApiResourceDelegationOperationType::Undelegate,
            self.config.max_items_per_scan,
        )
        .await
        {
            Ok(records) => {
                for record in records {
                    intents.push(LocalResourceReclaimIntent::ExecuteLocalUndelegation(
                        record.resource_trade_no,
                    ));
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to scan executable local undelegation records");
            }
        }

        match ApiResourceDelegationRepo::scan_can_recover_local_undelegation_for_origin_type(
            &self.pool,
            ApiTradeType::Collect as i64,
            self.config.max_items_per_scan,
        )
        .await
        {
            Ok(records) => {
                for record in records {
                    intents.push(LocalResourceReclaimIntent::RecoverLocalUndelegation(
                        record.resource_trade_no,
                    ));
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to scan recoverable local undelegation records");
            }
        }

        intents
    }
}

pub struct LocalResourceReclaimScannerActor {
    scanner: Arc<LocalResourceReclaimScanner>,
    intent_tx: mpsc::Sender<LocalResourceReclaimIntent>,
    shutdown_rx: broadcast::Receiver<()>,
}

impl LocalResourceReclaimScannerActor {
    pub fn new(
        scanner: Arc<LocalResourceReclaimScanner>,
        intent_tx: mpsc::Sender<LocalResourceReclaimIntent>,
        shutdown_rx: broadcast::Receiver<()>,
    ) -> Self {
        Self { scanner, intent_tx, shutdown_rx }
    }

    pub async fn run(mut self) {
        crate::infrastructure::system_ready::wait_system_ready().await;
        info!("Local resource reclaim shadow scanner actor running");

        let mut interval = new_production_interval(self.scanner.config.scan_interval);

        loop {
            tokio::select! {
                _ = self.shutdown_rx.recv() => {
                    info!("Received shutdown signal for local resource reclaim scanner actor");
                    break;
                }
                _ = interval.tick() => {
                    for intent in self.scanner.scan_round().await {
                        if let Err(e) = self.intent_tx.send(intent).await {
                            error!(error = %e, "Failed to enqueue local resource reclaim intent");
                            break;
                        }
                    }
                }
            }
        }

        info!("Local resource reclaim shadow scanner actor stopped");
    }
}

#[derive(Debug, Clone)]
pub struct LocalResourceReclaimWorker {
    pool: ApiTransactionDbPool,
}

impl LocalResourceReclaimWorker {
    pub fn new(pool: ApiTransactionDbPool) -> Self {
        Self { pool }
    }

    pub async fn handle(&self, intent: LocalResourceReclaimIntent) -> Result<(), ServiceError> {
        match intent {
            LocalResourceReclaimIntent::ExecuteLocalUndelegation(resource_trade_no) => {
                let result =
                    self.process_local_undelegation_execute(resource_trade_no.clone()).await;
                self.handle_local_undelegation_execute_failure_if_needed(&resource_trade_no, result)
                    .await
            }
            LocalResourceReclaimIntent::RecoverLocalUndelegation(resource_trade_no) => {
                self.process_local_undelegation_recover(resource_trade_no).await
            }
        }
    }

    fn local_undelegation_retry_wait_secs(retry_count: i64) -> i64 {
        let exponent = retry_count.clamp(0, 6) as u32;
        (60_i64 * (1_i64 << exponent)).min(3600)
    }

    async fn handle_local_undelegation_execute_failure_if_needed(
        &self,
        resource_trade_no: &str,
        result: Result<(), ServiceError>,
    ) -> Result<(), ServiceError> {
        let Err(err) = result else {
            return Ok(());
        };
        self.schedule_local_undelegation_rebuild_retry(resource_trade_no, &err).await
    }

    async fn process_local_undelegation_execute(
        &self,
        resource_trade_no: String,
    ) -> Result<(), ServiceError> {
        info!(
            resource_trade_no = %resource_trade_no,
            source = "local_resource_reclaim_shadow",
            "Processing local undelegation execution"
        );

        let affected = ApiResourceDelegationRepo::claim_build_slot(&self.pool, &resource_trade_no)
            .await
            .map_err(|e| ServiceError::Database(e.into()))?;
        if affected == 0 {
            info!(
                resource_trade_no = %resource_trade_no,
                source = "local_resource_reclaim_shadow",
                "Local undelegation execution was already claimed or completed"
            );
            return Ok(());
        }

        let delegation =
            ApiResourceDelegationRepo::get_by_resource_trade_no(&self.pool, &resource_trade_no)
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;

        if delegation.source != ApiResourceDelegationSource::Local
            || delegation.operation_type != ApiResourceDelegationOperationType::Undelegate
        {
            return Err(ServiceError::Parameter(format!(
                "local resource reclaim requires source=Local + Undelegate, got source={:?} operation={:?}",
                delegation.source, delegation.operation_type
            )));
        }

        if delegation.tx_hash.is_some() {
            info!(
                resource_trade_no = %resource_trade_no,
                source = "local_resource_reclaim_shadow",
                "Local undelegation already has tx_hash, skipping execution"
            );
            return Ok(());
        }

        let tx_hash = self.execute_tron_local_undelegation(&delegation).await?;
        let affected = ApiResourceDelegationRepo::mark_broadcast_success(
            &self.pool,
            &resource_trade_no,
            &tx_hash,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;

        if affected == 0 {
            info!(
                resource_trade_no = %resource_trade_no,
                tx_hash = %tx_hash,
                source = "local_resource_reclaim_shadow",
                "Local undelegation broadcast fact already committed"
            );
        } else {
            info!(
                resource_trade_no = %resource_trade_no,
                tx_hash = %tx_hash,
                source = "local_resource_reclaim_shadow",
                "Local undelegation broadcast fact committed"
            );
        }

        Ok(())
    }

    async fn process_local_undelegation_recover(
        &self,
        resource_trade_no: String,
    ) -> Result<(), ServiceError> {
        info!(
            resource_trade_no = %resource_trade_no,
            source = "local_resource_reclaim_shadow",
            "Processing local undelegation recover"
        );

        let delegation =
            ApiResourceDelegationRepo::get_by_resource_trade_no(&self.pool, &resource_trade_no)
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;

        if delegation.source != ApiResourceDelegationSource::Local
            || delegation.operation_type != ApiResourceDelegationOperationType::Undelegate
        {
            return Ok(());
        }

        if delegation.result_received_at.is_some() || delegation.err_code.is_some() {
            return Ok(());
        }

        let tx_hash =
            delegation.tx_hash.as_deref().filter(|s| !s.trim().is_empty()).ok_or_else(|| {
                ServiceError::Parameter("local undelegation recover requires tx_hash".to_string())
            })?;

        match ApiTransDomain::process_recovered_tx(
            &delegation.chain_code,
            &delegation.owner_address,
            tx_hash,
            0,
            "0",
        )
        .await
        {
            Ok(Some(resp)) => {
                let payload = format!("local_undelegation_recovered:{}", resp.tx_hash);
                ApiResourceDelegationRepo::mark_result_received(
                    &self.pool,
                    &resource_trade_no,
                    ApiResourceDelegationResultStatus::Success,
                    None,
                    None,
                    None,
                    Some(&payload),
                )
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;
                info!(
                    resource_trade_no = %resource_trade_no,
                    tx_hash = %tx_hash,
                    source = "local_resource_reclaim_shadow",
                    "Local undelegation recovered as success"
                );
                Ok(())
            }
            Ok(None) => self.schedule_local_undelegation_recover_retry(&resource_trade_no).await,
            Err(err) => {
                self.schedule_local_undelegation_rebuild_retry(&resource_trade_no, &err).await
            }
        }
    }

    async fn execute_tron_local_undelegation(
        &self,
        delegation: &ApiResourceDelegationEntity,
    ) -> Result<String, ServiceError> {
        if !delegation.chain_code.eq_ignore_ascii_case("tron") {
            return Err(ServiceError::Parameter(format!(
                "local resource reclaim only supports tron, got {}",
                delegation.chain_code
            )));
        }

        let trx_amount = Self::parse_resource_delegation_native_amount(&delegation.native_amount)?;
        let resource = Self::tron_resource_name(delegation.resource_type);
        let chain = ChainAdapterFactory::get_tron_adapter().await?;
        let _chain_rpc_guard =
            crate::infrastructure::chain_rpc_guard::acquire_if_guarded(&delegation.chain_code)
                .await;

        let args = UnDelegateArgs::new(
            &delegation.owner_address,
            &delegation.receiver_address,
            trx_amount,
            resource,
            None,
        )?;
        let raw = args.build_raw_transaction(chain.get_provider()).await?;
        let (tx_hash, raw_tx) = self.sign_tron_local_undelegation(delegation, raw).await?;
        let tx_resp =
            ApiTransDomain::broadcast_transfer(&delegation.chain_code, raw_tx, Some(&tx_hash))
                .await?;

        let Some(tx) = tx_resp else {
            info!(
                resource_trade_no = %delegation.resource_trade_no,
                tx_hash = %tx_hash,
                source = "local_resource_reclaim_shadow",
                "Local undelegation broadcast result uncertain"
            );
            return Err(ServiceError::Parameter(
                "local undelegation broadcast result uncertain".to_string(),
            ));
        };

        if tx.tx_hash != tx_hash {
            return Err(ServiceError::System(SystemError::Internal(
                "local undelegation tx_hash mismatch between build and broadcast".to_string(),
            )));
        }

        Ok(tx_hash)
    }

    async fn sign_tron_local_undelegation(
        &self,
        delegation: &ApiResourceDelegationEntity,
        mut raw: RawTransactionParams,
    ) -> Result<(String, RawTx), ServiceError> {
        let chain = ChainAdapterFactory::get_tron_adapter().await?;
        let provider = chain.get_provider();
        let consumer =
            provider.transfer_fee(&delegation.owner_address, None, &raw.raw_data_hex, 1).await?;
        let balance = chain.balance(&delegation.owner_address, None).await?;
        if balance.to::<i64>() < consumer.transaction_fee_i64() {
            return Err(ServiceError::Parameter(format!(
                "local undelegation balance is insufficient for tx fee: balance={}, need={}",
                balance,
                consumer.transaction_fee_i64()
            )));
        }

        let handles = get_context()?.get_handles_arc().await?;
        let private_key_manager = handles.get_global_private_key_manager();
        let private_key = private_key_manager
            .get_private_key(&delegation.owner_address, &delegation.chain_code)
            .await?;
        let sign = wallet_utils::sign::sign_tron(&raw.tx_id, &private_key, None)?;
        raw.signature.push(sign);

        let tx_hash = raw.tx_id.clone();
        let raw_tx = RawTx::Tron(
            raw,
            BillResourceConsume::new_tron(consumer.act_bandwidth() as u64, 0),
            consumer.transaction_fee(),
        );

        Ok((tx_hash, raw_tx))
    }

    fn parse_resource_delegation_native_amount(amount: &str) -> Result<i64, ServiceError> {
        let parsed = amount.trim().parse::<i64>().map_err(|_| {
            ServiceError::Parameter(format!("invalid resource delegation native amount: {amount}"))
        })?;
        if parsed <= 0 {
            return Err(ServiceError::Parameter(format!(
                "resource delegation native amount must be positive: {amount}"
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

    async fn schedule_local_undelegation_recover_retry(
        &self,
        resource_trade_no: &str,
    ) -> Result<(), ServiceError> {
        let task =
            ApiResourceDelegationRepo::get_by_resource_trade_no(&self.pool, resource_trade_no)
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;
        let wait_secs = Self::local_undelegation_retry_wait_secs(task.retry_count);
        let next_retry_at = chrono::Utc::now() + chrono::Duration::seconds(wait_secs);
        ApiResourceDelegationRepo::mark_recover_retry_wait(
            &self.pool,
            resource_trade_no,
            "recover_waiting",
            &next_retry_at.to_rfc3339(),
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;
        Ok(())
    }

    async fn schedule_local_undelegation_rebuild_retry(
        &self,
        resource_trade_no: &str,
        err: &ServiceError,
    ) -> Result<(), ServiceError> {
        let task =
            ApiResourceDelegationRepo::get_by_resource_trade_no(&self.pool, resource_trade_no)
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;
        let wait_secs = Self::local_undelegation_retry_wait_secs(task.retry_count);
        let next_retry_at = chrono::Utc::now() + chrono::Duration::seconds(wait_secs);
        ApiResourceDelegationRepo::reset_for_retry(
            &self.pool,
            resource_trade_no,
            if err.is_network_error() { "retry_build" } else { "retry_recover" },
            &next_retry_at.to_rfc3339(),
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;
        Ok(())
    }
}

pub struct LocalResourceReclaimDispatcherActor {
    worker: LocalResourceReclaimWorker,
    intent_rx: mpsc::Receiver<LocalResourceReclaimIntent>,
    shutdown_rx: broadcast::Receiver<()>,
}

impl LocalResourceReclaimDispatcherActor {
    pub fn new(
        worker: LocalResourceReclaimWorker,
        intent_rx: mpsc::Receiver<LocalResourceReclaimIntent>,
        shutdown_rx: broadcast::Receiver<()>,
    ) -> Self {
        Self { worker, intent_rx, shutdown_rx }
    }

    pub async fn run(mut self) {
        crate::infrastructure::system_ready::wait_system_ready().await;
        info!("Local resource reclaim shadow dispatcher actor running");

        loop {
            tokio::select! {
                _ = self.shutdown_rx.recv() => {
                    info!("Received shutdown signal for local resource reclaim dispatcher actor");
                    break;
                }
                Some(intent) = self.intent_rx.recv() => {
                    if let Err(e) = self.worker.handle(intent).await {
                        error!(error = %e, "Failed to handle local resource reclaim intent");
                    }
                }
                else => {
                    info!("Local resource reclaim intent channel closed");
                    break;
                }
            }
        }

        info!("Local resource reclaim shadow dispatcher actor stopped");
    }
}

#[derive(Debug)]
pub struct LocalResourceReclaimShadowActorSystem {
    shutdown_tx: broadcast::Sender<()>,
    scanner_handle: Option<tokio::task::JoinHandle<()>>,
    dispatcher_handle: Option<tokio::task::JoinHandle<()>>,
}

impl LocalResourceReclaimShadowActorSystem {
    pub fn new(pool: ApiTransactionDbPool) -> Self {
        let (shutdown_tx, shutdown_rx1) = broadcast::channel(1);
        let shutdown_rx2 = shutdown_tx.subscribe();
        let (intent_tx, intent_rx) = mpsc::channel(100);

        let scanner = Arc::new(LocalResourceReclaimScanner::new(pool.clone()));
        let worker = LocalResourceReclaimWorker::new(pool);

        info!(
            scan_interval_secs = scanner.config.scan_interval.as_secs(),
            max_items_per_scan = scanner.config.max_items_per_scan,
            "Local resource reclaim shadow runtime config"
        );

        let scanner_clone = scanner.clone();
        let intent_tx_clone = intent_tx.clone();
        tokio::spawn(async move {
            for intent in scanner_clone.scan_round().await {
                if let Err(e) = intent_tx_clone.send(intent).await {
                    error!(error = %e, "Failed to enqueue warm local resource reclaim intent");
                    break;
                }
            }
            info!("Warm local resource reclaim shadow scan completed");
        });

        let scanner_actor =
            LocalResourceReclaimScannerActor::new(scanner.clone(), intent_tx, shutdown_rx1);
        let scanner_handle = Some(tokio::spawn(async move {
            scanner_actor.run().await;
        }));

        let dispatcher_actor =
            LocalResourceReclaimDispatcherActor::new(worker, intent_rx, shutdown_rx2);
        let dispatcher_handle = Some(tokio::spawn(async move {
            dispatcher_actor.run().await;
        }));

        Self { shutdown_tx, scanner_handle, dispatcher_handle }
    }

    pub async fn stop(&mut self) {
        info!("Stopping local resource reclaim shadow system");

        let _ = self.shutdown_tx.send(());

        if let Some(handle) = self.scanner_handle.take() {
            if let Err(err) = handle.await {
                warn!(error = %err, "local resource reclaim scanner join failed during stop");
            }
        }

        if let Some(handle) = self.dispatcher_handle.take() {
            if let Err(err) = handle.await {
                warn!(error = %err, "local resource reclaim dispatcher join failed during stop");
            }
        }

        info!("Local resource reclaim shadow system stopped");
    }
}

pub(crate) async fn init(pool: ApiTransactionDbPool) -> LocalResourceReclaimShadowActorSystem {
    LocalResourceReclaimShadowActorSystem::new(pool)
}

pub async fn scan_and_process_once(pool: ApiTransactionDbPool) -> Result<(), ServiceError> {
    let scanner = LocalResourceReclaimScanner::new(pool.clone());
    let worker = LocalResourceReclaimWorker::new(pool);

    for intent in scanner.scan_round().await {
        worker.handle(intent).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wallet_database::{
        SqliteContext, entities::api_resource_delegation::NewApiResourceDelegation,
    };

    #[tokio::test]
    async fn scanner_owns_only_local_undelegation_execute_and_recover() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_root = dir.path().to_string_lossy().to_string();
        let pool = SqliteContext::new(&db_root, Some("api_transaction.db"))
            .await
            .expect("init api_transaction.db")
            .into_transaction_db_pool()
            .expect("transaction pool");

        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::local_undelegate(
                "uid",
                "rsc_local_undelegate_execute",
                "C_EXECUTE",
                ApiTradeType::Collect as i64,
                "owner",
                "receiver",
                "5",
                "1000",
            ),
        )
        .await
        .expect("insert executable local undelegate");

        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::local_undelegate(
                "uid",
                "rsc_local_undelegate_recover",
                "C_RECOVER",
                ApiTradeType::Collect as i64,
                "owner",
                "receiver",
                "5",
                "1000",
            ),
        )
        .await
        .expect("insert recoverable local undelegate");
        ApiResourceDelegationRepo::claim_build_slot(&pool, "rsc_local_undelegate_recover")
            .await
            .expect("claim build slot");
        ApiResourceDelegationRepo::mark_broadcast_success(
            &pool,
            "rsc_local_undelegate_recover",
            "tx_hash_recover",
        )
        .await
        .expect("mark broadcast");
        let scanner = LocalResourceReclaimScanner::with_config(
            pool,
            LocalResourceReclaimScannerConfig {
                scan_interval: Duration::from_secs(60),
                max_items_per_scan: 8,
            },
        );

        let intents = scanner.scan_round().await;
        assert!(intents.iter().any(|intent| matches!(
            intent,
            LocalResourceReclaimIntent::ExecuteLocalUndelegation(trade_no)
                if trade_no == "rsc_local_undelegate_execute"
        )));
        assert!(intents.iter().any(|intent| matches!(
            intent,
            LocalResourceReclaimIntent::RecoverLocalUndelegation(trade_no)
                if trade_no == "rsc_local_undelegate_recover"
        )));
    }

    #[tokio::test]
    async fn local_undelegation_retry_resets_execution_facts() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_root = dir.path().to_string_lossy().to_string();
        let pool = SqliteContext::new(&db_root, Some("api_transaction.db"))
            .await
            .expect("init api_transaction.db")
            .into_transaction_db_pool()
            .expect("transaction pool");

        let worker = LocalResourceReclaimWorker::new(pool.clone());

        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::local_undelegate(
                "uid",
                "rsc_local_undelegate_retry",
                "C_LOCAL_UNDELEGATE_RETRY",
                ApiTradeType::Collect as i64,
                "withdraw_owner",
                "receiver",
                "5",
                "1000",
            ),
        )
        .await
        .expect("insert local undelegate");
        ApiResourceDelegationRepo::mark_broadcast_success(
            &pool,
            "rsc_local_undelegate_retry",
            "tx_hash_retry",
        )
        .await
        .expect("mark broadcast");

        worker
            .schedule_local_undelegation_rebuild_retry(
                "rsc_local_undelegate_retry",
                &ServiceError::System(SystemError::Internal("recover failed".to_string())),
            )
            .await
            .expect("schedule retry");

        let persisted = ApiResourceDelegationRepo::get_by_resource_trade_no(
            &pool,
            "rsc_local_undelegate_retry",
        )
        .await
        .expect("load task");
        assert_eq!(persisted.tx_hash, None);
        assert_eq!(persisted.tx_status, None);
        assert_eq!(persisted.recover_status.as_deref(), Some("retry_recover"));
        assert!(persisted.next_retry_at.is_some());
        assert_eq!(persisted.retry_count, 1);
    }
}
