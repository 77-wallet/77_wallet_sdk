use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use chrono::Utc;
use tokio::sync::{Mutex, Semaphore, broadcast, mpsc};
use tracing::{error, info, warn};
use wallet_database::{
    ApiResourceDbPool, ApiTransactionDbPool,
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
use wallet_transport_backend::request::api_wallet::transaction::{
    TransAckType, TransEventAckReq, TransStatus, TxExecReceiptUploadReq,
};
use wallet_utils::RetryableError as _;

use crate::{
    context::CONTEXT,
    domain::{api_wallet::trans::ApiTransDomain, chain::adapter::ChainAdapterFactory},
    error::service::ServiceError,
    infrastructure::{
        api_trans::{
            resource_ack_type::{
                is_original_order_resource_result_fact, merchant_original_resource_result_ack_type,
                merchant_original_resource_result_trans_type, platform_resource_result_ack_type,
                platform_resource_task_trans_type,
            },
            resource_amount::parse_resource_delegation_native_trx_units,
            resource_chain_facts::{
                resource_chain_failed, resource_chain_success, resource_failure_receipt_uploadable,
                resource_success_receipt_uploadable,
            },
            resource_delegate::resource_gate_projection,
            resource_delegation::{
                execute_resource_delegation, mark_recovered_resource_delegation_failure,
                record_resource_uncertain_and_maybe_rebroadcast, resource_delegation_failure_fact,
                resource_delegation_is_built_unbroadcast, resource_delegation_retry_wait_secs,
                resource_recovery_error_is_uncertain,
            },
            shadow_rpc_policy,
        },
        runtime::time::new_production_interval,
    },
};
use wallet_chain_interact::tron::operations::stake::ResourceType as TronStakeResourceType;

const PLATFORM_RESOURCE_DELEGATE_DEFAULT_CONCURRENCY: usize = 4;

type PlatformCapacityReservations = HashMap<String, i64>;

static PLATFORM_DELEGATION_CAPACITY_RESERVATIONS: OnceLock<Mutex<PlatformCapacityReservations>> =
    OnceLock::new();

fn platform_capacity_reservations() -> &'static Mutex<PlatformCapacityReservations> {
    PLATFORM_DELEGATION_CAPACITY_RESERVATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn platform_capacity_key(owner_address: &str, resource_type: ApiResourceType) -> String {
    format!("{}:{}", owner_address, resource_type.as_i64())
}

fn can_reserve_platform_delegation_capacity(
    available_native_amount: i64,
    reserved_native_amount: i64,
    required_native_amount: i64,
) -> bool {
    required_native_amount > 0
        && available_native_amount.saturating_sub(reserved_native_amount) >= required_native_amount
}

fn can_project_platform_terminal_failure(
    affected: u64,
    result_status: Option<ApiResourceDelegationResultStatus>,
    chain_failed: bool,
    finished: bool,
) -> bool {
    affected > 0
        || (result_status == Some(ApiResourceDelegationResultStatus::Fail)
            && chain_failed
            && finished)
}

fn reserve_platform_delegation_capacity(
    reservations: &mut PlatformCapacityReservations,
    key: &str,
    available_native_amount: i64,
    required_native_amount: i64,
) -> bool {
    let reserved_native_amount = reservations.get(key).copied().unwrap_or_default();
    if !can_reserve_platform_delegation_capacity(
        available_native_amount,
        reserved_native_amount,
        required_native_amount,
    ) {
        return false;
    }

    reservations.insert(key.to_string(), reserved_native_amount + required_native_amount);
    true
}

fn release_platform_delegation_capacity_reservation(key: &str, reserved_native_amount: i64) {
    let key = key.to_string();
    tokio::spawn(async move {
        let mut reservations = platform_capacity_reservations().lock().await;
        release_platform_delegation_capacity(&mut reservations, &key, reserved_native_amount);
    });
}

fn release_platform_delegation_capacity(
    reservations: &mut PlatformCapacityReservations,
    key: &str,
    reserved_native_amount: i64,
) {
    let Some(current) = reservations.get_mut(key) else {
        return;
    };
    *current = current.saturating_sub(reserved_native_amount);
    if *current == 0 {
        reservations.remove(key);
    }
}

#[derive(Debug)]
struct PlatformDelegationCapacityReservation {
    key: String,
    native_amount: i64,
    released: bool,
}

impl PlatformDelegationCapacityReservation {
    async fn release(mut self) {
        let mut reservations = platform_capacity_reservations().lock().await;
        release_platform_delegation_capacity(&mut reservations, &self.key, self.native_amount);
        self.released = true;
    }
}

impl Drop for PlatformDelegationCapacityReservation {
    fn drop(&mut self) {
        if !self.released {
            release_platform_delegation_capacity_reservation(&self.key, self.native_amount);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformResourceDelegateIntent {
    SendPlatformDelegationTaskAck(String),
    ExecutePlatformDelegation(String),
    RecoverPlatformDelegation(String),
    UploadPlatformDelegationTxExecReceipt(String),
    SendPlatformDelegationResultAck(String),
}

impl PlatformResourceDelegateIntent {
    fn next_after_success(&self) -> Option<Self> {
        match self {
            Self::SendPlatformDelegationTaskAck(trade_no) => {
                Some(Self::ExecutePlatformDelegation(trade_no.clone()))
            }
            Self::ExecutePlatformDelegation(trade_no) => {
                Some(Self::RecoverPlatformDelegation(trade_no.clone()))
            }
            Self::RecoverPlatformDelegation(trade_no) => {
                Some(Self::UploadPlatformDelegationTxExecReceipt(trade_no.clone()))
            }
            Self::UploadPlatformDelegationTxExecReceipt(trade_no) => {
                Some(Self::SendPlatformDelegationResultAck(trade_no.clone()))
            }
            Self::SendPlatformDelegationResultAck(_) => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlatformResourceDelegateScannerConfig {
    pub scan_interval: Duration,
    pub max_items_per_scan: usize,
}

impl Default for PlatformResourceDelegateScannerConfig {
    fn default() -> Self {
        let scan_interval_secs = shadow_rpc_policy::read_u64_env(
            "PLATFORM_RESOURCE_DELEGATE_SHADOW_SCAN_INTERVAL_SECS",
            10,
            10,
            120,
        );
        let max_items_per_scan = shadow_rpc_policy::read_u64_env(
            "PLATFORM_RESOURCE_DELEGATE_SHADOW_MAX_ITEMS_PER_SCAN",
            20,
            1,
            200,
        ) as usize;

        Self { scan_interval: Duration::from_secs(scan_interval_secs), max_items_per_scan }
    }
}

#[derive(Debug, Clone)]
pub struct PlatformResourceDelegateScanner {
    pool: ApiResourceDbPool,
    config: PlatformResourceDelegateScannerConfig,
}

impl PlatformResourceDelegateScanner {
    pub fn new(pool: ApiResourceDbPool) -> Self {
        Self::with_config(pool, PlatformResourceDelegateScannerConfig::default())
    }

    pub fn with_config(
        pool: ApiResourceDbPool,
        config: PlatformResourceDelegateScannerConfig,
    ) -> Self {
        Self { pool, config }
    }

    pub async fn scan_round(&self) -> Vec<PlatformResourceDelegateIntent> {
        let mut intents = Vec::new();
        self.scan_task_ack(ApiTradeType::Collect, &mut intents).await;
        self.scan_task_ack(ApiTradeType::Withdraw, &mut intents).await;
        self.scan_execute(ApiTradeType::Collect, &mut intents).await;
        self.scan_execute(ApiTradeType::Withdraw, &mut intents).await;
        self.scan_recover(ApiTradeType::Collect, &mut intents).await;
        self.scan_recover(ApiTradeType::Withdraw, &mut intents).await;
        self.scan_receipt_upload(&mut intents).await;
        self.scan_result_ack(&mut intents).await;
        intents
    }

    async fn scan_task_ack(
        &self,
        origin_type: ApiTradeType,
        intents: &mut Vec<PlatformResourceDelegateIntent>,
    ) {
        match ApiResourceDelegationRepo::scan_need_task_ack_for_origin_type_source_and_operation(
            &self.pool,
            origin_type as i64,
            ApiResourceDelegationSource::Platform,
            ApiResourceDelegationOperationType::Delegate,
            self.config.max_items_per_scan,
        )
        .await
        {
            Ok(records) => {
                intents.extend(records.into_iter().map(|record| {
                    PlatformResourceDelegateIntent::SendPlatformDelegationTaskAck(
                        record.resource_trade_no,
                    )
                }));
            }
            Err(e) => {
                error!(error = %e, ?origin_type, "Failed to scan platform delegate ACK records")
            }
        }
    }

    async fn scan_execute(
        &self,
        origin_type: ApiTradeType,
        intents: &mut Vec<PlatformResourceDelegateIntent>,
    ) {
        match ApiResourceDelegationRepo::scan_can_execute_for_origin_type_source_and_operation(
            &self.pool,
            origin_type as i64,
            ApiResourceDelegationSource::Platform,
            ApiResourceDelegationOperationType::Delegate,
            self.config.max_items_per_scan,
        )
        .await
        {
            Ok(records) => {
                intents.extend(records.into_iter().map(|record| {
                    PlatformResourceDelegateIntent::ExecutePlatformDelegation(
                        record.resource_trade_no,
                    )
                }));
            }
            Err(e) => {
                error!(error = %e, ?origin_type, "Failed to scan executable platform delegate records")
            }
        }
    }

    async fn scan_recover(
        &self,
        origin_type: ApiTradeType,
        intents: &mut Vec<PlatformResourceDelegateIntent>,
    ) {
        match ApiResourceDelegationRepo::scan_can_recover_by_origin_type_source_and_operation(
            &self.pool,
            origin_type as i64,
            ApiResourceDelegationSource::Platform,
            ApiResourceDelegationOperationType::Delegate,
            self.config.max_items_per_scan,
        )
        .await
        {
            Ok(records) => {
                intents.extend(records.into_iter().map(|record| {
                    PlatformResourceDelegateIntent::RecoverPlatformDelegation(
                        record.resource_trade_no,
                    )
                }));
            }
            Err(e) => {
                error!(error = %e, ?origin_type, "Failed to scan recoverable platform delegate records")
            }
        }
    }

    async fn scan_receipt_upload(&self, intents: &mut Vec<PlatformResourceDelegateIntent>) {
        match ApiResourceDelegationRepo::scan_need_tx_exec_receipt_upload_for_source_and_operation(
            &self.pool,
            ApiResourceDelegationSource::Platform,
            ApiResourceDelegationOperationType::Delegate,
            self.config.max_items_per_scan,
        )
        .await
        {
            Ok(records) => {
                intents.extend(records.into_iter().map(|record| {
                    PlatformResourceDelegateIntent::UploadPlatformDelegationTxExecReceipt(
                        record.resource_trade_no,
                    )
                }));
            }
            Err(e) => error!(error = %e, "Failed to scan platform delegate receipt records"),
        }
    }

    async fn scan_result_ack(&self, intents: &mut Vec<PlatformResourceDelegateIntent>) {
        match ApiResourceDelegationRepo::scan_need_result_ack_for_source_and_operation(
            &self.pool,
            ApiResourceDelegationSource::Platform,
            ApiResourceDelegationOperationType::Delegate,
            self.config.max_items_per_scan,
        )
        .await
        {
            Ok(records) => {
                intents.extend(records.into_iter().map(|record| {
                    PlatformResourceDelegateIntent::SendPlatformDelegationResultAck(
                        record.resource_trade_no,
                    )
                }));
            }
            Err(e) => error!(error = %e, "Failed to scan platform delegate result ACK records"),
        }
    }
}

pub struct PlatformResourceDelegateScannerActor {
    scanner: Arc<PlatformResourceDelegateScanner>,
    intent_tx: mpsc::Sender<PlatformResourceDelegateIntent>,
    shutdown_rx: broadcast::Receiver<()>,
}

impl PlatformResourceDelegateScannerActor {
    pub fn new(
        scanner: Arc<PlatformResourceDelegateScanner>,
        intent_tx: mpsc::Sender<PlatformResourceDelegateIntent>,
        shutdown_rx: broadcast::Receiver<()>,
    ) -> Self {
        Self { scanner, intent_tx, shutdown_rx }
    }

    pub async fn run(mut self) {
        crate::infrastructure::system_ready::wait_system_ready().await;
        info!("Platform resource delegate shadow scanner actor running");
        let mut interval = new_production_interval(self.scanner.config.scan_interval);
        loop {
            tokio::select! {
                _ = self.shutdown_rx.recv() => break,
                _ = interval.tick() => {
                    for intent in self.scanner.scan_round().await {
                        if let Err(e) = self.intent_tx.send(intent).await {
                            error!(error = %e, "Failed to enqueue platform resource delegate intent");
                            break;
                        }
                    }
                }
            }
        }
        info!("Platform resource delegate shadow scanner actor stopped");
    }
}

#[derive(Debug, Clone)]
pub struct PlatformResourceDelegateWorker {
    pool: ApiResourceDbPool,
    transaction_pool: ApiTransactionDbPool,
}

impl PlatformResourceDelegateWorker {
    pub fn new(pool: ApiResourceDbPool, transaction_pool: ApiTransactionDbPool) -> Self {
        Self { pool, transaction_pool }
    }

    pub async fn handle(
        &self,
        intent: PlatformResourceDelegateIntent,
    ) -> Result<Option<PlatformResourceDelegateIntent>, ServiceError> {
        match intent {
            PlatformResourceDelegateIntent::SendPlatformDelegationTaskAck(trade_no) => {
                self.process_task_ack(trade_no.clone()).await?;
                Ok(PlatformResourceDelegateIntent::SendPlatformDelegationTaskAck(trade_no)
                    .next_after_success())
            }
            PlatformResourceDelegateIntent::ExecutePlatformDelegation(trade_no) => {
                let result = self.process_execute(trade_no.clone()).await;
                self.handle_execute_failure_if_needed(&trade_no, result).await?;
                Ok(PlatformResourceDelegateIntent::ExecutePlatformDelegation(trade_no)
                    .next_after_success())
            }
            PlatformResourceDelegateIntent::RecoverPlatformDelegation(trade_no) => {
                if self.process_recover(trade_no.clone()).await? {
                    Ok(PlatformResourceDelegateIntent::RecoverPlatformDelegation(trade_no)
                        .next_after_success())
                } else {
                    Ok(None)
                }
            }
            PlatformResourceDelegateIntent::UploadPlatformDelegationTxExecReceipt(trade_no) => {
                self.process_receipt_upload(trade_no.clone()).await?;
                Ok(PlatformResourceDelegateIntent::UploadPlatformDelegationTxExecReceipt(trade_no)
                    .next_after_success())
            }
            PlatformResourceDelegateIntent::SendPlatformDelegationResultAck(trade_no) => {
                self.process_result_ack(trade_no).await?;
                Ok(None)
            }
        }
    }

    async fn process_task_ack(&self, resource_trade_no: String) -> Result<(), ServiceError> {
        let task = self.load_platform_delegate(&resource_trade_no).await?;
        if task.task_ack_sent_at.is_some() {
            return Ok(());
        }
        let attempted =
            ApiResourceDelegationRepo::mark_task_ack_attempted(&self.pool, &resource_trade_no)
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;
        if attempted == 0 {
            return Ok(());
        }
        CONTEXT
            .get()
            .unwrap()
            .get_global_backend_api()
            .trans_event_ack(&TransEventAckReq::new(
                &resource_trade_no,
                platform_resource_task_trans_type(&task),
                TransAckType::Tx,
            ))
            .await?;
        ApiResourceDelegationRepo::mark_task_ack_sent(&self.pool, &resource_trade_no)
            .await
            .map_err(|e| ServiceError::Database(e.into()))?;
        Ok(())
    }

    async fn process_execute(&self, resource_trade_no: String) -> Result<(), ServiceError> {
        let stage_start = Instant::now();
        info!(
            observability_event = "platform_resource_delegate_execute_start",
            resource_trade_no = %resource_trade_no,
            source = "platform_resource_delegate",
            "Platform resource delegation execute stage started"
        );

        let claim_start = Instant::now();
        let existing = self.load_platform_delegate(&resource_trade_no).await?;
        let built_unbroadcast = resource_delegation_is_built_unbroadcast(&existing);
        if existing.last_broadcast_at.is_some() {
            return Ok(());
        }
        let affected = if built_unbroadcast {
            1
        } else {
            ApiResourceDelegationRepo::claim_build_slot(&self.pool, &resource_trade_no)
                .await
                .map_err(|e| ServiceError::Database(e.into()))?
        };
        let claim_elapsed_ms = claim_start.elapsed().as_millis();
        if affected == 0 {
            info!(
                observability_event = "platform_resource_delegate_execute_claim_skipped",
                resource_trade_no = %resource_trade_no,
                claim_elapsed_ms = %claim_elapsed_ms,
                total_elapsed_ms = %stage_start.elapsed().as_millis(),
                source = "platform_resource_delegate",
                "Platform resource delegation execute skipped because build slot was not claimed"
            );
            return Ok(());
        }

        let load_start = Instant::now();
        let task = self.load_platform_delegate(&resource_trade_no).await?;
        let load_elapsed_ms = load_start.elapsed().as_millis();
        let ack_to_execute_load_ms = task
            .task_ack_sent_at
            .map(|time| Utc::now().signed_duration_since(time).num_milliseconds())
            .filter(|elapsed| *elapsed >= 0);

        info!(
            observability_event = "platform_resource_delegate_execute_task_loaded",
            resource_trade_no = %resource_trade_no,
            created_at = ?task.created_at,
            task_ack_sent_at = ?task.task_ack_sent_at,
            ack_to_execute_load_ms = ?ack_to_execute_load_ms,
            claim_elapsed_ms = %claim_elapsed_ms,
            load_elapsed_ms = %load_elapsed_ms,
            source = "platform_resource_delegate",
            "Platform resource delegation execute task loaded"
        );

        let capacity_start = Instant::now();
        let capacity_reservation = self.reserve_delegate_capacity_if_needed(&task).await?;
        let capacity_elapsed_ms = capacity_start.elapsed().as_millis();

        let execute_start = Instant::now();
        let tx_hash = execute_resource_delegation(
            crate::context::get_context()?,
            &self.pool,
            &task,
            "platform_resource_delegate",
        )
        .await?;
        let execute_elapsed_ms = execute_start.elapsed().as_millis();
        if let Some(reservation) = capacity_reservation {
            reservation.release().await;
        }

        info!(
            observability_event = "platform_resource_delegate_execute_complete",
            resource_trade_no = %resource_trade_no,
            tx_hash = %tx_hash,
            claim_elapsed_ms = %claim_elapsed_ms,
            load_elapsed_ms = %load_elapsed_ms,
            capacity_elapsed_ms = %capacity_elapsed_ms,
            execute_elapsed_ms = %execute_elapsed_ms,
            total_elapsed_ms = %stage_start.elapsed().as_millis(),
            source = "platform_resource_delegate",
            "Platform resource delegation execute stage completed"
        );
        Ok(())
    }

    async fn reserve_delegate_capacity_if_needed(
        &self,
        task: &ApiResourceDelegationEntity,
    ) -> Result<Option<PlatformDelegationCapacityReservation>, ServiceError> {
        if task.operation_type != ApiResourceDelegationOperationType::Delegate {
            return Ok(None);
        }

        let required_native_amount =
            parse_resource_delegation_native_trx_units(&task.native_amount)?;
        let tron_resource_type = match task.resource_type {
            ApiResourceType::Bandwidth => TronStakeResourceType::BANDWIDTH,
            ApiResourceType::Energy => TronStakeResourceType::ENERGY,
        };
        let key = platform_capacity_key(&task.owner_address, task.resource_type);

        let capacity_check_start = Instant::now();
        info!(
            observability_event = "platform_resource_delegate_capacity_check_start",
            resource_trade_no = %task.resource_trade_no,
            owner_address = %task.owner_address,
            receiver_address = %task.receiver_address,
            resource_type = ?task.resource_type,
            required_native_amount = %required_native_amount,
            source = "platform_resource_delegate",
            "Platform resource delegation capacity check started"
        );

        let chain = ChainAdapterFactory::get_tron_adapter(crate::context::get_context()?).await?;
        let available_native_amount = chain
            .get_provider()
            .can_delegate_resource(&task.owner_address, tron_resource_type)
            .await?
            .to_sun();
        let chain_capacity_elapsed_ms = capacity_check_start.elapsed().as_millis();

        let reservation_lock_start = Instant::now();
        let mut reservations = platform_capacity_reservations().lock().await;
        let reservation_lock_elapsed_ms = reservation_lock_start.elapsed().as_millis();
        let reserved_native_amount = reservations.get(&key).copied().unwrap_or_default();

        if !reserve_platform_delegation_capacity(
            &mut reservations,
            &key,
            available_native_amount,
            required_native_amount,
        ) {
            warn!(
                observability_event = "platform_resource_delegate_capacity_insufficient",
                resource_trade_no = %task.resource_trade_no,
                owner_address = %task.owner_address,
                receiver_address = %task.receiver_address,
                resource_type = ?task.resource_type,
                required_native_amount = %required_native_amount,
                available_native_amount = %available_native_amount,
                reserved_native_amount = %reserved_native_amount,
                chain_capacity_elapsed_ms = %chain_capacity_elapsed_ms,
                reservation_lock_elapsed_ms = %reservation_lock_elapsed_ms,
                total_capacity_elapsed_ms = %capacity_check_start.elapsed().as_millis(),
                source = "platform_resource_delegate",
                "Platform resource delegation capacity is insufficient before broadcast"
            );
            return Err(ServiceError::Parameter(format!(
                "platform resource delegation capacity insufficient: required={}, available={}, reserved={}",
                required_native_amount, available_native_amount, reserved_native_amount
            )));
        }

        info!(
            observability_event = "platform_resource_delegate_capacity_reserved",
            resource_trade_no = %task.resource_trade_no,
            owner_address = %task.owner_address,
            receiver_address = %task.receiver_address,
            resource_type = ?task.resource_type,
            required_native_amount = %required_native_amount,
            available_native_amount = %available_native_amount,
            reserved_native_amount_before = %reserved_native_amount,
            reserved_native_amount_after = %(reserved_native_amount + required_native_amount),
            chain_capacity_elapsed_ms = %chain_capacity_elapsed_ms,
            reservation_lock_elapsed_ms = %reservation_lock_elapsed_ms,
            total_capacity_elapsed_ms = %capacity_check_start.elapsed().as_millis(),
            source = "platform_resource_delegate",
            "Platform resource delegation capacity reserved"
        );

        Ok(Some(PlatformDelegationCapacityReservation {
            key,
            native_amount: required_native_amount,
            released: false,
        }))
    }

    async fn process_recover(&self, resource_trade_no: String) -> Result<bool, ServiceError> {
        let task = self.load_platform_delegate(&resource_trade_no).await?;
        if task.finished_at.is_some() || task.err_code.is_some() {
            return Ok(resource_chain_success(&task) || resource_chain_failed(&task));
        }
        if resource_delegation_is_built_unbroadcast(&task) {
            self.process_execute(resource_trade_no).await?;
            return Ok(false);
        }
        let Some(tx_hash) = task.tx_hash.as_deref().filter(|s| !s.trim().is_empty()) else {
            return Ok(false);
        };
        match ApiTransDomain::process_recovered_tx(
            crate::context::get_context()?,
            &task.chain_code,
            &task.owner_address,
            tx_hash,
            0,
            "0",
        )
        .await
        {
            Ok(Some(resp)) => {
                let payload = format!("platform_delegation_recovered:{}", resp.tx_hash);
                let transaction_time = resp
                    .transaction_time_ms
                    .and_then(|millis| {
                        chrono::DateTime::<chrono::Utc>::from_timestamp_millis(millis as i64)
                    })
                    .map(|time| time.to_rfc3339());
                ApiResourceDelegationRepo::mark_recovered_chain_success(
                    &self.pool,
                    &resource_trade_no,
                    &resp.tx_hash,
                    transaction_time.as_deref(),
                    &payload,
                )
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;
                Ok(true)
            }
            Ok(None) => {
                let rebroadcast =
                    record_resource_uncertain_and_maybe_rebroadcast(&self.pool, &resource_trade_no)
                        .await?;
                if rebroadcast {
                    return Ok(false);
                }
                Ok(false)
            }
            Err(err) if resource_recovery_error_is_uncertain(&err) => {
                record_resource_uncertain_and_maybe_rebroadcast(&self.pool, &resource_trade_no)
                    .await?;
                Ok(false)
            }
            Err(err) => {
                mark_recovered_resource_delegation_failure(
                    &self.pool,
                    &task,
                    tx_hash,
                    &err,
                    "platform_delegation_recovered_failure",
                )
                .await?;
                Ok(true)
            }
        }
    }

    async fn process_receipt_upload(&self, resource_trade_no: String) -> Result<(), ServiceError> {
        let stage_start = Instant::now();
        let task = self.load_platform_delegate(&resource_trade_no).await?;
        if task.tx_exec_receipt_uploaded_at.is_some() {
            info!(
                observability_event = "platform_resource_delegate_receipt_upload_skipped",
                resource_trade_no = %resource_trade_no,
                elapsed_ms = %stage_start.elapsed().as_millis(),
                source = "platform_resource_delegate",
                "Platform resource delegation receipt upload skipped because it is already uploaded"
            );
            return Ok(());
        }
        let payload = Self::build_receipt_payload(&task)?;
        info!(
            observability_event = "platform_resource_delegate_receipt_upload_start",
            resource_trade_no = %resource_trade_no,
            tx_hash = ?task.tx_hash,
            result_status = ?task.result_status,
            err_code = ?task.err_code,
            chain_success_at = ?task.chain_success_at,
            chain_failed_at = ?task.chain_failed_at,
            finished_at = ?task.finished_at,
            source = "platform_resource_delegate",
            "Platform resource delegation receipt upload started"
        );
        let upload_start = Instant::now();
        CONTEXT.get().unwrap().get_global_backend_api().upload_tx_exec_receipt(&payload).await?;
        let upload_elapsed_ms = upload_start.elapsed().as_millis();
        let mark_start = Instant::now();
        let marked =
            ApiResourceDelegationRepo::mark_tx_exec_receipt_uploaded_for_source_and_operation(
                &self.pool,
                &resource_trade_no,
                ApiResourceDelegationSource::Platform,
                ApiResourceDelegationOperationType::Delegate,
            )
            .await
            .map_err(|e| ServiceError::Database(e.into()))?;
        if marked == 0 {
            let current = self.load_platform_delegate(&resource_trade_no).await?;
            if current.tx_exec_receipt_uploaded_at.is_none() {
                return Err(ServiceError::Database(wallet_database::Error::Database(
                    wallet_database::DatabaseError::UpdateFailed(format!(
                        "resource delegation receipt upload fact conflict: trade_no={resource_trade_no}"
                    )),
                )));
            }
        }
        let mark_elapsed_ms = mark_start.elapsed().as_millis();

        info!(
            observability_event = "platform_resource_delegate_receipt_upload_complete",
            resource_trade_no = %resource_trade_no,
            tx_hash = ?task.tx_hash,
            result_status = ?task.result_status,
            err_code = ?task.err_code,
            upload_elapsed_ms = %upload_elapsed_ms,
            mark_elapsed_ms = %mark_elapsed_ms,
            total_elapsed_ms = %stage_start.elapsed().as_millis(),
            source = "platform_resource_delegate",
            "Platform resource delegation receipt upload completed"
        );

        // 失败路径可能没有 result message，所以在 receipt upload 后 bypass release。
        // 这是失败例外路径；成功路径仍必须等 result ACK 后才释放原单 gate。
        resource_gate_projection::project_gate_if_failure(
            &self.transaction_pool,
            &task,
            "platform_resource_delegate",
        )
        .await
    }

    async fn process_result_ack(&self, resource_trade_no: String) -> Result<(), ServiceError> {
        let task = self.load_platform_delegate(&resource_trade_no).await?;
        if task.result_ack_sent_at.is_some() {
            return Ok(());
        }
        if task.finished_at.is_none() || task.result_payload.is_none() {
            return Ok(());
        }
        let (trans_type, ack_type) = if is_original_order_resource_result_fact(&task) {
            (
                merchant_original_resource_result_trans_type(&task),
                merchant_original_resource_result_ack_type(),
            )
        } else {
            (platform_resource_task_trans_type(&task), platform_resource_result_ack_type())
        };
        let attempted =
            ApiResourceDelegationRepo::mark_result_ack_attempted(&self.pool, &resource_trade_no)
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;
        if attempted == 0 {
            return Ok(());
        }
        CONTEXT
            .get()
            .unwrap()
            .get_global_backend_api()
            .trans_event_ack(&TransEventAckReq::new(&resource_trade_no, trans_type, ack_type))
            .await?;

        // ACK 成功后才能 release 原单 gate。
        // 这样即使 collect/withdraw shadow 独立扫描，也不会在资源结果 ACK 前推进原单。
        resource_gate_projection::mark_result_ack_and_project_gate(
            &self.pool,
            &self.transaction_pool,
            &task,
            "platform_resource_delegate",
        )
        .await
    }

    async fn handle_execute_failure_if_needed(
        &self,
        resource_trade_no: &str,
        result: Result<(), ServiceError>,
    ) -> Result<(), ServiceError> {
        let Err(err) = result else {
            return Ok(());
        };
        if err.is_transient_db_pool_timeout() {
            return self.schedule_rebuild(resource_trade_no).await;
        }
        match err.retry_policy() {
            wallet_utils::RetryPolicy::Never => {
                let (err_code, err_msg) = resource_delegation_failure_fact(&err);
                let payload = format!("platform_delegation_terminal_failure:{err_msg}");
                let affected = ApiResourceDelegationRepo::mark_failed_with_result_if_unfinished(
                    &self.pool,
                    resource_trade_no,
                    None,
                    &err_code,
                    &err_msg,
                    &payload,
                )
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;
                let task = self.load_platform_delegate(resource_trade_no).await?;
                if !can_project_platform_terminal_failure(
                    affected,
                    task.result_status,
                    task.chain_failed_at.is_some(),
                    task.finished_at.is_some(),
                ) {
                    info!(
                        resource_trade_no = %resource_trade_no,
                        source = "platform_resource_delegate",
                        "Skip platform delegation failure projection because the task is no longer eligible"
                    );
                    return Ok(());
                }
                resource_gate_projection::project_gate_if_failure(
                    &self.transaction_pool,
                    &task,
                    "platform_resource_delegate",
                )
                .await
            }
            wallet_utils::RetryPolicy::Delay => self.schedule_rebuild(resource_trade_no).await,
        }
    }

    async fn schedule_rebuild(&self, resource_trade_no: &str) -> Result<(), ServiceError> {
        ApiResourceDelegationRepo::reset_for_rebuild(&self.pool, resource_trade_no)
            .await
            .map_err(|e| ServiceError::Database(e.into()))?;
        Ok(())
    }

    fn build_receipt_payload(
        task: &ApiResourceDelegationEntity,
    ) -> Result<TxExecReceiptUploadReq, ServiceError> {
        let status = if resource_success_receipt_uploadable(task) {
            TransStatus::Success
        } else if resource_failure_receipt_uploadable(task) {
            TransStatus::Fail
        } else {
            return Err(ServiceError::Parameter(
                "resource delegation receipt upload requires confirmed success or failure chain facts"
                    .to_string(),
            ));
        };
        let remark = if matches!(status, TransStatus::Success) {
            ""
        } else {
            task.err_msg.as_deref().unwrap_or("")
        };
        let mut payload = TxExecReceiptUploadReq::new(
            Some(&task.owner_address),
            Some(&task.receiver_address),
            &task.resource_trade_no,
            platform_resource_task_trans_type(task),
            task.tx_hash.as_deref(),
            status,
            remark,
        );
        if let Some(err_code) = task.err_code.as_deref().filter(|s| !s.trim().is_empty()) {
            payload = payload.with_error_code(err_code);
        }
        Ok(payload)
    }

    async fn load_platform_delegate(
        &self,
        resource_trade_no: &str,
    ) -> Result<ApiResourceDelegationEntity, ServiceError> {
        let task =
            ApiResourceDelegationRepo::get_by_resource_trade_no(&self.pool, resource_trade_no)
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;
        if task.source == ApiResourceDelegationSource::Platform
            && task.operation_type == ApiResourceDelegationOperationType::Delegate
        {
            Ok(task)
        } else {
            Err(ServiceError::Parameter(format!(
                "platform resource delegate requires source=Platform + Delegate, got source={:?} operation={:?}",
                task.source, task.operation_type
            )))
        }
    }
}

pub struct PlatformResourceDelegateDispatcherActor {
    worker: PlatformResourceDelegateWorker,
    intent_tx: mpsc::Sender<PlatformResourceDelegateIntent>,
    intent_rx: mpsc::Receiver<PlatformResourceDelegateIntent>,
    shutdown_rx: broadcast::Receiver<()>,
    semaphore: Arc<Semaphore>,
}

impl PlatformResourceDelegateDispatcherActor {
    pub fn new(
        worker: PlatformResourceDelegateWorker,
        intent_tx: mpsc::Sender<PlatformResourceDelegateIntent>,
        intent_rx: mpsc::Receiver<PlatformResourceDelegateIntent>,
        shutdown_rx: broadcast::Receiver<()>,
    ) -> Self {
        let concurrency = shadow_rpc_policy::read_u64_env(
            "PLATFORM_RESOURCE_DELEGATE_SHADOW_CONCURRENCY",
            PLATFORM_RESOURCE_DELEGATE_DEFAULT_CONCURRENCY as u64,
            1,
            32,
        ) as usize;
        Self {
            worker,
            intent_tx,
            intent_rx,
            shutdown_rx,
            semaphore: Arc::new(Semaphore::new(concurrency)),
        }
    }

    pub async fn run(mut self) {
        crate::infrastructure::system_ready::wait_system_ready().await;
        info!(
            concurrency = self.semaphore.available_permits(),
            "Platform resource delegate shadow dispatcher actor running"
        );
        loop {
            tokio::select! {
                _ = self.shutdown_rx.recv() => break,
                Some(intent) = self.intent_rx.recv() => {
                    let permit_wait_start = Instant::now();
                    let intent_debug = format!("{intent:?}");
                    let intent_trade_no = match &intent {
                        PlatformResourceDelegateIntent::SendPlatformDelegationTaskAck(trade_no)
                        | PlatformResourceDelegateIntent::ExecutePlatformDelegation(trade_no)
                        | PlatformResourceDelegateIntent::RecoverPlatformDelegation(trade_no)
                        | PlatformResourceDelegateIntent::UploadPlatformDelegationTxExecReceipt(
                            trade_no,
                        )
                        | PlatformResourceDelegateIntent::SendPlatformDelegationResultAck(
                            trade_no,
                        ) => trade_no.clone(),
                    };
                    let Ok(permit) = self.semaphore.clone().acquire_owned().await else {
                        error!("Platform resource delegate dispatcher semaphore closed");
                        break;
                    };
                    let permit_wait_ms = permit_wait_start.elapsed().as_millis();
                    info!(
                        observability_event = "platform_resource_delegate_dispatch_permit_acquired",
                        resource_trade_no = %intent_trade_no,
                        intent = %intent_debug,
                        permit_wait_ms = %permit_wait_ms,
                        available_permits_after_acquire = self.semaphore.available_permits(),
                        source = "platform_resource_delegate",
                        "Platform resource delegate dispatcher acquired permit"
                    );
                    let worker = self.worker.clone();
                    tokio::spawn(async move {
                        let _permit = permit;
                        let mut current = Some(intent);
                        while let Some(intent) = current {
                            let intent_start = Instant::now();
                            let intent_debug = format!("{intent:?}");
                            match worker.handle(intent).await {
                                Ok(next_intent) => {
                                    info!(
                                        observability_event = "platform_resource_delegate_intent_complete",
                                        intent = %intent_debug,
                                        elapsed_ms = %intent_start.elapsed().as_millis(),
                                        has_next_intent = next_intent.is_some(),
                                        source = "platform_resource_delegate",
                                        "Platform resource delegate intent completed"
                                    );
                                    current = next_intent;
                                }
                                Err(e) => {
                                    error!(
                                        observability_event = "platform_resource_delegate_intent_failed",
                                        intent = %intent_debug,
                                        elapsed_ms = %intent_start.elapsed().as_millis(),
                                        error = %e,
                                        source = "platform_resource_delegate",
                                        "Failed to handle platform resource delegate intent"
                                    );
                                    break;
                                }
                            }
                        };
                    });
                }
                else => break,
            }
        }
        info!("Platform resource delegate shadow dispatcher actor stopped");
    }
}

#[derive(Debug)]
pub struct PlatformResourceDelegateShadowActorSystem {
    shutdown_tx: broadcast::Sender<()>,
    intent_tx: mpsc::Sender<PlatformResourceDelegateIntent>,
    scanner_handle: Option<tokio::task::JoinHandle<()>>,
    dispatcher_handle: Option<tokio::task::JoinHandle<()>>,
}

impl PlatformResourceDelegateShadowActorSystem {
    pub fn new(pool: ApiResourceDbPool, transaction_pool: ApiTransactionDbPool) -> Self {
        let (shutdown_tx, shutdown_rx1) = broadcast::channel(1);
        let shutdown_rx2 = shutdown_tx.subscribe();
        let (intent_tx, intent_rx) = mpsc::channel(100);
        let scanner = Arc::new(PlatformResourceDelegateScanner::new(pool.clone()));
        let worker = PlatformResourceDelegateWorker::new(pool, transaction_pool);

        let scanner_clone = scanner.clone();
        let intent_tx_clone = intent_tx.clone();
        tokio::spawn(async move {
            for intent in scanner_clone.scan_round().await {
                if let Err(e) = intent_tx_clone.send(intent).await {
                    error!(error = %e, "Failed to enqueue warm platform resource delegate intent");
                    break;
                }
            }
        });

        let scanner_actor =
            PlatformResourceDelegateScannerActor::new(scanner, intent_tx.clone(), shutdown_rx1);
        let scanner_handle = Some(tokio::spawn(async move {
            scanner_actor.run().await;
        }));
        let dispatcher_actor = PlatformResourceDelegateDispatcherActor::new(
            worker,
            intent_tx.clone(),
            intent_rx,
            shutdown_rx2,
        );
        let dispatcher_handle = Some(tokio::spawn(async move {
            dispatcher_actor.run().await;
        }));
        Self { shutdown_tx, intent_tx, scanner_handle, dispatcher_handle }
    }

    pub async fn trigger_platform_delegation(
        &self,
        resource_trade_no: &str,
    ) -> Result<(), ServiceError> {
        self.intent_tx
            .send(PlatformResourceDelegateIntent::SendPlatformDelegationTaskAck(
                resource_trade_no.to_string(),
            ))
            .await
            .map_err(|e| {
                ServiceError::System(crate::error::system::SystemError::Internal(format!(
                    "platform resource delegate intent channel closed: {e}"
                )))
            })
    }

    pub async fn stop(&mut self) {
        info!("Stopping platform resource delegate shadow system");
        let _ = self.shutdown_tx.send(());
        if let Some(handle) = self.scanner_handle.take() {
            if let Err(err) = handle.await {
                warn!(error = %err, "platform resource delegate scanner join failed during stop");
            }
        }
        if let Some(handle) = self.dispatcher_handle.take() {
            if let Err(err) = handle.await {
                warn!(error = %err, "platform resource delegate dispatcher join failed during stop");
            }
        }
        info!("Platform resource delegate shadow system stopped");
    }
}

pub(crate) async fn init(
    pool: ApiResourceDbPool,
    transaction_pool: ApiTransactionDbPool,
) -> PlatformResourceDelegateShadowActorSystem {
    PlatformResourceDelegateShadowActorSystem::new(pool, transaction_pool)
}

pub async fn scan_and_process_once(
    pool: ApiResourceDbPool,
    transaction_pool: ApiTransactionDbPool,
) -> Result<(), ServiceError> {
    let scanner = PlatformResourceDelegateScanner::new(pool.clone());
    let worker = PlatformResourceDelegateWorker::new(pool, transaction_pool);
    for intent in scanner.scan_round().await {
        worker.handle(intent).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        PLATFORM_RESOURCE_DELEGATE_DEFAULT_CONCURRENCY, PlatformCapacityReservations,
        PlatformResourceDelegateIntent, can_project_platform_terminal_failure,
        release_platform_delegation_capacity, reserve_platform_delegation_capacity,
    };
    use wallet_database::entities::api_resource_delegation::ApiResourceDelegationResultStatus;

    #[test]
    fn platform_resource_delegate_intent_chains_success_stages() {
        let trade_no = "WD_RSC_FAST_WAKE";

        assert_eq!(
            PlatformResourceDelegateIntent::SendPlatformDelegationTaskAck(trade_no.to_string())
                .next_after_success(),
            Some(PlatformResourceDelegateIntent::ExecutePlatformDelegation(trade_no.to_string()))
        );
        assert_eq!(
            PlatformResourceDelegateIntent::ExecutePlatformDelegation(trade_no.to_string())
                .next_after_success(),
            Some(PlatformResourceDelegateIntent::RecoverPlatformDelegation(trade_no.to_string()))
        );
        assert_eq!(
            PlatformResourceDelegateIntent::RecoverPlatformDelegation(trade_no.to_string())
                .next_after_success(),
            Some(PlatformResourceDelegateIntent::UploadPlatformDelegationTxExecReceipt(
                trade_no.to_string()
            ))
        );
        assert_eq!(
            PlatformResourceDelegateIntent::UploadPlatformDelegationTxExecReceipt(
                trade_no.to_string()
            )
            .next_after_success(),
            Some(PlatformResourceDelegateIntent::SendPlatformDelegationResultAck(
                trade_no.to_string()
            ))
        );
        assert_eq!(
            PlatformResourceDelegateIntent::SendPlatformDelegationResultAck(trade_no.to_string())
                .next_after_success(),
            None
        );
    }

    #[test]
    fn platform_resource_delegate_default_concurrency_is_parallel_but_bounded() {
        assert!(PLATFORM_RESOURCE_DELEGATE_DEFAULT_CONCURRENCY > 1);
        assert!(PLATFORM_RESOURCE_DELEGATE_DEFAULT_CONCURRENCY <= 32);
    }

    #[test]
    fn platform_terminal_failure_projection_requires_complete_persisted_fact() {
        assert!(can_project_platform_terminal_failure(1, None, false, false));
        assert!(can_project_platform_terminal_failure(
            0,
            Some(ApiResourceDelegationResultStatus::Fail),
            true,
            true,
        ));
        assert!(!can_project_platform_terminal_failure(
            0,
            Some(ApiResourceDelegationResultStatus::Fail),
            false,
            true,
        ));
        assert!(!can_project_platform_terminal_failure(0, None, false, false));
    }

    #[test]
    fn platform_resource_delegate_capacity_reservation_blocks_oversubscription() {
        let mut reservations = PlatformCapacityReservations::new();
        let key = "TY9:1";

        assert!(reserve_platform_delegation_capacity(&mut reservations, key, 500, 199));
        assert!(reserve_platform_delegation_capacity(&mut reservations, key, 500, 199));
        assert!(!reserve_platform_delegation_capacity(&mut reservations, key, 500, 199));
        assert_eq!(reservations.get(key).copied(), Some(398));
    }

    #[test]
    fn platform_resource_delegate_capacity_release_is_immediately_reusable() {
        let mut reservations = PlatformCapacityReservations::new();
        let key = "TY9:1";

        assert!(reserve_platform_delegation_capacity(&mut reservations, key, 398, 199));
        assert!(reserve_platform_delegation_capacity(&mut reservations, key, 398, 199));
        assert!(!reserve_platform_delegation_capacity(&mut reservations, key, 398, 199));

        release_platform_delegation_capacity(&mut reservations, key, 199);

        assert_eq!(reservations.get(key).copied(), Some(199));
        assert!(reserve_platform_delegation_capacity(&mut reservations, key, 398, 199));
    }
}
