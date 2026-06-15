use std::{sync::Arc, time::Duration};

use chrono::Utc;
use tokio::sync::{broadcast, mpsc};
use tracing::{error, info, warn};
use wallet_database::{
    ApiTransactionDbPool,
    entities::{
        api_resource_delegation::{
            ApiResourceDelegationEntity, ApiResourceDelegationOperationType,
            ApiResourceDelegationRecoverStatus, ApiResourceDelegationResultStatus,
            ApiResourceDelegationSource,
        },
        api_resource_gate::ApiResourceGateResult,
        api_trade_type::ApiTradeType,
    },
    repositories::api_wallet::{
        collect::ApiCollectRepo, resource_delegation::ApiResourceDelegationRepo,
        withdraw::ApiWithdrawRepo,
    },
};
use wallet_transport_backend::request::api_wallet::transaction::{
    TransAckType, TransEventAckReq, TransStatus, TxExecReceiptUploadReq,
};
use wallet_utils::RetryableError as _;

use crate::{
    context::CONTEXT,
    domain::api_wallet::trans::ApiTransDomain,
    error::service::ServiceError,
    infrastructure::{
        api_trans::{
            resource_ack_type::{
                is_original_order_resource_result_fact, merchant_original_resource_result_ack_type,
                merchant_original_resource_result_trans_type, platform_resource_result_ack_type,
                platform_resource_task_trans_type,
            },
            resource_delegation::{
                execute_resource_delegation, resource_delegation_failure_fact,
                resource_delegation_retry_wait_secs,
            },
            shadow_rpc_policy,
        },
        runtime::time::new_production_interval,
    },
};

#[derive(Debug, Clone)]
pub enum PlatformResourceDelegateIntent {
    SendPlatformDelegationTaskAck(String),
    ExecutePlatformDelegation(String),
    RecoverPlatformDelegation(String),
    UploadPlatformDelegationTxExecReceipt(String),
    SendPlatformDelegationResultAck(String),
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
            30,
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
    pool: ApiTransactionDbPool,
    config: PlatformResourceDelegateScannerConfig,
}

impl PlatformResourceDelegateScanner {
    pub fn new(pool: ApiTransactionDbPool) -> Self {
        Self::with_config(pool, PlatformResourceDelegateScannerConfig::default())
    }

    pub fn with_config(
        pool: ApiTransactionDbPool,
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
    pool: ApiTransactionDbPool,
}

impl PlatformResourceDelegateWorker {
    pub fn new(pool: ApiTransactionDbPool) -> Self {
        Self { pool }
    }

    pub async fn handle(&self, intent: PlatformResourceDelegateIntent) -> Result<(), ServiceError> {
        match intent {
            PlatformResourceDelegateIntent::SendPlatformDelegationTaskAck(trade_no) => {
                self.process_task_ack(trade_no).await
            }
            PlatformResourceDelegateIntent::ExecutePlatformDelegation(trade_no) => {
                let result = self.process_execute(trade_no.clone()).await;
                self.handle_execute_failure_if_needed(&trade_no, result).await
            }
            PlatformResourceDelegateIntent::RecoverPlatformDelegation(trade_no) => {
                self.process_recover(trade_no).await
            }
            PlatformResourceDelegateIntent::UploadPlatformDelegationTxExecReceipt(trade_no) => {
                self.process_receipt_upload(trade_no).await
            }
            PlatformResourceDelegateIntent::SendPlatformDelegationResultAck(trade_no) => {
                self.process_result_ack(trade_no).await
            }
        }
    }

    async fn process_task_ack(&self, resource_trade_no: String) -> Result<(), ServiceError> {
        let task = self.load_platform_delegate(&resource_trade_no).await?;
        if task.task_ack_sent_at.is_some() {
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
        let affected = ApiResourceDelegationRepo::claim_build_slot(&self.pool, &resource_trade_no)
            .await
            .map_err(|e| ServiceError::Database(e.into()))?;
        if affected == 0 {
            return Ok(());
        }
        let task = self.load_platform_delegate(&resource_trade_no).await?;
        let tx_hash = execute_resource_delegation(
            crate::context::get_context()?,
            &task,
            "platform_resource_delegate",
        )
        .await?;
        ApiResourceDelegationRepo::mark_broadcast_success(&self.pool, &resource_trade_no, &tx_hash)
            .await
            .map_err(|e| ServiceError::Database(e.into()))?;
        Ok(())
    }

    async fn process_recover(&self, resource_trade_no: String) -> Result<(), ServiceError> {
        let task = self.load_platform_delegate(&resource_trade_no).await?;
        if task.result_received_at.is_some() || task.err_code.is_some() {
            return Ok(());
        }
        let Some(tx_hash) = task.tx_hash.as_deref().filter(|s| !s.trim().is_empty()) else {
            return Ok(());
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
                Ok(())
            }
            Ok(None) => {
                self.schedule_retry(
                    &resource_trade_no,
                    &ServiceError::Parameter(
                        "platform resource delegation recover returned no final result".to_string(),
                    ),
                )
                .await
            }
            Err(err) => self.schedule_retry(&resource_trade_no, &err).await,
        }
    }

    async fn process_receipt_upload(&self, resource_trade_no: String) -> Result<(), ServiceError> {
        let task = self.load_platform_delegate(&resource_trade_no).await?;
        if task.tx_exec_receipt_uploaded_at.is_some() {
            return Ok(());
        }
        let payload = Self::build_receipt_payload(&task)?;
        CONTEXT.get().unwrap().get_global_backend_api().upload_tx_exec_receipt(&payload).await?;
        ApiResourceDelegationRepo::mark_tx_exec_receipt_uploaded_for_source_and_operation(
            &self.pool,
            &resource_trade_no,
            ApiResourceDelegationSource::Platform,
            ApiResourceDelegationOperationType::Delegate,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;

        // 失败路径可能没有 result message，所以在 receipt upload 后 bypass release。
        // 这是失败例外路径；成功路径仍必须等 result ACK 后才释放原单 gate。
        self.project_gate_if_failure(&task).await
    }

    async fn process_result_ack(&self, resource_trade_no: String) -> Result<(), ServiceError> {
        let task = self.load_platform_delegate(&resource_trade_no).await?;
        if task.result_ack_sent_at.is_some() {
            return Ok(());
        }
        if task.result_received_at.is_none() || task.result_payload.is_none() {
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
        CONTEXT
            .get()
            .unwrap()
            .get_global_backend_api()
            .trans_event_ack(&TransEventAckReq::new(&resource_trade_no, trans_type, ack_type))
            .await?;

        // ACK 成功后才能 release 原单 gate。
        // 这样即使 collect/withdraw shadow 独立扫描，也不会在资源结果 ACK 前推进原单。
        self.mark_result_ack_and_project_gate(&task).await
    }

    async fn handle_execute_failure_if_needed(
        &self,
        resource_trade_no: &str,
        result: Result<(), ServiceError>,
    ) -> Result<(), ServiceError> {
        let Err(err) = result else {
            return Ok(());
        };
        match err.retry_policy() {
            wallet_utils::RetryPolicy::Never => {
                let (err_code, err_msg) = resource_delegation_failure_fact(&err);
                ApiResourceDelegationRepo::mark_failed_if_unfinished(
                    &self.pool,
                    resource_trade_no,
                    &err_code,
                    &err_msg,
                )
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;
                let task = self.load_platform_delegate(resource_trade_no).await?;
                self.project_gate_if_failure(&task).await
            }
            wallet_utils::RetryPolicy::Delay => self.schedule_retry(resource_trade_no, &err).await,
        }
    }

    async fn schedule_retry(
        &self,
        resource_trade_no: &str,
        err: &ServiceError,
    ) -> Result<(), ServiceError> {
        let task = self.load_platform_delegate(resource_trade_no).await?;
        let wait_secs = resource_delegation_retry_wait_secs(task.retry_count);
        let next_retry_at = Utc::now() + chrono::Duration::seconds(wait_secs);
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
        Ok(())
    }

    async fn mark_result_ack_and_project_gate(
        &self,
        task: &ApiResourceDelegationEntity,
    ) -> Result<(), ServiceError> {
        let release = Self::gate_release_from_result(task);
        let origin_trade_type =
            task.origin_trade_type.map(|x| x.to_string()).unwrap_or_else(|| "None".to_string());
        let origin_trade_no = task.origin_trade_no.as_deref().unwrap_or("None");
        let (release_origin, release_result) = release
            .as_ref()
            .map(|(o, r)| (o.as_str(), r.to_string()))
            .unwrap_or(("None", "None".to_string()));
        info!(
            resource_trade_no = %task.resource_trade_no,
            origin_trade_no = %origin_trade_no,
            origin_trade_type = %origin_trade_type,
            gate_release_origin = %release_origin,
            gate_release_result = %release_result,
            result_status = ?task.result_status,
            tx_status = %task.tx_status.as_deref().unwrap_or("None"),
            err_code = %task.err_code.as_deref().unwrap_or("None"),
            source = "platform_resource_delegate",
            "Marking resource delegation result ACK and projecting origin gate"
        );
        match task.origin_trade_type {
            Some(x) if x == ApiTradeType::Collect as i64 => {
                ApiCollectRepo::mark_resource_result_ack_sent_and_release_gate(
                    &self.pool,
                    &task.resource_trade_no,
                    release.as_ref().map(|(origin, _)| origin.as_str()),
                    release.as_ref().map(|(_, result)| *result),
                )
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;
            }
            Some(x) if x == ApiTradeType::Withdraw as i64 => {
                ApiWithdrawRepo::mark_resource_result_ack_sent_and_release_gate(
                    &self.pool,
                    &task.resource_trade_no,
                    release.as_ref().map(|(origin, _)| origin.as_str()),
                    release.as_ref().map(|(_, result)| *result),
                )
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;
            }
            _ => {
                ApiResourceDelegationRepo::mark_result_ack_sent(
                    &self.pool,
                    &task.resource_trade_no,
                )
                .await
                .map_err(|e| ServiceError::Database(e.into()))?;
            }
        }
        Ok(())
    }

    async fn project_gate_if_failure(
        &self,
        task: &ApiResourceDelegationEntity,
    ) -> Result<(), ServiceError> {
        if !Self::is_failure(task) {
            return Ok(());
        }
        let Some(origin_trade_no) = task.origin_trade_no.as_deref() else {
            return Ok(());
        };
        let result = ApiResourceGateResult::ResourceDelegationFailedBypass;
        match task.origin_trade_type {
            Some(x) if x == ApiTradeType::Collect as i64 => {
                let rows =
                    ApiCollectRepo::mark_resource_released(&self.pool, origin_trade_no, result)
                        .await
                        .map_err(|e| ServiceError::Database(e.into()))?;
                info!(
                    origin_trade_no = %origin_trade_no,
                    resource_trade_no = %task.resource_trade_no,
                    source = "platform_resource_delegate",
                    rows_affected = %rows,
                    release_reason = ?result,
                    "Release collect gate by delegation failure bypass"
                );
            }
            Some(x) if x == ApiTradeType::Withdraw as i64 => {
                let rows =
                    ApiWithdrawRepo::mark_resource_released(&self.pool, origin_trade_no, result)
                        .await
                        .map_err(|e| ServiceError::Database(e.into()))?;
                info!(
                    origin_trade_no = %origin_trade_no,
                    resource_trade_no = %task.resource_trade_no,
                    source = "platform_resource_delegate",
                    rows_affected = %rows,
                    release_reason = ?result,
                    "Release withdraw gate by delegation failure bypass"
                );
            }
            _ => {}
        }
        Ok(())
    }

    fn gate_release_from_result(
        task: &ApiResourceDelegationEntity,
    ) -> Option<(String, ApiResourceGateResult)> {
        let origin = task.origin_trade_no.clone()?;
        let result = if is_original_order_resource_result_fact(task) {
            match task.result_status {
                Some(ApiResourceDelegationResultStatus::Success) => {
                    ApiResourceGateResult::ResourceDelegationSuccess
                }
                Some(ApiResourceDelegationResultStatus::Fail) => {
                    ApiResourceGateResult::ResourceDelegationFailedBypass
                }
                None => return None,
            }
        } else if task.err_code.is_none() && matches!(task.tx_status.as_deref(), Some("success")) {
            ApiResourceGateResult::ResourceDelegationSuccess
        } else if Self::is_failure(task) {
            ApiResourceGateResult::ResourceDelegationFailedBypass
        } else {
            return None;
        };
        Some((origin, result))
    }

    fn is_failure(task: &ApiResourceDelegationEntity) -> bool {
        task.err_code.is_some() || matches!(task.tx_status.as_deref(), Some("fail"))
    }

    fn build_receipt_payload(
        task: &ApiResourceDelegationEntity,
    ) -> Result<TxExecReceiptUploadReq, ServiceError> {
        let status = if matches!(task.tx_status.as_deref(), Some("success")) {
            TransStatus::Success
        } else if task.err_code.is_some() {
            TransStatus::Fail
        } else {
            return Err(ServiceError::Parameter(
                "resource delegation receipt upload requires success tx_status or failure err_code"
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
    intent_rx: mpsc::Receiver<PlatformResourceDelegateIntent>,
    shutdown_rx: broadcast::Receiver<()>,
}

impl PlatformResourceDelegateDispatcherActor {
    pub fn new(
        worker: PlatformResourceDelegateWorker,
        intent_rx: mpsc::Receiver<PlatformResourceDelegateIntent>,
        shutdown_rx: broadcast::Receiver<()>,
    ) -> Self {
        Self { worker, intent_rx, shutdown_rx }
    }

    pub async fn run(mut self) {
        crate::infrastructure::system_ready::wait_system_ready().await;
        info!("Platform resource delegate shadow dispatcher actor running");
        loop {
            tokio::select! {
                _ = self.shutdown_rx.recv() => break,
                Some(intent) = self.intent_rx.recv() => {
                    if let Err(e) = self.worker.handle(intent).await {
                        error!(error = %e, "Failed to handle platform resource delegate intent");
                    }
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
    scanner_handle: Option<tokio::task::JoinHandle<()>>,
    dispatcher_handle: Option<tokio::task::JoinHandle<()>>,
}

impl PlatformResourceDelegateShadowActorSystem {
    pub fn new(pool: ApiTransactionDbPool) -> Self {
        let (shutdown_tx, shutdown_rx1) = broadcast::channel(1);
        let shutdown_rx2 = shutdown_tx.subscribe();
        let (intent_tx, intent_rx) = mpsc::channel(100);
        let scanner = Arc::new(PlatformResourceDelegateScanner::new(pool.clone()));
        let worker = PlatformResourceDelegateWorker::new(pool);

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
            PlatformResourceDelegateScannerActor::new(scanner, intent_tx, shutdown_rx1);
        let scanner_handle = Some(tokio::spawn(async move {
            scanner_actor.run().await;
        }));
        let dispatcher_actor =
            PlatformResourceDelegateDispatcherActor::new(worker, intent_rx, shutdown_rx2);
        let dispatcher_handle = Some(tokio::spawn(async move {
            dispatcher_actor.run().await;
        }));
        Self { shutdown_tx, scanner_handle, dispatcher_handle }
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

pub(crate) async fn init(pool: ApiTransactionDbPool) -> PlatformResourceDelegateShadowActorSystem {
    PlatformResourceDelegateShadowActorSystem::new(pool)
}

pub async fn scan_and_process_once(pool: ApiTransactionDbPool) -> Result<(), ServiceError> {
    let scanner = PlatformResourceDelegateScanner::new(pool.clone());
    let worker = PlatformResourceDelegateWorker::new(pool);
    for intent in scanner.scan_round().await {
        worker.handle(intent).await?;
    }
    Ok(())
}
