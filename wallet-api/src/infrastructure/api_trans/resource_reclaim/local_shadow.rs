use std::{fs, path::PathBuf, sync::Arc, time::Duration};

use tokio::sync::{broadcast, mpsc};
use tracing::{error, info, warn};
use wallet_chain_interact::{
    BillResourceConsume,
    tron::operations::{RawTransactionParams, TronTxOperation, stake::UnDelegateArgs},
};
use wallet_database::{
    ApiResourceDbPool,
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
    domain::{
        api_wallet::{adapter::tx::RawTx, trans::ApiTransDomain},
        chain::adapter::ChainAdapterFactory,
    },
    error::{service::ServiceError, system::SystemError},
    infrastructure::{
        api_trans::{
            resource_amount::parse_resource_delegation_native_trx_units,
            resource_delegation::{
                execute_resource_delegation, mark_recovered_resource_delegation_failure,
                record_resource_uncertain_and_maybe_rebroadcast,
                resource_delegation_is_built_unbroadcast, resource_delegation_retry_wait_secs,
                resource_recovery_error_is_uncertain,
            },
            shadow_rpc_policy,
        },
        runtime::time::new_production_interval,
    },
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
    api_resource_pool: ApiResourceDbPool,
    config: LocalResourceReclaimScannerConfig,
}

impl LocalResourceReclaimScanner {
    pub fn new(api_resource_pool: ApiResourceDbPool) -> Self {
        Self::with_config(api_resource_pool, LocalResourceReclaimScannerConfig::default())
    }

    pub fn with_config(
        api_resource_pool: ApiResourceDbPool,
        config: LocalResourceReclaimScannerConfig,
    ) -> Self {
        Self { api_resource_pool, config }
    }

    pub async fn scan_round(&self) -> Vec<LocalResourceReclaimIntent> {
        let mut intents = Vec::new();

        match ApiResourceDelegationRepo::scan_can_execute_for_origin_type_source_and_operation(
            &self.api_resource_pool,
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
            &self.api_resource_pool,
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

#[derive(Clone)]
pub struct LocalResourceReclaimWorker {
    ctx: &'static crate::context::Context,
}

impl LocalResourceReclaimWorker {
    pub fn new(ctx: &'static crate::context::Context) -> Self {
        Self { ctx }
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

    fn origin_trade_no<'a>(delegation: &'a ApiResourceDelegationEntity) -> &'a str {
        delegation.origin_trade_no.as_deref().unwrap_or("<missing>")
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

        let existing = ApiResourceDelegationRepo::get_by_resource_trade_no(
            &self.ctx.api_resource_pool()?,
            &resource_trade_no,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;
        let built_unbroadcast = resource_delegation_is_built_unbroadcast(&existing);
        if existing.last_broadcast_at.is_some() {
            return Ok(());
        }
        let affected = if built_unbroadcast {
            1
        } else {
            ApiResourceDelegationRepo::claim_build_slot(
                &self.ctx.api_resource_pool()?,
                &resource_trade_no,
            )
            .await
            .map_err(|e| ServiceError::Database(e.into()))?
        };
        if affected == 0 {
            info!(
                resource_trade_no = %resource_trade_no,
                source = "local_resource_reclaim_shadow",
                "Local undelegation execution was already claimed or completed"
            );
            return Ok(());
        }

        let delegation = ApiResourceDelegationRepo::get_by_resource_trade_no(
            &self.ctx.api_resource_pool()?,
            &resource_trade_no,
        )
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

        execute_resource_delegation(
            self.ctx,
            &self.ctx.api_resource_pool()?,
            &delegation,
            "local_resource_delegation_undelegate",
        )
        .await?;

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

        let delegation = ApiResourceDelegationRepo::get_by_resource_trade_no(
            &self.ctx.api_resource_pool()?,
            &resource_trade_no,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;

        if delegation.source != ApiResourceDelegationSource::Local
            || delegation.operation_type != ApiResourceDelegationOperationType::Undelegate
        {
            return Ok(());
        }

        if delegation.finished_at.is_some() || delegation.err_code.is_some() {
            info!(
                resource_trade_no = %resource_trade_no,
                origin_trade_no = %Self::origin_trade_no(&delegation),
                finished_at = ?delegation.finished_at,
                err_code = ?delegation.err_code,
                source = "local_resource_reclaim_shadow",
                "Local undelegation already reached a terminal fact, skipping recover"
            );
            return Ok(());
        }

        if resource_delegation_is_built_unbroadcast(&delegation) {
            execute_resource_delegation(
                self.ctx,
                &self.ctx.api_resource_pool()?,
                &delegation,
                "local_resource_delegation_undelegate",
            )
            .await?;
            return Ok(());
        }

        let tx_hash =
            delegation.tx_hash.as_deref().filter(|s| !s.trim().is_empty()).ok_or_else(|| {
                ServiceError::Parameter("local undelegation recover requires tx_hash".to_string())
            })?;

        match ApiTransDomain::process_recovered_tx(
            self.ctx,
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
                let transaction_time = resp
                    .transaction_time_ms
                    .and_then(|millis| {
                        chrono::DateTime::<chrono::Utc>::from_timestamp_millis(millis as i64)
                    })
                    .map(|time| time.to_rfc3339());
                ApiResourceDelegationRepo::mark_recovered_chain_success(
                    &self.ctx.api_resource_pool()?,
                    &resource_trade_no,
                    tx_hash,
                    transaction_time.as_deref(),
                    &payload,
                )
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;
                info!(
                    resource_trade_no = %resource_trade_no,
                    origin_trade_no = %Self::origin_trade_no(&delegation),
                    tx_hash = %tx_hash,
                    source = "local_resource_reclaim_shadow",
                    "Local undelegation recovered as success"
                );
                Ok(())
            }
            Ok(None) => {
                let rebroadcast = record_resource_uncertain_and_maybe_rebroadcast(
                    &self.ctx.api_resource_pool()?,
                    &resource_trade_no,
                )
                .await?;
                if rebroadcast {
                    return Ok(());
                }
                Ok(())
            }
            Err(err) if resource_recovery_error_is_uncertain(&err) => {
                record_resource_uncertain_and_maybe_rebroadcast(
                    &self.ctx.api_resource_pool()?,
                    &resource_trade_no,
                )
                .await?;
                Ok(())
            }
            Err(err) if Self::is_transaction_expired_error(&err) => {
                self.schedule_local_undelegation_rebuild_retry(&resource_trade_no, &err).await
            }
            Err(err) => {
                mark_recovered_resource_delegation_failure(
                    &self.ctx.api_resource_pool()?,
                    &delegation,
                    tx_hash,
                    &err,
                    "local_undelegation_recovered_failure",
                )
                .await?;
                Ok(())
            }
        }
    }

    fn is_transaction_expired_error(err: &ServiceError) -> bool {
        err.to_string().contains("Transaction expired")
    }

    async fn schedule_local_undelegation_rebuild_retry(
        &self,
        resource_trade_no: &str,
        err: &ServiceError,
    ) -> Result<(), ServiceError> {
        let task = ApiResourceDelegationRepo::get_by_resource_trade_no(
            &self.ctx.api_resource_pool()?,
            resource_trade_no,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;
        ApiResourceDelegationRepo::reset_for_rebuild(
            &self.ctx.api_resource_pool()?,
            resource_trade_no,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;
        info!(
            resource_trade_no = %resource_trade_no,
            origin_trade_no = %Self::origin_trade_no(&task),
            error = %err,
            source = "local_resource_reclaim_shadow",
            "Local undelegation reset for rebuild"
        );
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
    pub fn new(ctx: &'static crate::context::Context) -> Result<Self, ServiceError> {
        let api_resource_pool = ctx.api_resource_pool()?;
        let (shutdown_tx, shutdown_rx1) = broadcast::channel(1);
        let shutdown_rx2 = shutdown_tx.subscribe();
        let (intent_tx, intent_rx) = mpsc::channel(100);

        let scanner = Arc::new(LocalResourceReclaimScanner::new(api_resource_pool.clone()));
        let worker = LocalResourceReclaimWorker::new(ctx);

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

        Ok(Self { shutdown_tx, scanner_handle, dispatcher_handle })
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

pub(crate) async fn init(
    ctx: &'static crate::context::Context,
) -> Result<LocalResourceReclaimShadowActorSystem, ServiceError> {
    LocalResourceReclaimShadowActorSystem::new(ctx)
}

pub async fn scan_and_process_once(
    ctx: &'static crate::context::Context,
) -> Result<(), ServiceError> {
    let api_resource_pool = ctx.api_resource_pool()?;
    let scanner = LocalResourceReclaimScanner::new(api_resource_pool.clone());
    let worker = LocalResourceReclaimWorker::new(ctx);

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

    async fn test_ctx() -> &'static crate::context::Context {
        crate::testkit::context::api_trans_test_ctx().await
    }

    #[tokio::test]
    async fn scanner_owns_only_local_undelegation_execute_and_recover() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_root = dir.path().to_string_lossy().to_string();
        let pool = SqliteContext::new(&db_root, Some("api_resource.db"))
            .await
            .expect("init api_resource.db")
            .into_api_resource_db_pool()
            .expect("resource pool");

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
    async fn local_undelegation_rebuild_resets_execution_facts() {
        let ctx = test_ctx().await;
        let pool = ctx.api_resource_pool().expect("resource pool");
        let worker = LocalResourceReclaimWorker::new(ctx);

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
    }

    #[test]
    fn local_undelegation_treats_transaction_expired_as_rebuildable() {
        let expired = ServiceError::System(SystemError::Internal(
            "Chain interact error: `rpc error Transaction expired`".to_string(),
        ));
        let other = ServiceError::System(SystemError::Internal(
            "Chain interact error: `broadcasted tx failed`".to_string(),
        ));

        assert!(LocalResourceReclaimWorker::is_transaction_expired_error(&expired));
        assert!(!LocalResourceReclaimWorker::is_transaction_expired_error(&other));
    }

    #[tokio::test]
    async fn scanner_reopens_execute_immediately_after_rebuild_reset() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_root = dir.path().to_string_lossy().to_string();
        let pool = SqliteContext::new(&db_root, Some("api_resource.db"))
            .await
            .expect("init api_resource.db")
            .into_api_resource_db_pool()
            .expect("resource pool");

        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::local_undelegate(
                "uid",
                "rsc_local_undelegate_future_retry",
                "C_FUTURE_RETRY",
                ApiTradeType::Collect as i64,
                "owner",
                "receiver",
                "5",
                "1000",
            ),
        )
        .await
        .expect("insert local undelegate");

        ApiResourceDelegationRepo::reset_for_rebuild(&pool, "rsc_local_undelegate_future_retry")
            .await
            .expect("reset for rebuild");

        let scanner = LocalResourceReclaimScanner::with_config(
            pool.clone(),
            LocalResourceReclaimScannerConfig {
                scan_interval: Duration::from_secs(60),
                max_items_per_scan: 8,
            },
        );
        let intents = scanner.scan_round().await;
        assert!(intents.iter().any(|intent| matches!(
            intent,
            LocalResourceReclaimIntent::ExecuteLocalUndelegation(trade_no)
                if trade_no == "rsc_local_undelegate_future_retry"
        )));
    }
}
