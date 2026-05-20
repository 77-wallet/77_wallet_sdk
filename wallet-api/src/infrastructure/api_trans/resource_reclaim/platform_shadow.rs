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
            ApiResourceDelegationRecoverStatus, ApiResourceDelegationResultStatus,
            ApiResourceDelegationSource,
        },
        api_resource_type::ApiResourceType,
        api_trade_type::ApiTradeType,
    },
    repositories::api_wallet::resource_delegation::ApiResourceDelegationRepo,
};
use wallet_utils::RetryableError as _;

use crate::{
    context::{CONTEXT, get_context},
    domain::{
        api_wallet::{adapter::tx::RawTx, trans::ApiTransDomain},
        chain::adapter::ChainAdapterFactory,
    },
    error::{service::ServiceError, system::SystemError},
    infrastructure::{
        api_trans::{
            resource_ack_type::resource_delegation_ack_trans_type,
            resource_amount::parse_resource_delegation_native_trx_units, shadow_rpc_policy,
        },
        runtime::time::new_production_interval,
    },
};
use wallet_transport_backend::request::api_wallet::transaction::{TransAckType, TransEventAckReq};

#[derive(Debug, Clone)]
pub enum PlatformResourceReclaimIntent {
    SendPlatformUndelegationTaskAck(String),
    ExecutePlatformUndelegation(String),
    RecoverPlatformUndelegation(String),
}

#[derive(Debug, Clone)]
pub struct PlatformResourceReclaimScannerConfig {
    pub scan_interval: Duration,
    pub max_items_per_scan: usize,
}

impl Default for PlatformResourceReclaimScannerConfig {
    fn default() -> Self {
        let scan_interval_secs = shadow_rpc_policy::read_u64_env(
            "PLATFORM_RESOURCE_RECLAIM_SHADOW_SCAN_INTERVAL_SECS",
            30,
            10,
            120,
        );
        let max_items_per_scan = shadow_rpc_policy::read_u64_env(
            "PLATFORM_RESOURCE_RECLAIM_SHADOW_MAX_ITEMS_PER_SCAN",
            20,
            1,
            200,
        ) as usize;

        Self { scan_interval: Duration::from_secs(scan_interval_secs), max_items_per_scan }
    }
}

#[derive(Debug, Clone)]
pub struct PlatformResourceReclaimScanner {
    pool: ApiTransactionDbPool,
    config: PlatformResourceReclaimScannerConfig,
}

impl PlatformResourceReclaimScanner {
    pub fn new(pool: ApiTransactionDbPool) -> Self {
        Self::with_config(pool, PlatformResourceReclaimScannerConfig::default())
    }

    pub fn with_config(
        pool: ApiTransactionDbPool,
        config: PlatformResourceReclaimScannerConfig,
    ) -> Self {
        Self { pool, config }
    }

    pub async fn scan_round(&self) -> Vec<PlatformResourceReclaimIntent> {
        let mut intents = Vec::new();

        self.scan_collect_platform_undelegation_task_ack(&mut intents).await;
        self.scan_withdraw_platform_undelegation_task_ack(&mut intents).await;
        self.scan_collect_platform_undelegation(&mut intents).await;
        self.scan_withdraw_platform_undelegation(&mut intents).await;
        self.scan_collect_platform_undelegation_recover(&mut intents).await;
        self.scan_withdraw_platform_undelegation_recover(&mut intents).await;

        intents
    }

    async fn scan_collect_platform_undelegation_task_ack(
        &self,
        intents: &mut Vec<PlatformResourceReclaimIntent>,
    ) {
        match ApiResourceDelegationRepo::scan_need_task_ack_for_origin_type_source_and_operation(
            &self.pool,
            ApiTradeType::Collect as i64,
            ApiResourceDelegationSource::Platform,
            ApiResourceDelegationOperationType::Undelegate,
            self.config.max_items_per_scan,
        )
        .await
        {
            Ok(records) => {
                for record in records {
                    intents.push(PlatformResourceReclaimIntent::SendPlatformUndelegationTaskAck(
                        record.resource_trade_no,
                    ));
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to scan collect platform undelegation task ACK records");
            }
        }
    }

    async fn scan_withdraw_platform_undelegation_task_ack(
        &self,
        intents: &mut Vec<PlatformResourceReclaimIntent>,
    ) {
        match ApiResourceDelegationRepo::scan_need_task_ack_for_origin_type_source_and_operation(
            &self.pool,
            ApiTradeType::Withdraw as i64,
            ApiResourceDelegationSource::Platform,
            ApiResourceDelegationOperationType::Undelegate,
            self.config.max_items_per_scan,
        )
        .await
        {
            Ok(records) => {
                for record in records {
                    intents.push(PlatformResourceReclaimIntent::SendPlatformUndelegationTaskAck(
                        record.resource_trade_no,
                    ));
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to scan withdraw platform undelegation task ACK records");
            }
        }
    }

    async fn scan_collect_platform_undelegation(
        &self,
        intents: &mut Vec<PlatformResourceReclaimIntent>,
    ) {
        match ApiResourceDelegationRepo::scan_can_execute_for_origin_type_source_and_operation(
            &self.pool,
            ApiTradeType::Collect as i64,
            ApiResourceDelegationSource::Platform,
            ApiResourceDelegationOperationType::Undelegate,
            self.config.max_items_per_scan,
        )
        .await
        {
            Ok(records) => {
                for record in records {
                    intents.push(PlatformResourceReclaimIntent::ExecutePlatformUndelegation(
                        record.resource_trade_no,
                    ));
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to scan executable collect platform undelegation records");
            }
        }
    }

    async fn scan_withdraw_platform_undelegation(
        &self,
        intents: &mut Vec<PlatformResourceReclaimIntent>,
    ) {
        match ApiResourceDelegationRepo::scan_can_execute_for_origin_type_source_and_operation(
            &self.pool,
            ApiTradeType::Withdraw as i64,
            ApiResourceDelegationSource::Platform,
            ApiResourceDelegationOperationType::Undelegate,
            self.config.max_items_per_scan,
        )
        .await
        {
            Ok(records) => {
                for record in records {
                    intents.push(PlatformResourceReclaimIntent::ExecutePlatformUndelegation(
                        record.resource_trade_no,
                    ));
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to scan executable withdraw platform undelegation records");
            }
        }
    }

    async fn scan_collect_platform_undelegation_recover(
        &self,
        intents: &mut Vec<PlatformResourceReclaimIntent>,
    ) {
        match ApiResourceDelegationRepo::scan_can_recover_by_origin_type_source_and_operation(
            &self.pool,
            ApiTradeType::Collect as i64,
            ApiResourceDelegationSource::Platform,
            ApiResourceDelegationOperationType::Undelegate,
            self.config.max_items_per_scan,
        )
        .await
        {
            Ok(records) => {
                for record in records {
                    intents.push(PlatformResourceReclaimIntent::RecoverPlatformUndelegation(
                        record.resource_trade_no,
                    ));
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to scan recoverable collect platform undelegation records");
            }
        }
    }

    async fn scan_withdraw_platform_undelegation_recover(
        &self,
        intents: &mut Vec<PlatformResourceReclaimIntent>,
    ) {
        match ApiResourceDelegationRepo::scan_can_recover_by_origin_type_source_and_operation(
            &self.pool,
            ApiTradeType::Withdraw as i64,
            ApiResourceDelegationSource::Platform,
            ApiResourceDelegationOperationType::Undelegate,
            self.config.max_items_per_scan,
        )
        .await
        {
            Ok(records) => {
                for record in records {
                    intents.push(PlatformResourceReclaimIntent::RecoverPlatformUndelegation(
                        record.resource_trade_no,
                    ));
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to scan recoverable withdraw platform undelegation records");
            }
        }
    }
}

pub struct PlatformResourceReclaimScannerActor {
    scanner: Arc<PlatformResourceReclaimScanner>,
    intent_tx: mpsc::Sender<PlatformResourceReclaimIntent>,
    shutdown_rx: broadcast::Receiver<()>,
}

impl PlatformResourceReclaimScannerActor {
    pub fn new(
        scanner: Arc<PlatformResourceReclaimScanner>,
        intent_tx: mpsc::Sender<PlatformResourceReclaimIntent>,
        shutdown_rx: broadcast::Receiver<()>,
    ) -> Self {
        Self { scanner, intent_tx, shutdown_rx }
    }

    pub async fn run(mut self) {
        crate::infrastructure::system_ready::wait_system_ready().await;
        info!("Platform resource reclaim shadow scanner actor running");

        let mut interval = new_production_interval(self.scanner.config.scan_interval);

        loop {
            tokio::select! {
                _ = self.shutdown_rx.recv() => {
                    info!("Received shutdown signal for platform resource reclaim scanner actor");
                    break;
                }
                _ = interval.tick() => {
                    for intent in self.scanner.scan_round().await {
                        if let Err(e) = self.intent_tx.send(intent).await {
                            error!(error = %e, "Failed to enqueue platform resource reclaim intent");
                            break;
                        }
                    }
                }
            }
        }

        info!("Platform resource reclaim shadow scanner actor stopped");
    }
}

#[derive(Debug, Clone)]
pub struct PlatformResourceReclaimWorker {
    pool: ApiTransactionDbPool,
}

impl PlatformResourceReclaimWorker {
    pub fn new(pool: ApiTransactionDbPool) -> Self {
        Self { pool }
    }

    pub async fn handle(&self, intent: PlatformResourceReclaimIntent) -> Result<(), ServiceError> {
        match intent {
            PlatformResourceReclaimIntent::SendPlatformUndelegationTaskAck(resource_trade_no) => {
                self.process_platform_undelegation_task_ack(resource_trade_no).await
            }
            PlatformResourceReclaimIntent::ExecutePlatformUndelegation(resource_trade_no) => {
                let result =
                    self.process_platform_undelegation_execute(resource_trade_no.clone()).await;
                self.handle_platform_undelegation_execute_failure_if_needed(
                    &resource_trade_no,
                    result,
                )
                .await
            }
            PlatformResourceReclaimIntent::RecoverPlatformUndelegation(resource_trade_no) => {
                self.process_platform_undelegation_recover(resource_trade_no).await
            }
        }
    }

    fn platform_undelegation_retry_wait_secs(retry_count: i64) -> i64 {
        let exponent = retry_count.clamp(0, 6) as u32;
        (60_i64 * (1_i64 << exponent)).min(3600)
    }

    fn origin_trade_no<'a>(delegation: &'a ApiResourceDelegationEntity) -> &'a str {
        delegation.origin_trade_no.as_deref().unwrap_or("<missing>")
    }

    async fn process_platform_undelegation_task_ack(
        &self,
        resource_trade_no: String,
    ) -> Result<(), ServiceError> {
        info!(
            resource_trade_no = %resource_trade_no,
            source = "platform_resource_reclaim_shadow",
            "Processing platform undelegation task ACK"
        );

        let resource_task =
            ApiResourceDelegationRepo::get_by_resource_trade_no(&self.pool, &resource_trade_no)
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;

        if resource_task.task_ack_sent_at.is_some() {
            info!(
                resource_trade_no = %resource_trade_no,
                source = "platform_resource_reclaim_shadow",
                "Platform undelegation task ACK already sent"
            );
            return Ok(());
        }

        if resource_task.source != ApiResourceDelegationSource::Platform
            || resource_task.operation_type != ApiResourceDelegationOperationType::Undelegate
        {
            return Err(ServiceError::Parameter(format!(
                "platform undelegation task ACK requires source=Platform + Undelegate, got source={:?} operation={:?}",
                resource_task.source, resource_task.operation_type
            )));
        }

        let backend_api = CONTEXT.get().unwrap().get_global_backend_api();
        backend_api
            .trans_event_ack(&TransEventAckReq::new(
                &resource_trade_no,
                resource_delegation_ack_trans_type(&resource_task),
                TransAckType::Tx,
            ))
            .await?;

        let affected =
            ApiResourceDelegationRepo::mark_task_ack_sent(&self.pool, &resource_trade_no)
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;
        if affected == 0 {
            warn!(
                resource_trade_no = %resource_trade_no,
                source = "platform_resource_reclaim_shadow",
                "Platform undelegation task ACK marked 0 rows"
            );
        } else {
            info!(
                resource_trade_no = %resource_trade_no,
                source = "platform_resource_reclaim_shadow",
                "Platform undelegation task ACK sent successfully"
            );
        }

        Ok(())
    }

    async fn handle_platform_undelegation_execute_failure_if_needed(
        &self,
        resource_trade_no: &str,
        result: Result<(), ServiceError>,
    ) -> Result<(), ServiceError> {
        let Err(err) = result else {
            return Ok(());
        };
        self.schedule_platform_undelegation_rebuild_retry(resource_trade_no, &err).await
    }

    async fn process_platform_undelegation_execute(
        &self,
        resource_trade_no: String,
    ) -> Result<(), ServiceError> {
        info!(
            resource_trade_no = %resource_trade_no,
            source = "platform_resource_reclaim_shadow",
            "Processing platform undelegation execution"
        );

        let affected = ApiResourceDelegationRepo::claim_build_slot(&self.pool, &resource_trade_no)
            .await
            .map_err(|e| ServiceError::Database(e.into()))?;
        if affected == 0 {
            info!(
                resource_trade_no = %resource_trade_no,
                source = "platform_resource_reclaim_shadow",
                "Platform undelegation execution was already claimed or completed"
            );
            return Ok(());
        }

        let delegation =
            ApiResourceDelegationRepo::get_by_resource_trade_no(&self.pool, &resource_trade_no)
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;

        if delegation.source != ApiResourceDelegationSource::Platform
            || delegation.operation_type != ApiResourceDelegationOperationType::Undelegate
        {
            return Err(ServiceError::Parameter(format!(
                "platform resource reclaim requires source=Platform + Undelegate, got source={:?} operation={:?}",
                delegation.source, delegation.operation_type
            )));
        }

        if delegation.tx_hash.is_some() {
            info!(
                resource_trade_no = %resource_trade_no,
                origin_trade_no = %Self::origin_trade_no(&delegation),
                retry_count = delegation.retry_count,
                recover_status = ?delegation.recover_status,
                source = "platform_resource_reclaim_shadow",
                "Platform undelegation already has tx_hash, skipping execution"
            );
            return Ok(());
        }

        let tx_hash = self.execute_tron_platform_undelegation(&delegation).await?;
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
                origin_trade_no = %Self::origin_trade_no(&delegation),
                tx_hash = %tx_hash,
                retry_count = delegation.retry_count,
                source = "platform_resource_reclaim_shadow",
                "Platform undelegation broadcast fact already committed"
            );
        } else {
            info!(
                resource_trade_no = %resource_trade_no,
                origin_trade_no = %Self::origin_trade_no(&delegation),
                tx_hash = %tx_hash,
                retry_count = delegation.retry_count,
                source = "platform_resource_reclaim_shadow",
                "Platform undelegation broadcast fact committed"
            );
        }

        Ok(())
    }

    async fn process_platform_undelegation_recover(
        &self,
        resource_trade_no: String,
    ) -> Result<(), ServiceError> {
        info!(
            resource_trade_no = %resource_trade_no,
            source = "platform_resource_reclaim_shadow",
            "Processing platform undelegation recover"
        );

        let delegation =
            ApiResourceDelegationRepo::get_by_resource_trade_no(&self.pool, &resource_trade_no)
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;

        if delegation.source != ApiResourceDelegationSource::Platform
            || delegation.operation_type != ApiResourceDelegationOperationType::Undelegate
        {
            return Ok(());
        }

        if delegation.result_received_at.is_some() || delegation.err_code.is_some() {
            info!(
                resource_trade_no = %resource_trade_no,
                origin_trade_no = %Self::origin_trade_no(&delegation),
                result_received_at = ?delegation.result_received_at,
                err_code = ?delegation.err_code,
                source = "platform_resource_reclaim_shadow",
                "Platform undelegation already reached a terminal fact, skipping recover"
            );
            return Ok(());
        }

        let tx_hash =
            delegation.tx_hash.as_deref().filter(|s| !s.trim().is_empty()).ok_or_else(|| {
                ServiceError::Parameter(
                    "platform undelegation recover requires tx_hash".to_string(),
                )
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
                let payload = format!("platform_undelegation_recovered:{}", resp.tx_hash);
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
                    origin_trade_no = %Self::origin_trade_no(&delegation),
                    tx_hash = %tx_hash,
                    retry_count = delegation.retry_count,
                    source = "platform_resource_reclaim_shadow",
                    "Platform undelegation recovered as success"
                );
                Ok(())
            }
            Ok(None) => self.schedule_platform_undelegation_recover_retry(&resource_trade_no).await,
            Err(err) => {
                self.schedule_platform_undelegation_rebuild_retry(&resource_trade_no, &err).await
            }
        }
    }

    async fn execute_tron_platform_undelegation(
        &self,
        delegation: &ApiResourceDelegationEntity,
    ) -> Result<String, ServiceError> {
        if !delegation.chain_code.eq_ignore_ascii_case("tron") {
            return Err(ServiceError::Parameter(format!(
                "platform resource reclaim only supports tron, got {}",
                delegation.chain_code
            )));
        }

        let trx_amount = parse_resource_delegation_native_trx_units(&delegation.native_amount)?;
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
        let (tx_hash, raw_tx) = self.sign_tron_platform_undelegation(delegation, raw).await?;
        let tx_resp =
            ApiTransDomain::broadcast_transfer(&delegation.chain_code, raw_tx, Some(&tx_hash))
                .await?;

        let Some(tx) = tx_resp else {
            info!(
                resource_trade_no = %delegation.resource_trade_no,
                tx_hash = %tx_hash,
                source = "platform_resource_reclaim_shadow",
                "Platform undelegation broadcast result uncertain"
            );
            return Err(ServiceError::Parameter(
                "platform undelegation broadcast result uncertain".to_string(),
            ));
        };

        if tx.tx_hash != tx_hash {
            return Err(ServiceError::System(SystemError::Internal(
                "platform undelegation tx_hash mismatch between build and broadcast".to_string(),
            )));
        }

        Ok(tx_hash)
    }

    async fn sign_tron_platform_undelegation(
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
                "platform undelegation balance is insufficient for tx fee: balance={}, need={}",
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

    fn tron_resource_name(resource_type: ApiResourceType) -> &'static str {
        match resource_type {
            ApiResourceType::Bandwidth => "bandwidth",
            ApiResourceType::Energy => "energy",
        }
    }

    async fn schedule_platform_undelegation_recover_retry(
        &self,
        resource_trade_no: &str,
    ) -> Result<(), ServiceError> {
        let task =
            ApiResourceDelegationRepo::get_by_resource_trade_no(&self.pool, resource_trade_no)
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;
        let wait_secs = Self::platform_undelegation_retry_wait_secs(task.retry_count);
        let next_retry_at = chrono::Utc::now() + chrono::Duration::seconds(wait_secs);
        ApiResourceDelegationRepo::mark_recover_retry_wait(
            &self.pool,
            resource_trade_no,
            ApiResourceDelegationRecoverStatus::RecoverWaiting,
            &next_retry_at.to_rfc3339(),
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;
        info!(
            resource_trade_no = %resource_trade_no,
            origin_trade_no = %Self::origin_trade_no(&task),
            retry_count = task.retry_count + 1,
            recover_status = ?ApiResourceDelegationRecoverStatus::RecoverWaiting,
            next_retry_at = %next_retry_at.to_rfc3339(),
            wait_secs,
            source = "platform_resource_reclaim_shadow",
            "Platform undelegation recover scheduled for retry"
        );
        Ok(())
    }

    async fn schedule_platform_undelegation_rebuild_retry(
        &self,
        resource_trade_no: &str,
        err: &ServiceError,
    ) -> Result<(), ServiceError> {
        let task =
            ApiResourceDelegationRepo::get_by_resource_trade_no(&self.pool, resource_trade_no)
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;
        let wait_secs = Self::platform_undelegation_retry_wait_secs(task.retry_count);
        let next_retry_at = chrono::Utc::now() + chrono::Duration::seconds(wait_secs);
        let next_status = if err.is_network_error() {
            ApiResourceDelegationRecoverStatus::RetryBuild
        } else {
            ApiResourceDelegationRecoverStatus::RetryRecover
        };
        ApiResourceDelegationRepo::reset_for_retry(
            &self.pool,
            resource_trade_no,
            next_status,
            &next_retry_at.to_rfc3339(),
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;
        info!(
            resource_trade_no = %resource_trade_no,
            origin_trade_no = %Self::origin_trade_no(&task),
            retry_count = task.retry_count + 1,
            recover_status = ?next_status,
            next_retry_at = %next_retry_at.to_rfc3339(),
            wait_secs,
            error = %err,
            source = "platform_resource_reclaim_shadow",
            "Platform undelegation reset for retry"
        );
        Ok(())
    }
}

pub struct PlatformResourceReclaimDispatcherActor {
    worker: PlatformResourceReclaimWorker,
    intent_rx: mpsc::Receiver<PlatformResourceReclaimIntent>,
    shutdown_rx: broadcast::Receiver<()>,
}

impl PlatformResourceReclaimDispatcherActor {
    pub fn new(
        worker: PlatformResourceReclaimWorker,
        intent_rx: mpsc::Receiver<PlatformResourceReclaimIntent>,
        shutdown_rx: broadcast::Receiver<()>,
    ) -> Self {
        Self { worker, intent_rx, shutdown_rx }
    }

    pub async fn run(mut self) {
        crate::infrastructure::system_ready::wait_system_ready().await;
        info!("Platform resource reclaim shadow dispatcher actor running");

        loop {
            tokio::select! {
                _ = self.shutdown_rx.recv() => {
                    info!("Received shutdown signal for platform resource reclaim dispatcher actor");
                    break;
                }
                Some(intent) = self.intent_rx.recv() => {
                    if let Err(e) = self.worker.handle(intent).await {
                        error!(error = %e, "Failed to handle platform resource reclaim intent");
                    }
                }
                else => {
                    info!("Platform resource reclaim intent channel closed");
                    break;
                }
            }
        }

        info!("Platform resource reclaim shadow dispatcher actor stopped");
    }
}

#[derive(Debug)]
pub struct PlatformResourceReclaimShadowActorSystem {
    shutdown_tx: broadcast::Sender<()>,
    scanner_handle: Option<tokio::task::JoinHandle<()>>,
    dispatcher_handle: Option<tokio::task::JoinHandle<()>>,
}

impl PlatformResourceReclaimShadowActorSystem {
    pub fn new(pool: ApiTransactionDbPool) -> Self {
        let (shutdown_tx, shutdown_rx1) = broadcast::channel(1);
        let shutdown_rx2 = shutdown_tx.subscribe();
        let (intent_tx, intent_rx) = mpsc::channel(100);

        let scanner = Arc::new(PlatformResourceReclaimScanner::new(pool.clone()));
        let worker = PlatformResourceReclaimWorker::new(pool);

        info!(
            scan_interval_secs = scanner.config.scan_interval.as_secs(),
            max_items_per_scan = scanner.config.max_items_per_scan,
            "Platform resource reclaim shadow runtime config"
        );

        let scanner_clone = scanner.clone();
        let intent_tx_clone = intent_tx.clone();
        tokio::spawn(async move {
            for intent in scanner_clone.scan_round().await {
                if let Err(e) = intent_tx_clone.send(intent).await {
                    error!(error = %e, "Failed to enqueue warm platform resource reclaim intent");
                    break;
                }
            }
            info!("Warm platform resource reclaim shadow scan completed");
        });

        let scanner_actor =
            PlatformResourceReclaimScannerActor::new(scanner.clone(), intent_tx, shutdown_rx1);
        let scanner_handle = Some(tokio::spawn(async move {
            scanner_actor.run().await;
        }));

        let dispatcher_actor =
            PlatformResourceReclaimDispatcherActor::new(worker, intent_rx, shutdown_rx2);
        let dispatcher_handle = Some(tokio::spawn(async move {
            dispatcher_actor.run().await;
        }));

        Self { shutdown_tx, scanner_handle, dispatcher_handle }
    }

    pub async fn stop(&mut self) {
        info!("Stopping platform resource reclaim shadow system");

        let _ = self.shutdown_tx.send(());

        if let Some(handle) = self.scanner_handle.take() {
            if let Err(err) = handle.await {
                warn!(error = %err, "platform resource reclaim scanner join failed during stop");
            }
        }

        if let Some(handle) = self.dispatcher_handle.take() {
            if let Err(err) = handle.await {
                warn!(error = %err, "platform resource reclaim dispatcher join failed during stop");
            }
        }

        info!("Platform resource reclaim shadow system stopped");
    }
}

pub(crate) async fn init(pool: ApiTransactionDbPool) -> PlatformResourceReclaimShadowActorSystem {
    PlatformResourceReclaimShadowActorSystem::new(pool)
}

pub async fn scan_and_process_once(pool: ApiTransactionDbPool) -> Result<(), ServiceError> {
    let scanner = PlatformResourceReclaimScanner::new(pool.clone());
    let worker = PlatformResourceReclaimWorker::new(pool);

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
    async fn scanner_finds_platform_undelegation_for_collect_and_withdraw() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_root = dir.path().to_string_lossy().to_string();
        let pool = SqliteContext::new(&db_root, Some("api_transaction.db"))
            .await
            .expect("init api_transaction.db")
            .into_transaction_db_pool()
            .expect("transaction pool");

        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate_task(
                "uid",
                "rsc_platform_undelegate_collect",
                ApiTradeType::Collect,
                ApiResourceDelegationOperationType::Undelegate,
                "tron",
                "owner",
                "receiver",
                ApiResourceType::Energy,
                "5",
                "1000",
            ),
        )
        .await
        .expect("insert collect platform undelegate");
        ApiResourceDelegationRepo::mark_task_ack_sent(&pool, "rsc_platform_undelegate_collect")
            .await
            .expect("mark task ack");

        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate_task(
                "uid",
                "rsc_platform_undelegate_withdraw",
                ApiTradeType::Withdraw,
                ApiResourceDelegationOperationType::Undelegate,
                "tron",
                "owner",
                "receiver",
                ApiResourceType::Energy,
                "5",
                "1000",
            ),
        )
        .await
        .expect("insert withdraw platform undelegate");
        ApiResourceDelegationRepo::mark_task_ack_sent(&pool, "rsc_platform_undelegate_withdraw")
            .await
            .expect("mark task ack");

        let scanner = PlatformResourceReclaimScanner::with_config(
            pool,
            PlatformResourceReclaimScannerConfig {
                scan_interval: Duration::from_secs(60),
                max_items_per_scan: 8,
            },
        );

        let intents = scanner.scan_round().await;
        assert!(intents.iter().any(|intent| matches!(
            intent,
            PlatformResourceReclaimIntent::ExecutePlatformUndelegation(trade_no)
                if trade_no == "rsc_platform_undelegate_collect"
        )));
        assert!(intents.iter().any(|intent| matches!(
            intent,
            PlatformResourceReclaimIntent::ExecutePlatformUndelegation(trade_no)
                if trade_no == "rsc_platform_undelegate_withdraw"
        )));
    }

    #[tokio::test]
    async fn platform_reclaim_scanner_finds_task_ack_before_execute() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_root = dir.path().to_string_lossy().to_string();
        let pool = SqliteContext::new(&db_root, Some("api_transaction.db"))
            .await
            .expect("init api_transaction.db")
            .into_transaction_db_pool()
            .expect("transaction pool");

        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate_task(
                "uid",
                "rsc_platform_undelegate_needs_ack",
                ApiTradeType::Collect,
                ApiResourceDelegationOperationType::Undelegate,
                "tron",
                "owner",
                "receiver",
                ApiResourceType::Energy,
                "5",
                "1000",
            ),
        )
        .await
        .expect("insert collect platform undelegate");

        let scanner = PlatformResourceReclaimScanner::with_config(
            pool,
            PlatformResourceReclaimScannerConfig {
                scan_interval: Duration::from_secs(60),
                max_items_per_scan: 8,
            },
        );

        let intents = scanner.scan_round().await;
        assert!(intents.iter().any(|intent| matches!(
            intent,
            PlatformResourceReclaimIntent::SendPlatformUndelegationTaskAck(trade_no)
                if trade_no == "rsc_platform_undelegate_needs_ack"
        )));
        assert!(!intents.iter().any(|intent| matches!(
            intent,
            PlatformResourceReclaimIntent::ExecutePlatformUndelegation(trade_no)
                if trade_no == "rsc_platform_undelegate_needs_ack"
        )));
    }

    #[tokio::test]
    async fn scanner_finds_platform_undelegation_recover_for_collect_and_withdraw() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_root = dir.path().to_string_lossy().to_string();
        let pool = SqliteContext::new(&db_root, Some("api_transaction.db"))
            .await
            .expect("init api_transaction.db")
            .into_transaction_db_pool()
            .expect("transaction pool");

        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate_task(
                "uid",
                "rsc_platform_undelegate_collect_recover",
                ApiTradeType::Collect,
                ApiResourceDelegationOperationType::Undelegate,
                "tron",
                "owner",
                "receiver",
                ApiResourceType::Energy,
                "5",
                "1000",
            ),
        )
        .await
        .expect("insert collect platform undelegate");
        ApiResourceDelegationRepo::mark_task_ack_sent(
            &pool,
            "rsc_platform_undelegate_collect_recover",
        )
        .await
        .expect("mark task ack");
        ApiResourceDelegationRepo::claim_build_slot(
            &pool,
            "rsc_platform_undelegate_collect_recover",
        )
        .await
        .expect("claim build slot");
        ApiResourceDelegationRepo::mark_broadcast_success(
            &pool,
            "rsc_platform_undelegate_collect_recover",
            "tx_hash_collect",
        )
        .await
        .expect("mark broadcast success");

        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate_task(
                "uid",
                "rsc_platform_undelegate_withdraw_recover",
                ApiTradeType::Withdraw,
                ApiResourceDelegationOperationType::Undelegate,
                "tron",
                "owner",
                "receiver",
                ApiResourceType::Energy,
                "5",
                "1000",
            ),
        )
        .await
        .expect("insert withdraw platform undelegate");
        ApiResourceDelegationRepo::mark_task_ack_sent(
            &pool,
            "rsc_platform_undelegate_withdraw_recover",
        )
        .await
        .expect("mark task ack");
        ApiResourceDelegationRepo::claim_build_slot(
            &pool,
            "rsc_platform_undelegate_withdraw_recover",
        )
        .await
        .expect("claim build slot");
        ApiResourceDelegationRepo::mark_broadcast_success(
            &pool,
            "rsc_platform_undelegate_withdraw_recover",
            "tx_hash_withdraw",
        )
        .await
        .expect("mark broadcast success");

        let scanner = PlatformResourceReclaimScanner::with_config(
            pool,
            PlatformResourceReclaimScannerConfig {
                scan_interval: Duration::from_secs(60),
                max_items_per_scan: 8,
            },
        );

        let intents = scanner.scan_round().await;
        assert!(intents.iter().any(|intent| matches!(
            intent,
            PlatformResourceReclaimIntent::RecoverPlatformUndelegation(trade_no)
                if trade_no == "rsc_platform_undelegate_collect_recover"
        )));
        assert!(intents.iter().any(|intent| matches!(
            intent,
            PlatformResourceReclaimIntent::RecoverPlatformUndelegation(trade_no)
                if trade_no == "rsc_platform_undelegate_withdraw_recover"
        )));
    }
}
