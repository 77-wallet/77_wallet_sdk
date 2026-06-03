use crate::response_vo::api_wallet::withdraw_display::FailureReasonDisplay;
use wallet_database::entities::api_withdraw::{ApiWithdrawEntity, ApiWithdrawStatus, ErrCode};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiWithdrawOrderVo {
    pub trade_no: String,
    /// 账户名称，保留历史字段名 name，供审核列表展示使用。
    pub name: String,
    /// 商户平台交易单号，来自 MQTT outOrderId，前端订单号展示优先使用它。
    pub out_order_id: Option<String>,
    /// 商户客户 ID，来自 MQTT clientId，用于会员 ID 展示。
    pub client_id: Option<String>,
    /// 商户交易申请时间，来自 MQTT createTime。
    pub create_time: Option<String>,
    pub chain_code: String,
    pub symbol: String,
    pub value: String,
    pub from_addr: String,
    pub to_addr: String,
    pub status: ApiWithdrawStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub audit_passed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub audit_rejected_at: Option<chrono::DateTime<chrono::Utc>>,
    pub err_msg: Option<String>,
    pub tx_hash: Option<String>,
    /// 实际链上手续费，TRON 对应后台回传的 nativeFee。
    pub transaction_fee: String,
    /// 实际上链区块高度，TRON 对应后台回传的 blockNumber。
    pub block_height: Option<String>,
    /// 实际消耗的 Bandwidth，拆自 resource_consume，便于前端直接展示。
    pub bandwidth_consume: Option<u64>,
    /// 实际消耗的 Energy，拆自 resource_consume，便于前端直接展示。
    pub energy_consume: Option<u64>,
    /// 审核前预估手续费，用于交易未上链前展示。
    pub estimated_transaction_fee: Option<String>,
    /// 预估资源消耗原始 JSON，保留给旧客户端和排查链路使用。
    pub estimated_resource_consume: Option<String>,
    /// 预估 Bandwidth，拆自 estimated_resource_consume。
    pub estimated_bandwidth_consume: Option<u64>,
    /// 预估 Energy，拆自 estimated_resource_consume。
    pub estimated_energy_consume: Option<u64>,
    pub fee_estimated_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 申请时间，派生自 created_at
    pub apply_time: chrono::DateTime<chrono::Utc>,
    /// 签名时间，派生自 audit_passed_at ?? audit_rejected_at
    /// 通过单展示 audit_passed_at，拒绝单展示 audit_rejected_at
    pub sign_time: Option<chrono::DateTime<chrono::Utc>>,
    /// 失败原因类型标识（仅在失败状态时返回，供前端映射文案使用）
    pub failure_reason_display: Option<FailureReasonDisplay>,
}

impl From<ApiWithdrawEntity> for ApiWithdrawOrderVo {
    fn from(entity: ApiWithdrawEntity) -> Self {
        let sign_time = entity.audit_passed_at.or(entity.audit_rejected_at);
        let failure_reason_display = failure_reason_display(
            entity.status,
            entity.err_code,
            entity.failure_stage,
            entity.audit_rejected_at.is_some(),
            entity.chain_failed_at.is_some(),
        );
        let actual_resource = resource_consume_display(Some(&entity.resource_consume));
        let estimated_resource =
            resource_consume_display(entity.estimated_resource_consume.as_deref());
        Self {
            trade_no: entity.trade_no,
            name: entity.name,
            out_order_id: entity.out_order_id,
            client_id: entity.client_id,
            create_time: entity.create_time,
            chain_code: entity.chain_code,
            symbol: entity.symbol,
            value: entity.value,
            from_addr: entity.from_addr,
            to_addr: entity.to_addr,
            status: entity.status,
            created_at: entity.created_at,
            audit_passed_at: entity.audit_passed_at,
            audit_rejected_at: entity.audit_rejected_at,
            err_msg: entity.err_msg,
            tx_hash: entity.tx_hash,
            transaction_fee: entity.transaction_fee,
            block_height: entity.block_height,
            bandwidth_consume: actual_resource.bandwidth,
            energy_consume: actual_resource.energy,
            estimated_transaction_fee: entity.estimated_transaction_fee,
            estimated_resource_consume: entity.estimated_resource_consume,
            estimated_bandwidth_consume: estimated_resource.bandwidth,
            estimated_energy_consume: estimated_resource.energy,
            fee_estimated_at: entity.fee_estimated_at,
            apply_time: entity.created_at,
            sign_time,
            failure_reason_display,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiWithdrawOrderDetailVo {
    pub trade_no: String,
    /// 账户名称，保留历史字段名 name，供审核详情展示使用。
    pub name: String,
    /// 商户平台交易单号，来自 MQTT outOrderId，前端订单号展示优先使用它。
    pub out_order_id: Option<String>,
    /// 商户客户 ID，来自 MQTT clientId，用于会员 ID 展示。
    pub client_id: Option<String>,
    /// 商户交易申请时间，来自 MQTT createTime。
    pub create_time: Option<String>,
    pub chain_code: String,
    pub symbol: String,
    pub value: String,
    pub from_addr: String,
    pub to_addr: String,
    pub validate: String,
    pub status: ApiWithdrawStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub audit_passed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub audit_rejected_at: Option<chrono::DateTime<chrono::Utc>>,
    pub audit_reason: Option<String>,
    pub tx_hash: Option<String>,
    pub err_code: Option<ErrCode>,
    pub err_msg: Option<String>,
    pub notes: Option<String>,
    /// 实际链上手续费，TRON 对应后台回传的 nativeFee。
    pub transaction_fee: String,
    /// 实际上链区块高度，TRON 对应后台回传的 blockNumber。
    pub block_height: Option<String>,
    /// 实际消耗的 Bandwidth，拆自 resource_consume，便于前端直接展示。
    pub bandwidth_consume: Option<u64>,
    /// 实际消耗的 Energy，拆自 resource_consume，便于前端直接展示。
    pub energy_consume: Option<u64>,
    /// 审核前预估手续费，用于交易未上链前展示。
    pub estimated_transaction_fee: Option<String>,
    /// 预估资源消耗原始 JSON，保留给旧客户端和排查链路使用。
    pub estimated_resource_consume: Option<String>,
    /// 预估 Bandwidth，拆自 estimated_resource_consume。
    pub estimated_bandwidth_consume: Option<u64>,
    /// 预估 Energy，拆自 estimated_resource_consume。
    pub estimated_energy_consume: Option<u64>,
    pub fee_estimated_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 申请时间，派生自 created_at
    pub apply_time: chrono::DateTime<chrono::Utc>,
    /// 签名时间，派生自 audit_passed_at ?? audit_rejected_at
    pub sign_time: Option<chrono::DateTime<chrono::Utc>>,
    /// 失败原因类型标识（仅在失败状态时返回，供前端映射文案使用）
    pub failure_reason_display: Option<FailureReasonDisplay>,
}

impl From<ApiWithdrawEntity> for ApiWithdrawOrderDetailVo {
    fn from(entity: ApiWithdrawEntity) -> Self {
        let sign_time = entity.audit_passed_at.or(entity.audit_rejected_at);
        let failure_reason_display = failure_reason_display(
            entity.status,
            entity.err_code,
            entity.failure_stage,
            entity.audit_rejected_at.is_some(),
            entity.chain_failed_at.is_some(),
        );
        let actual_resource = resource_consume_display(Some(&entity.resource_consume));
        let estimated_resource =
            resource_consume_display(entity.estimated_resource_consume.as_deref());
        Self {
            trade_no: entity.trade_no,
            name: entity.name,
            out_order_id: entity.out_order_id,
            client_id: entity.client_id,
            create_time: entity.create_time,
            chain_code: entity.chain_code,
            symbol: entity.symbol,
            value: entity.value,
            from_addr: entity.from_addr,
            to_addr: entity.to_addr,
            validate: entity.validate,
            status: entity.status,
            created_at: entity.created_at,
            audit_passed_at: entity.audit_passed_at,
            audit_rejected_at: entity.audit_rejected_at,
            audit_reason: entity.audit_reason,
            tx_hash: entity.tx_hash,
            err_code: entity.err_code,
            err_msg: entity.err_msg,
            notes: entity.notes,
            transaction_fee: entity.transaction_fee,
            block_height: entity.block_height,
            bandwidth_consume: actual_resource.bandwidth,
            energy_consume: actual_resource.energy,
            estimated_transaction_fee: entity.estimated_transaction_fee,
            estimated_resource_consume: entity.estimated_resource_consume,
            estimated_bandwidth_consume: estimated_resource.bandwidth,
            estimated_energy_consume: estimated_resource.energy,
            fee_estimated_at: entity.fee_estimated_at,
            apply_time: entity.created_at,
            sign_time,
            failure_reason_display,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct ResourceConsumeDisplay {
    bandwidth: Option<u64>,
    energy: Option<u64>,
}

fn resource_consume_display(raw: Option<&str>) -> ResourceConsumeDisplay {
    let Some(raw) = raw.map(str::trim).filter(|item| !item.is_empty() && *item != "0") else {
        return ResourceConsumeDisplay::default();
    };

    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return ResourceConsumeDisplay::default();
    };

    ResourceConsumeDisplay {
        // 实际消耗历史上按 BillResourceConsume 存 net_used，预估值按产品语义存 bandwidth。
        bandwidth: first_u64(&value, &["bandwidth", "net_used", "netUsed"]),
        energy: first_u64(&value, &["energy", "energy_used", "energyUsed"]),
    }
}

fn first_u64(value: &serde_json::Value, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|key| value.get(*key).and_then(serde_json::Value::as_u64))
}

fn failure_reason_display(
    status: ApiWithdrawStatus,
    err_code: Option<ErrCode>,
    failure_stage: Option<wallet_database::entities::api_withdraw::WithdrawFailureStage>,
    has_audit_rejected: bool,
    has_chain_failed: bool,
) -> Option<FailureReasonDisplay> {
    if matches!(status, ApiWithdrawStatus::Success | ApiWithdrawStatus::ConfirmSuccessReport) {
        return None;
    }

    if matches!(status, ApiWithdrawStatus::AuditReject) || has_audit_rejected {
        return Some(FailureReasonDisplay::AuditRejected);
    }

    let has_failure_fact = failure_stage.is_some() || err_code.is_some() || has_chain_failed;
    let is_active_failure_status =
        matches!(status, ApiWithdrawStatus::SendingTxFailed | ApiWithdrawStatus::Failure);
    let is_reported_failure_status = matches!(
        status,
        ApiWithdrawStatus::SendingTxFailedReport | ApiWithdrawStatus::ConfirmFailureReport
    );

    if is_active_failure_status || (is_reported_failure_status && has_failure_fact) {
        Some(FailureReasonDisplay::from_failure_facts(err_code, failure_stage, has_chain_failed))
    } else if has_chain_failed {
        Some(FailureReasonDisplay::from_failure_facts(err_code, failure_stage, has_chain_failed))
    } else {
        None
    }
}
