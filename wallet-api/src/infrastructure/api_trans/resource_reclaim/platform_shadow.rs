use std::{fs, path::PathBuf, sync::Arc, time::Duration};

use tokio::sync::{broadcast, mpsc};
use tracing::{error, info, warn};
use wallet_chain_interact::{
    BillResourceConsume,
    tron::operations::{RawTransactionParams, TronTxOperation},
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
        api_wallet::{
            adapter::tx::RawTx,
            trans::{ApiTransDomain, shared_mechanism},
        },
        chain::adapter::ChainAdapterFactory,
    },
    error::{service::ServiceError, system::SystemError},
    infrastructure::{
        api_trans::{
            resource_ack_type::{
                platform_resource_result_ack_type, platform_resource_task_trans_type,
            },
            resource_amount::parse_resource_delegation_native_trx_units,
            resource_authorization::{
                ResourceDelegationSigner, new_tron_undelegate_args,
                resolve_resource_delegation_signer,
            },
            resource_chain_facts::{
                resource_failure_receipt_uploadable, resource_success_receipt_uploadable,
            },
            resource_delegation::{
                execute_resource_delegation, mark_recovered_resource_delegation_failure,
                record_resource_uncertain_and_maybe_rebroadcast,
                resource_delegation_is_built_unbroadcast, resource_delegation_retry_wait_secs,
                resource_recovery_error_is_uncertain,
            },
            resource_rpc_auth, shadow_rpc_policy,
        },
        runtime::time::new_production_interval,
    },
};
use wallet_transport_backend::request::api_wallet::transaction::{
    TransAckType, TransEventAckReq, TransStatus, TxExecReceiptUploadReq,
};

#[derive(Debug, Clone)]
pub enum PlatformResourceReclaimIntent {
    SendPlatformUndelegationTaskAck(String),
    ExecutePlatformUndelegation(String),
    RecoverPlatformUndelegation(String),
    UploadPlatformUndelegationTxExecReceipt(String),
    SendPlatformUndelegationResultAck(String),
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
    api_resource_pool: ApiResourceDbPool,
    config: PlatformResourceReclaimScannerConfig,
}

impl PlatformResourceReclaimScanner {
    pub fn new(api_resource_pool: ApiResourceDbPool) -> Self {
        Self::with_config(api_resource_pool, PlatformResourceReclaimScannerConfig::default())
    }

    pub fn with_config(
        api_resource_pool: ApiResourceDbPool,
        config: PlatformResourceReclaimScannerConfig,
    ) -> Self {
        Self { api_resource_pool, config }
    }

    pub async fn scan_round(&self) -> Vec<PlatformResourceReclaimIntent> {
        let mut intents = Vec::new();

        self.scan_collect_platform_undelegation_task_ack(&mut intents).await;
        self.scan_withdraw_platform_undelegation_task_ack(&mut intents).await;
        self.scan_collect_platform_undelegation(&mut intents).await;
        self.scan_withdraw_platform_undelegation(&mut intents).await;
        self.scan_collect_platform_undelegation_recover(&mut intents).await;
        self.scan_withdraw_platform_undelegation_recover(&mut intents).await;
        self.scan_platform_undelegation_receipt_upload(&mut intents).await;
        self.scan_platform_undelegation_result_ack(&mut intents).await;

        intents
    }

    async fn scan_collect_platform_undelegation_task_ack(
        &self,
        intents: &mut Vec<PlatformResourceReclaimIntent>,
    ) {
        match ApiResourceDelegationRepo::scan_need_task_ack_for_origin_type_source_and_operation(
            &self.api_resource_pool,
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
            &self.api_resource_pool,
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
            &self.api_resource_pool,
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
            &self.api_resource_pool,
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
            &self.api_resource_pool,
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
            &self.api_resource_pool,
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

    async fn scan_platform_undelegation_receipt_upload(
        &self,
        intents: &mut Vec<PlatformResourceReclaimIntent>,
    ) {
        match ApiResourceDelegationRepo::scan_need_tx_exec_receipt_upload_for_source_and_operation(
            &self.api_resource_pool,
            ApiResourceDelegationSource::Platform,
            ApiResourceDelegationOperationType::Undelegate,
            self.config.max_items_per_scan,
        )
        .await
        {
            Ok(records) => {
                for record in records {
                    intents.push(
                        PlatformResourceReclaimIntent::UploadPlatformUndelegationTxExecReceipt(
                            record.resource_trade_no,
                        ),
                    );
                }
            }
            Err(e) => {
                error!(
                    error = %e,
                    "Failed to scan platform undelegation receipt upload records"
                );
            }
        }
    }

    async fn scan_platform_undelegation_result_ack(
        &self,
        intents: &mut Vec<PlatformResourceReclaimIntent>,
    ) {
        match ApiResourceDelegationRepo::scan_need_result_ack_for_source_and_operation(
            &self.api_resource_pool,
            ApiResourceDelegationSource::Platform,
            ApiResourceDelegationOperationType::Undelegate,
            self.config.max_items_per_scan,
        )
        .await
        {
            Ok(records) => {
                for record in records {
                    intents.push(PlatformResourceReclaimIntent::SendPlatformUndelegationResultAck(
                        record.resource_trade_no,
                    ));
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to scan platform undelegation result ACK records");
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

#[derive(Clone)]
pub struct PlatformResourceReclaimWorker {
    ctx: &'static crate::context::Context,
}

impl PlatformResourceReclaimWorker {
    const PLATFORM_UNDELEGATION_TERMINAL_ERR_CODE: &'static str = "ERR_6008";

    pub fn new(ctx: &'static crate::context::Context) -> Self {
        Self { ctx }
    }

    pub async fn maintenance_reclaim_residual_energy(
        &self,
        template_resource_trade_no: &str,
    ) -> Result<(i64, String), ServiceError> {
        let mut template = ApiResourceDelegationRepo::get_by_resource_trade_no(
            &self.ctx.api_resource_pool()?,
            template_resource_trade_no,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;

        if template.source != ApiResourceDelegationSource::Platform
            || template.operation_type != ApiResourceDelegationOperationType::Undelegate
            || template.resource_type != ApiResourceType::Energy
        {
            return Err(ServiceError::Parameter(
                "maintenance cleanup requires a platform TRON Energy reclaim record".to_string(),
            ));
        }

        let chain = ChainAdapterFactory::get_tron_adapter_with_ctx(self.ctx).await?;
        let delegated = chain
            .get_provider()
            .delegated_resource(&template.owner_address, &template.receiver_address)
            .await?;
        let delegated_sun =
            delegated.delegated_resource.iter().map(|item| item.frozen_balance_for_energy).sum();
        let delegated_trx = maintenance_energy_trx_units(delegated_sun)?;

        template.native_amount = delegated_trx.to_string();
        template.resource_trade_no = format!("maintenance_residual_{template_resource_trade_no}");

        info!(
            template_resource_trade_no,
            owner_address = %template.owner_address,
            receiver_address = %template.receiver_address,
            delegated_trx,
            source = "platform_resource_reclaim_maintenance",
            "Executing residual platform Energy cleanup"
        );
        let tx_hash = self.execute_maintenance_platform_undelegation(&template).await?;
        Ok((delegated_trx, tx_hash))
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
            PlatformResourceReclaimIntent::UploadPlatformUndelegationTxExecReceipt(
                resource_trade_no,
            ) => self.process_platform_undelegation_tx_exec_receipt(resource_trade_no).await,
            PlatformResourceReclaimIntent::SendPlatformUndelegationResultAck(resource_trade_no) => {
                self.process_platform_undelegation_result_ack(resource_trade_no).await
            }
        }
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

        let resource_task = ApiResourceDelegationRepo::get_by_resource_trade_no(
            &self.ctx.api_resource_pool()?,
            &resource_trade_no,
        )
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

        let attempted = ApiResourceDelegationRepo::mark_task_ack_attempted(
            &self.ctx.api_resource_pool()?,
            &resource_trade_no,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;
        if attempted == 0 {
            return Ok(());
        }
        let backend_api = self.ctx.get_global_backend_api();
        if let Err(err) = backend_api
            .trans_event_ack(&TransEventAckReq::new(
                &resource_trade_no,
                platform_resource_task_trans_type(&resource_task),
                TransAckType::Tx,
            ))
            .await
        {
            let service_err: ServiceError = err.into();
            return Err(service_err);
        }

        let affected = ApiResourceDelegationRepo::mark_task_ack_sent(
            &self.ctx.api_resource_pool()?,
            &resource_trade_no,
        )
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

    async fn process_platform_undelegation_result_ack(
        &self,
        resource_trade_no: String,
    ) -> Result<(), ServiceError> {
        info!(
            resource_trade_no = %resource_trade_no,
            source = "platform_resource_reclaim_shadow",
            "Processing platform undelegation result ACK"
        );

        let resource_task = ApiResourceDelegationRepo::get_by_resource_trade_no(
            &self.ctx.api_resource_pool()?,
            &resource_trade_no,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;

        if resource_task.result_ack_sent_at.is_some() {
            info!(
                resource_trade_no = %resource_trade_no,
                source = "platform_resource_reclaim_shadow",
                "Platform undelegation result ACK already sent"
            );
            return Ok(());
        }

        if resource_task.source != ApiResourceDelegationSource::Platform
            || resource_task.operation_type != ApiResourceDelegationOperationType::Undelegate
        {
            return Err(ServiceError::Parameter(format!(
                "platform undelegation result ACK requires source=Platform + Undelegate, got source={:?} operation={:?}",
                resource_task.source, resource_task.operation_type
            )));
        }

        if resource_task.finished_at.is_none() || resource_task.result_payload.is_none() {
            warn!(
                resource_trade_no = %resource_trade_no,
                source = "platform_resource_reclaim_shadow",
                "Platform undelegation result ACK skipped because result fact is incomplete"
            );
            return Ok(());
        }

        if resource_task.tx_res_received_at.is_none() {
            warn!(
                resource_trade_no = %resource_trade_no,
                source = "platform_resource_reclaim_shadow",
                "Platform undelegation result ACK skipped because MQTT result is not received"
            );
            return Ok(());
        }

        let attempted = ApiResourceDelegationRepo::mark_result_ack_attempted(
            &self.ctx.api_resource_pool()?,
            &resource_trade_no,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;
        if attempted == 0 {
            return Ok(());
        }
        let backend_api = self.ctx.get_global_backend_api();
        if let Err(e) = backend_api
            .trans_event_ack(&TransEventAckReq::new(
                &resource_trade_no,
                platform_resource_task_trans_type(&resource_task),
                platform_resource_result_ack_type(),
            ))
            .await
        {
            return Err(e.into());
        }

        let affected = ApiResourceDelegationRepo::mark_result_ack_sent(
            &self.ctx.api_resource_pool()?,
            &resource_trade_no,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;
        if affected == 0 {
            warn!(
                resource_trade_no = %resource_trade_no,
                source = "platform_resource_reclaim_shadow",
                "Platform undelegation result ACK marked 0 rows"
            );
        } else {
            info!(
                resource_trade_no = %resource_trade_no,
                source = "platform_resource_reclaim_shadow",
                "Platform undelegation result ACK sent successfully"
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
        if Self::is_terminal_platform_undelegation_execute_error(&err) {
            return self.mark_platform_undelegation_terminal_failure(resource_trade_no, &err).await;
        }
        self.schedule_platform_undelegation_rebuild_retry(resource_trade_no, &err).await
    }

    fn is_terminal_platform_undelegation_execute_error(err: &ServiceError) -> bool {
        let message = err.to_string();
        if matches!(err, ServiceError::Parameter(_))
            && (message.contains("authorized resource delegation missing permissionId")
                || message.contains("authorized resource signer not found"))
        {
            return true;
        }

        message.contains("delegated Resource does not exist")
            || message.contains("insufficient delegateFrozenBalance")
    }

    async fn mark_platform_undelegation_terminal_failure(
        &self,
        resource_trade_no: &str,
        err: &ServiceError,
    ) -> Result<(), ServiceError> {
        let err_msg = err.to_string();
        let payload = format!("platform_undelegation_terminal_failure:{err_msg}");
        let affected = ApiResourceDelegationRepo::mark_failed_with_result_if_unfinished(
            &self.ctx.api_resource_pool()?,
            resource_trade_no,
            Some(2),
            Self::PLATFORM_UNDELEGATION_TERMINAL_ERR_CODE,
            &err_msg,
            &payload,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;

        info!(
            resource_trade_no = %resource_trade_no,
            affected = affected,
            error = %err_msg,
            source = "platform_resource_reclaim_shadow",
            "Platform undelegation terminal failure recorded for backend reporting"
        );
        Ok(())
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
                source = "platform_resource_reclaim_shadow",
                "Platform undelegation execution was already claimed or completed"
            );
            return Ok(());
        }

        let delegation = ApiResourceDelegationRepo::get_by_resource_trade_no(
            &self.ctx.api_resource_pool()?,
            &resource_trade_no,
        )
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

        execute_resource_delegation(
            self.ctx,
            &self.ctx.api_resource_pool()?,
            &delegation,
            "platform_resource_delegation_undelegate",
        )
        .await?;

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

        let delegation = ApiResourceDelegationRepo::get_by_resource_trade_no(
            &self.ctx.api_resource_pool()?,
            &resource_trade_no,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;

        if delegation.source != ApiResourceDelegationSource::Platform
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
                source = "platform_resource_reclaim_shadow",
                "Platform undelegation already reached a terminal fact, skipping recover"
            );
            return Ok(());
        }

        if resource_delegation_is_built_unbroadcast(&delegation) {
            execute_resource_delegation(
                self.ctx,
                &self.ctx.api_resource_pool()?,
                &delegation,
                "platform_resource_delegation_undelegate",
            )
            .await?;
            return Ok(());
        }

        let tx_hash =
            delegation.tx_hash.as_deref().filter(|s| !s.trim().is_empty()).ok_or_else(|| {
                ServiceError::Parameter(
                    "platform undelegation recover requires tx_hash".to_string(),
                )
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
                let payload = format!("platform_undelegation_recovered:{}", resp.tx_hash);
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
                    source = "platform_resource_reclaim_shadow",
                    "Platform undelegation recovered as success"
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
            Err(err) => {
                mark_recovered_resource_delegation_failure(
                    &self.ctx.api_resource_pool()?,
                    &delegation,
                    tx_hash,
                    &err,
                    "platform_undelegation_recovered_failure",
                )
                .await?;
                Ok(())
            }
        }
    }

    async fn process_platform_undelegation_tx_exec_receipt(
        &self,
        resource_trade_no: String,
    ) -> Result<(), ServiceError> {
        info!(
            resource_trade_no = %resource_trade_no,
            source = "platform_resource_reclaim_shadow",
            "Processing platform undelegation tx exec receipt upload"
        );

        let delegation = ApiResourceDelegationRepo::get_by_resource_trade_no(
            &self.ctx.api_resource_pool()?,
            &resource_trade_no,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;

        if delegation.tx_exec_receipt_uploaded_at.is_some() {
            info!(
                resource_trade_no = %resource_trade_no,
                source = "platform_resource_reclaim_shadow",
                "Platform undelegation tx exec receipt already uploaded"
            );
            return Ok(());
        }

        if delegation.source != ApiResourceDelegationSource::Platform
            || delegation.operation_type != ApiResourceDelegationOperationType::Undelegate
        {
            return Err(ServiceError::Parameter(format!(
                "platform undelegation receipt upload requires source=Platform + Undelegate, got source={:?} operation={:?}",
                delegation.source, delegation.operation_type
            )));
        }

        let payload = Self::build_platform_undelegation_tx_exec_receipt_payload(&delegation)?;
        let backend_api = self.ctx.get_global_backend_api();
        backend_api.upload_tx_exec_receipt(&payload).await?;

        let affected =
            ApiResourceDelegationRepo::mark_tx_exec_receipt_uploaded_for_source_and_operation(
                &self.ctx.api_resource_pool()?,
                &resource_trade_no,
                ApiResourceDelegationSource::Platform,
                ApiResourceDelegationOperationType::Undelegate,
            )
            .await
            .map_err(|e| ServiceError::Database(e.into()))?;

        if affected == 0 {
            warn!(
                resource_trade_no = %resource_trade_no,
                source = "platform_resource_reclaim_shadow",
                "Platform undelegation tx exec receipt marked 0 rows"
            );
        } else {
            info!(
                resource_trade_no = %resource_trade_no,
                source = "platform_resource_reclaim_shadow",
                "Platform undelegation tx exec receipt uploaded successfully"
            );
        }

        Ok(())
    }

    fn build_platform_undelegation_tx_exec_receipt_payload(
        delegation: &ApiResourceDelegationEntity,
    ) -> Result<TxExecReceiptUploadReq, ServiceError> {
        if delegation.source != ApiResourceDelegationSource::Platform
            || delegation.operation_type != ApiResourceDelegationOperationType::Undelegate
        {
            return Err(ServiceError::Parameter(format!(
                "platform undelegation receipt payload requires source=Platform + Undelegate, got source={:?} operation={:?}",
                delegation.source, delegation.operation_type
            )));
        }

        let is_success = resource_success_receipt_uploadable(delegation);
        let upload_status = if is_success { TransStatus::Success } else { TransStatus::Fail };
        let remark = if is_success { "" } else { delegation.err_msg.as_deref().unwrap_or("") };
        if !is_success
            && !resource_failure_receipt_uploadable(delegation)
            && delegation.err_code.as_deref().is_none_or(|s| s.trim().is_empty())
            && remark.trim().is_empty()
        {
            return Err(ServiceError::Parameter(
                "platform undelegation failure receipt requires err_code or err_msg".to_string(),
            ));
        }

        let mut payload = TxExecReceiptUploadReq::new(
            Some(&delegation.owner_address),
            Some(&delegation.receiver_address),
            &delegation.resource_trade_no,
            platform_resource_task_trans_type(delegation),
            delegation.tx_hash.as_deref(),
            upload_status,
            remark,
        );

        if !is_success {
            if let Some(err_code) = delegation.err_code.as_deref().filter(|s| !s.is_empty()) {
                payload = payload.with_error_code(err_code);
            }
        }

        Ok(payload)
    }

    async fn execute_maintenance_platform_undelegation(
        &self,
        delegation: &ApiResourceDelegationEntity,
    ) -> Result<String, ServiceError> {
        if !shared_mechanism::is_tron_chain(&delegation.chain_code) {
            return Err(ServiceError::Parameter(format!(
                "platform resource reclaim only supports tron, got {}",
                delegation.chain_code
            )));
        }

        let trx_amount = parse_resource_delegation_native_trx_units(&delegation.native_amount)?;
        let resource = Self::tron_resource_name(delegation.resource_type);
        let chain = ChainAdapterFactory::get_tron_adapter_with_ctx(self.ctx).await?;
        let _chain_rpc_guard = crate::infrastructure::chain_rpc_guard::acquire_if_guarded_with_ctx(
            self.ctx,
            &delegation.chain_code,
        )
        .await;
        let signer = resolve_resource_delegation_signer(self.ctx, delegation).await?;
        let args = new_tron_undelegate_args(
            &delegation.owner_address,
            &delegation.receiver_address,
            trx_amount,
            resource,
            signer.permission_id,
        )?;
        let raw = args.build_raw_transaction(chain.get_provider()).await?;
        let (tx_hash, raw_tx) =
            self.sign_maintenance_platform_undelegation(delegation, &signer, raw).await?;
        let tx_resp = ApiTransDomain::broadcast_transfer(
            self.ctx,
            &delegation.chain_code,
            raw_tx,
            Some(&tx_hash),
        )
        .await?;

        let Some(tx) = tx_resp else {
            return Err(ServiceError::Parameter(
                "maintenance platform undelegation broadcast result uncertain".to_string(),
            ));
        };
        if tx.tx_hash != tx_hash {
            return Err(ServiceError::System(SystemError::Internal(
                "maintenance platform undelegation tx_hash mismatch".to_string(),
            )));
        }
        Ok(tx_hash)
    }

    async fn sign_maintenance_platform_undelegation(
        &self,
        delegation: &ApiResourceDelegationEntity,
        signer: &ResourceDelegationSigner,
        mut raw: RawTransactionParams,
    ) -> Result<(String, RawTx), ServiceError> {
        let chain = ChainAdapterFactory::get_tron_adapter_with_ctx(self.ctx).await?;
        let provider = chain.get_provider();
        let consumer =
            provider.transfer_fee(&delegation.owner_address, None, &raw.raw_data_hex, 1).await?;
        let balance = chain.balance(&delegation.owner_address, None).await?;
        if balance.to::<i64>() < consumer.transaction_fee_i64() {
            return Err(ServiceError::Parameter(format!(
                "maintenance platform undelegation balance is insufficient: balance={}, need={}",
                balance,
                consumer.transaction_fee_i64()
            )));
        }

        let handles = self.ctx.get_handles_arc().await?;
        let private_key_manager = handles.get_global_private_key_manager();
        let private_key = private_key_manager
            .get_private_key(&signer.signer_address, &delegation.chain_code)
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

    async fn schedule_platform_undelegation_rebuild_retry(
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
            source = "platform_resource_reclaim_shadow",
            "Platform undelegation reset for rebuild"
        );
        Ok(())
    }
}

fn maintenance_energy_trx_units(delegated_sun: i64) -> Result<i64, ServiceError> {
    const SUN_PER_TRX: i64 = 1_000_000;

    if delegated_sun <= 0 {
        return Err(ServiceError::Parameter(
            "no residual delegated Energy exists on chain".to_string(),
        ));
    }
    if delegated_sun % SUN_PER_TRX != 0 {
        return Err(ServiceError::Parameter(format!(
            "residual delegated Energy is not a whole TRX amount: {delegated_sun} SUN"
        )));
    }
    Ok(delegated_sun / SUN_PER_TRX)
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
    pub fn new(ctx: &'static crate::context::Context) -> Result<Self, ServiceError> {
        let api_resource_pool = ctx.api_resource_pool()?;
        let (shutdown_tx, shutdown_rx1) = broadcast::channel(1);
        let shutdown_rx2 = shutdown_tx.subscribe();
        let (intent_tx, intent_rx) = mpsc::channel(100);

        let scanner = Arc::new(PlatformResourceReclaimScanner::new(api_resource_pool.clone()));
        let worker = PlatformResourceReclaimWorker::new(ctx);

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

        Ok(Self { shutdown_tx, scanner_handle, dispatcher_handle })
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

pub(crate) async fn init(
    ctx: &'static crate::context::Context,
) -> Result<PlatformResourceReclaimShadowActorSystem, ServiceError> {
    PlatformResourceReclaimShadowActorSystem::new(ctx)
}

pub async fn scan_and_process_once(
    ctx: &'static crate::context::Context,
) -> Result<(), ServiceError> {
    let api_resource_pool = ctx.api_resource_pool()?;
    let scanner = PlatformResourceReclaimScanner::new(api_resource_pool.clone());
    let worker = PlatformResourceReclaimWorker::new(ctx);

    for intent in scanner.scan_round().await {
        worker.handle(intent).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wallet_database::{
        SqliteContext,
        entities::api_resource_delegation::{ApiResourceDelegationMode, NewApiResourceDelegation},
    };

    async fn test_ctx() -> &'static crate::context::Context {
        crate::testkit::context::api_trans_test_ctx().await
    }

    #[test]
    fn maintenance_energy_amount_uses_exact_whole_trx_chain_balance() {
        assert_eq!(maintenance_energy_trx_units(3_145_000_000).expect("amount"), 3_145);
        assert!(maintenance_energy_trx_units(0).is_err());
        assert!(maintenance_energy_trx_units(3_145_000_001).is_err());
    }

    #[tokio::test]
    async fn scanner_finds_platform_undelegation_for_collect_and_withdraw() {
        let ctx = crate::testkit::context::api_trans_test_ctx().await;
        let pool = ctx.api_resource_pool().expect("transaction pool");

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
        let ctx = crate::testkit::context::api_trans_test_ctx().await;
        let pool = ctx.api_resource_pool().expect("transaction pool");

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
    async fn platform_undelegation_terminal_failure_reports_receipt_and_result_ack() {
        let ctx = test_ctx().await;
        let pool = ctx.api_resource_pool().expect("transaction pool");

        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate_task(
                "uid",
                "rsc_terminal_reclaim",
                ApiTradeType::Collect,
                ApiResourceDelegationOperationType::Undelegate,
                "tron",
                "owner",
                "receiver",
                ApiResourceType::Energy,
                "5",
                "100",
            )
            .with_delegation_auth(ApiResourceDelegationMode::AuthorizedAddress, None),
        )
        .await
        .expect("insert platform reclaim");

        let worker = PlatformResourceReclaimWorker::new(ctx);
        worker
            .handle_platform_undelegation_execute_failure_if_needed(
                "rsc_terminal_reclaim",
                Err(ServiceError::Parameter(
                    "authorized resource delegation missing permissionId: trade_no=rsc_terminal_reclaim"
                        .to_string(),
                )),
            )
            .await
            .expect("record terminal failure");

        let got =
            ApiResourceDelegationRepo::get_by_resource_trade_no(&pool, "rsc_terminal_reclaim")
                .await
                .expect("load reclaim");
        assert_eq!(
            got.err_code.as_deref(),
            Some(PlatformResourceReclaimWorker::PLATFORM_UNDELEGATION_TERMINAL_ERR_CODE)
        );
        assert!(got.err_msg.as_deref().unwrap_or_default().contains("missing permissionId"));
        assert_eq!(got.result_status, Some(ApiResourceDelegationResultStatus::Fail));
        assert!(got.finished_at.is_some());
        assert!(
            got.result_payload
                .as_deref()
                .unwrap_or_default()
                .contains("platform_undelegation_terminal_failure")
        );

        let receipt_rows =
            ApiResourceDelegationRepo::scan_need_tx_exec_receipt_upload_for_source_and_operation(
                &pool,
                ApiResourceDelegationSource::Platform,
                ApiResourceDelegationOperationType::Undelegate,
                100,
            )
            .await
            .expect("scan receipt upload");
        assert!(receipt_rows.iter().any(|row| row.resource_trade_no == "rsc_terminal_reclaim"));

        let result_ack_rows =
            ApiResourceDelegationRepo::scan_need_result_ack_for_source_and_operation(
                &pool,
                ApiResourceDelegationSource::Platform,
                ApiResourceDelegationOperationType::Undelegate,
                100,
            )
            .await
            .expect("scan result ack");
        assert!(!result_ack_rows.iter().any(|row| row.resource_trade_no == "rsc_terminal_reclaim"));

        ApiResourceDelegationRepo::mark_backend_result_received(
            &pool,
            "rsc_terminal_reclaim",
            ApiResourceDelegationResultStatus::Fail,
            Some(2),
            Some(PlatformResourceReclaimWorker::PLATFORM_UNDELEGATION_TERMINAL_ERR_CODE),
            Some("authorized resource delegation missing permissionId"),
            Some("backend_terminal_failure"),
        )
        .await
        .expect("record backend terminal result");
        let result_ack_rows =
            ApiResourceDelegationRepo::scan_need_result_ack_for_source_and_operation(
                &pool,
                ApiResourceDelegationSource::Platform,
                ApiResourceDelegationOperationType::Undelegate,
                100,
            )
            .await
            .expect("scan result ack after backend result");
        assert!(result_ack_rows.iter().any(|row| row.resource_trade_no == "rsc_terminal_reclaim"));
    }

    #[tokio::test]
    async fn platform_undelegation_chain_validation_error_is_terminal_failure() {
        let ctx = crate::testkit::context::api_trans_test_ctx().await;
        let pool = ctx.api_resource_pool().expect("transaction pool");

        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate_task(
                "uid",
                "rsc_terminal_chain_validation",
                ApiTradeType::Collect,
                ApiResourceDelegationOperationType::Undelegate,
                "tron",
                "owner",
                "receiver",
                ApiResourceType::Energy,
                "5",
                "100",
            ),
        )
        .await
        .expect("insert platform reclaim");

        let worker = PlatformResourceReclaimWorker::new(ctx);
        worker
            .handle_platform_undelegation_execute_failure_if_needed(
                "rsc_terminal_chain_validation",
                Err(ServiceError::ChainInteract(
                    wallet_chain_interact::Error::RpcNode(
                        "contract validation error class org.tron.core.exception.ContractValidateException : delegated Resource does not exist"
                            .to_string(),
                    ),
                )),
            )
            .await
            .expect("record terminal failure");

        let got = ApiResourceDelegationRepo::get_by_resource_trade_no(
            &pool,
            "rsc_terminal_chain_validation",
        )
        .await
        .expect("load reclaim");
        assert_eq!(
            got.err_code.as_deref(),
            Some(PlatformResourceReclaimWorker::PLATFORM_UNDELEGATION_TERMINAL_ERR_CODE)
        );
        assert!(
            got.err_msg
                .as_deref()
                .unwrap_or_default()
                .contains("delegated Resource does not exist")
        );
        assert_eq!(got.result_status, Some(ApiResourceDelegationResultStatus::Fail));
    }

    #[tokio::test]
    async fn scanner_finds_platform_undelegation_recover_for_collect_and_withdraw() {
        let ctx = crate::testkit::context::api_trans_test_ctx().await;
        let pool = ctx.api_resource_pool().expect("transaction pool");

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

    #[tokio::test]
    async fn platform_undelegation_rebuild_clears_broadcast_hash() {
        let ctx = crate::testkit::context::api_trans_test_ctx().await;
        let pool = ctx.api_resource_pool().expect("transaction pool");

        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate_task(
                "uid",
                "rsc_platform_undelegate_rebuild_retry",
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
        .expect("insert platform undelegate");
        ApiResourceDelegationRepo::mark_broadcast_success(
            &pool,
            "rsc_platform_undelegate_rebuild_retry",
            "tx_hash_clear",
        )
        .await
        .expect("mark broadcast success");

        let worker = PlatformResourceReclaimWorker::new(ctx);
        worker
            .schedule_platform_undelegation_rebuild_retry(
                "rsc_platform_undelegate_rebuild_retry",
                &ServiceError::Parameter("build failed".to_string()),
            )
            .await
            .expect("schedule rebuild retry");

        let persisted = ApiResourceDelegationRepo::get_by_resource_trade_no(
            &pool,
            "rsc_platform_undelegate_rebuild_retry",
        )
        .await
        .expect("load task");
        assert_eq!(persisted.tx_hash, None);
    }

    #[tokio::test]
    async fn scanner_finds_platform_undelegation_receipt_upload_after_success() {
        let ctx = crate::testkit::context::api_trans_test_ctx().await;
        let pool = ctx.api_resource_pool().expect("transaction pool");

        for trade_no in [
            "rsc_platform_undelegate_collect_receipt",
            "rsc_platform_undelegate_withdraw_receipt",
            "rsc_platform_undelegate_uploaded_receipt",
        ] {
            let origin_trade_type = if trade_no == "rsc_platform_undelegate_withdraw_receipt" {
                ApiTradeType::Withdraw
            } else {
                ApiTradeType::Collect
            };
            ApiResourceDelegationRepo::upsert(
                &pool,
                NewApiResourceDelegation::platform_delegate_task(
                    "uid",
                    trade_no,
                    origin_trade_type,
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
            .expect("insert platform undelegate receipt candidate");
        }

        sqlx::query(
            r#"
            UPDATE api_resource_delegation
            SET task_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                building_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                tx_hash = resource_trade_no || '_tx_hash',
                chain_success_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                result_status = 1,
                finished_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE resource_trade_no IN (
                'rsc_platform_undelegate_collect_receipt',
                'rsc_platform_undelegate_withdraw_receipt',
                'rsc_platform_undelegate_uploaded_receipt'
            )
            "#,
        )
        .execute(pool.as_ref())
        .await
        .expect("mark platform undelegate success");

        ApiResourceDelegationRepo::mark_tx_exec_receipt_uploaded_for_source_and_operation(
            &pool,
            "rsc_platform_undelegate_uploaded_receipt",
            ApiResourceDelegationSource::Platform,
            ApiResourceDelegationOperationType::Undelegate,
        )
        .await
        .expect("mark uploaded receipt");

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
            PlatformResourceReclaimIntent::UploadPlatformUndelegationTxExecReceipt(trade_no)
                if trade_no == "rsc_platform_undelegate_collect_receipt"
        )));
        assert!(intents.iter().any(|intent| matches!(
            intent,
            PlatformResourceReclaimIntent::UploadPlatformUndelegationTxExecReceipt(trade_no)
                if trade_no == "rsc_platform_undelegate_withdraw_receipt"
        )));
        assert!(!intents.iter().any(|intent| matches!(
            intent,
            PlatformResourceReclaimIntent::UploadPlatformUndelegationTxExecReceipt(trade_no)
                if trade_no == "rsc_platform_undelegate_uploaded_receipt"
        )));
    }

    #[tokio::test]
    async fn scanner_finds_platform_undelegation_result_ack_after_result() {
        let ctx = crate::testkit::context::api_trans_test_ctx().await;
        let pool = ctx.api_resource_pool().expect("transaction pool");

        for trade_no in [
            "rsc_platform_undelegate_collect_result",
            "rsc_platform_undelegate_withdraw_result",
            "rsc_platform_undelegate_acked_result",
        ] {
            let origin_trade_type = if trade_no == "rsc_platform_undelegate_withdraw_result" {
                ApiTradeType::Withdraw
            } else {
                ApiTradeType::Collect
            };
            ApiResourceDelegationRepo::upsert(
                &pool,
                NewApiResourceDelegation::platform_delegate_task(
                    "uid",
                    trade_no,
                    origin_trade_type,
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
            .expect("insert platform undelegate result candidate");
            ApiResourceDelegationRepo::mark_backend_result_received(
                &pool,
                trade_no,
                ApiResourceDelegationResultStatus::Success,
                None,
                None,
                None,
                Some("payload"),
            )
            .await
            .expect("mark result received");
        }

        ApiResourceDelegationRepo::mark_result_ack_sent(
            &pool,
            "rsc_platform_undelegate_acked_result",
        )
        .await
        .expect("mark result acked");

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
            PlatformResourceReclaimIntent::SendPlatformUndelegationResultAck(trade_no)
                if trade_no == "rsc_platform_undelegate_collect_result"
        )));
        assert!(intents.iter().any(|intent| matches!(
            intent,
            PlatformResourceReclaimIntent::SendPlatformUndelegationResultAck(trade_no)
                if trade_no == "rsc_platform_undelegate_withdraw_result"
        )));
        assert!(!intents.iter().any(|intent| matches!(
            intent,
            PlatformResourceReclaimIntent::SendPlatformUndelegationResultAck(trade_no)
                if trade_no == "rsc_platform_undelegate_acked_result"
        )));
    }

    #[tokio::test]
    async fn platform_undelegation_recover_does_not_send_result_ack_before_mqtt_result() {
        let ctx = crate::testkit::context::api_trans_test_ctx().await;
        let pool = ctx.api_resource_pool().expect("transaction pool");

        ApiResourceDelegationRepo::upsert(
            &pool,
            NewApiResourceDelegation::platform_delegate_task(
                "uid",
                "rsc_platform_undelegate_recovered_result",
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
        .expect("insert platform undelegate result candidate");

        ApiResourceDelegationRepo::mark_result_received(
            &pool,
            "rsc_platform_undelegate_recovered_result",
            ApiResourceDelegationResultStatus::Success,
            None,
            None,
            None,
            Some("platform_undelegation_recovered:tx_hash"),
        )
        .await
        .expect("mark recovered result");

        let scanner = PlatformResourceReclaimScanner::with_config(
            pool,
            PlatformResourceReclaimScannerConfig {
                scan_interval: Duration::from_secs(60),
                max_items_per_scan: 8,
            },
        );

        let intents = scanner.scan_round().await;
        assert!(!intents.iter().any(|intent| matches!(
            intent,
            PlatformResourceReclaimIntent::SendPlatformUndelegationResultAck(trade_no)
                if trade_no == "rsc_platform_undelegate_recovered_result"
        )));
    }

    #[tokio::test]
    async fn platform_undelegation_receipt_payload_uses_reclaim_trans_type() {
        let task = ApiResourceDelegationEntity {
            id: 1,
            uid: "uid".to_string(),
            source: ApiResourceDelegationSource::Platform,
            operation_type: ApiResourceDelegationOperationType::Undelegate,
            origin_trade_no: None,
            origin_trade_type: Some(ApiTradeType::Collect as i64),
            resource_trade_no: "CR_1".to_string(),
            chain_code: "tron".to_string(),
            owner_address: "owner".to_string(),
            receiver_address: "receiver".to_string(),
            delegation_mode: wallet_database::entities::api_resource_delegation::ApiResourceDelegationMode::WithdrawAddress,
            permission_id: None,
            resource_type: ApiResourceType::Energy,
            native_amount: "1".to_string(),
            amount: "100".to_string(),
            status: wallet_database::entities::api_resource_delegation::ApiResourceDelegationStatus::Pending,
            task_ack_sent_at: None,
            task_ack_send_count: 0,
            building_at: None,
            last_broadcast_at: Some(chrono::Utc::now()),
            tx_hash: Some("tx_hash".to_string()),
            raw_tx: None,
            chain_success_at: Some(chrono::Utc::now()),
            chain_failed_at: None,
            transaction_time: Some(chrono::Utc::now()),
            broadcast_uncertain_since_at: None,
            broadcast_uncertain_retry_count: 0,
            broadcast_uncertain_last_checked_at: None,
            broadcast_uncertain_reconciled_at: None,
            broadcast_uncertain_rebroadcast_count: 0,
            tx_exec_receipt_uploaded_at: None,
            result_status: Some(ApiResourceDelegationResultStatus::Success),
            tx_res_received_at: None,
            finished_at: Some(chrono::Utc::now()),
            result_ack_sent_at: None,
            result_ack_send_count: 0,
            result_payload: None,
            fail_type: None,
            err_code: None,
            err_msg: None,
            created_at: chrono::Utc::now(),
            updated_at: None,
        };

        let payload =
            PlatformResourceReclaimWorker::build_platform_undelegation_tx_exec_receipt_payload(
                &task,
            )
            .expect("build receipt payload");
        let value = serde_json::to_value(payload).expect("serialize payload");
        assert_eq!(value["tradeNo"], "CR_1");
        assert_eq!(value["type"], "COL_RSC_RC");
        assert_eq!(value["status"], "SUCCESS");
        assert_eq!(value["hash"], "tx_hash");
    }

    #[tokio::test]
    async fn platform_undelegation_failure_receipt_requires_error_fact() {
        let task = ApiResourceDelegationEntity {
            id: 1,
            uid: "uid".to_string(),
            source: ApiResourceDelegationSource::Platform,
            operation_type: ApiResourceDelegationOperationType::Undelegate,
            origin_trade_no: None,
            origin_trade_type: Some(ApiTradeType::Collect as i64),
            resource_trade_no: "CR_EMPTY_FAIL".to_string(),
            chain_code: "tron".to_string(),
            owner_address: "owner".to_string(),
            receiver_address: "receiver".to_string(),
            delegation_mode: wallet_database::entities::api_resource_delegation::ApiResourceDelegationMode::WithdrawAddress,
            permission_id: None,
            resource_type: ApiResourceType::Energy,
            native_amount: "1".to_string(),
            amount: "100".to_string(),
            status: wallet_database::entities::api_resource_delegation::ApiResourceDelegationStatus::Pending,
            task_ack_sent_at: None,
            task_ack_send_count: 0,
            building_at: None,
            last_broadcast_at: None,
            tx_hash: None,
            raw_tx: None,
            chain_success_at: None,
            chain_failed_at: None,
            transaction_time: None,
            broadcast_uncertain_since_at: None,
            broadcast_uncertain_retry_count: 0,
            broadcast_uncertain_last_checked_at: None,
            broadcast_uncertain_reconciled_at: None,
            broadcast_uncertain_rebroadcast_count: 0,
            tx_exec_receipt_uploaded_at: None,
            result_status: None,
            tx_res_received_at: None,
            finished_at: None,
            result_ack_sent_at: None,
            result_ack_send_count: 0,
            result_payload: None,
            fail_type: None,
            err_code: None,
            err_msg: None,
            created_at: chrono::Utc::now(),
            updated_at: None,
        };

        let err =
            PlatformResourceReclaimWorker::build_platform_undelegation_tx_exec_receipt_payload(
                &task,
            )
            .unwrap_err();
        assert!(
            matches!(err, ServiceError::Parameter(message) if message.contains("requires err_code or err_msg"))
        );
    }

    #[tokio::test]
    async fn platform_undelegation_result_ack_payload_uses_reclaim_trans_type() {
        let task = ApiResourceDelegationEntity {
            id: 1,
            uid: "uid".to_string(),
            source: ApiResourceDelegationSource::Platform,
            operation_type: ApiResourceDelegationOperationType::Undelegate,
            origin_trade_no: None,
            origin_trade_type: Some(ApiTradeType::Collect as i64),
            resource_trade_no: "CR_1".to_string(),
            chain_code: "tron".to_string(),
            owner_address: "owner".to_string(),
            receiver_address: "receiver".to_string(),
            delegation_mode: wallet_database::entities::api_resource_delegation::ApiResourceDelegationMode::WithdrawAddress,
            permission_id: None,
            resource_type: ApiResourceType::Energy,
            native_amount: "1".to_string(),
            amount: "100".to_string(),
            status: wallet_database::entities::api_resource_delegation::ApiResourceDelegationStatus::Pending,
            task_ack_sent_at: None,
            task_ack_send_count: 0,
            building_at: None,
            last_broadcast_at: Some(chrono::Utc::now()),
            tx_hash: Some("tx_hash".to_string()),
            raw_tx: None,
            chain_success_at: Some(chrono::Utc::now()),
            chain_failed_at: None,
            transaction_time: Some(chrono::Utc::now()),
            broadcast_uncertain_since_at: None,
            broadcast_uncertain_retry_count: 0,
            broadcast_uncertain_last_checked_at: None,
            broadcast_uncertain_reconciled_at: None,
            broadcast_uncertain_rebroadcast_count: 0,
            tx_exec_receipt_uploaded_at: None,
            result_status: Some(ApiResourceDelegationResultStatus::Success),
            tx_res_received_at: Some(chrono::Utc::now()),
            finished_at: Some(chrono::Utc::now()),
            result_ack_sent_at: None,
            result_ack_send_count: 0,
            result_payload: Some("payload".to_string()),
            fail_type: None,
            err_code: None,
            err_msg: None,
            created_at: chrono::Utc::now(),
            updated_at: None,
        };

        let ack_req = TransEventAckReq::new(
            &task.resource_trade_no,
            platform_resource_task_trans_type(&task),
            platform_resource_result_ack_type(),
        );
        let value = serde_json::to_value(ack_req).expect("serialize ack");
        assert_eq!(value["tradeNo"], "CR_1");
        assert_eq!(value["type"], "COL_RSC_RC");
        assert_eq!(value["ackType"], "TX_RES");
    }
}
