use std::fmt;
use wallet_database::entities::api_collect::ApiCollectEntity;

use super::fact_snapshot::{dump_fact_snapshot, fact_mask};
use crate::infrastructure::collect::shadow::{
    predicate::evaluate_stage,
    stage::{COLLECT_ADVANCEMENT_ORDER, CollectStage},
};

#[derive(Debug, Clone)]
pub struct DiagnoseResult {
    pub stage: CollectStage,
    pub reasons: Vec<String>,
    pub facts_snapshot: String,
    pub facts_mask: (u64, u8),
    pub stuck_score: u8, // 0-4，0=可推进，4=完全阻塞
    pub stage_index: u8,
    pub wait_times: Vec<String>,
    pub next_expected_fact: Option<&'static str>,
}

impl fmt::Display for DiagnoseResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "stage={:?}, reasons={:?}, facts={}",
            self.stage, self.reasons, self.facts_snapshot
        )
    }
}

/// 诊断卡单原因
/// 使用统一的阶段评估，确保与推进顺序完全一致
pub fn diagnose_collect(collect: &ApiCollectEntity) -> DiagnoseResult {
    let wait_times = Vec::new();

    // 按推进顺序评估每个阶段，返回第一个可推进的阶段
    for (index, stage) in COLLECT_ADVANCEMENT_ORDER.iter().enumerate() {
        let eval = evaluate_stage(*stage, collect);

        if eval.can_advance {
            return DiagnoseResult {
                stage: *stage,
                reasons: eval.reasons.into_iter().map(|r| r.message).collect(),
                facts_snapshot: dump_fact_snapshot(collect),
                facts_mask: fact_mask(collect),
                stuck_score: calculate_severity(*stage, collect),
                stage_index: index as u8,
                wait_times: wait_times,
                next_expected_fact: Some(get_next_expected_fact(*stage)),
            };
        }
    }

    // 无可推进点：系统完全阻塞（通常是等待外部事实发生）
    let mut reasons = Vec::new();
    let mut next_expected_fact: Option<&'static str> = None;

    // 特例：已上传服务费记录，但 need_service_fee 仍为 true
    // 语义：等待“费用已到/费用问题已解决”的外部事实写入（例如 FeeRes 事件触发 resolve_need_service_fee）
    if collect.need_service_fee == Some(true) && collect.service_fee_uploaded_at.is_some() {
        reasons.push("Waiting for fee resolution (need_service_fee to be cleared)".to_string());
        next_expected_fact = Some("need_service_fee=false");
    }

    if reasons.is_empty() {
        reasons.push("No advancement possible".to_string());
    }

    DiagnoseResult {
        stage: CollectStage::FullyBlocked,
        reasons,
        facts_snapshot: dump_fact_snapshot(collect),
        facts_mask: fact_mask(collect),
        stuck_score: calculate_severity(CollectStage::FullyBlocked, collect),
        stage_index: COLLECT_ADVANCEMENT_ORDER.len() as u8,
        wait_times,
        next_expected_fact,
    }
}

/// 计算严重程度（基于阶段和具体情况）
fn calculate_severity(stage: CollectStage, collect: &ApiCollectEntity) -> u8 {
    let base_severity = stage.base_severity();

    // 计算等待时间权重
    let wait_weight = calculate_wait_weight(stage, collect);

    // 总严重程度，最大为 4
    std::cmp::min(base_severity + wait_weight, 4)
}

/// 计算等待时间权重
fn calculate_wait_weight(stage: CollectStage, collect: &ApiCollectEntity) -> u8 {
    // 基于创建时间计算等待分钟数
    let now = chrono::Utc::now();
    let wait_minutes = (now - collect.created_at).num_minutes();

    // 获取阶段特定的等待时间阈值
    let threshold = stage.wait_threshold_minutes();

    // 等待时间超过阈值后开始计算权重，每超过阈值时间增加 1 权重，最大 2
    if wait_minutes < threshold {
        0
    } else {
        let excess_minutes = wait_minutes - threshold;
        (excess_minutes / threshold).min(2) as u8
    }
}

/// 获取下一期望事实
fn get_next_expected_fact(stage: CollectStage) -> &'static str {
    match stage {
        CollectStage::NeedOrderAck => "order_ack_sent_at",
        CollectStage::CanBuild => "raw_tx",
        CollectStage::NeedTxFeeResAck => "tx_fee_res_ack_sent_at",
        CollectStage::CanBroadcast => "last_broadcast_at",
        CollectStage::NeedRecover => "transaction_time",
        CollectStage::NeedTxExecReceiptUpload => "tx_exec_receipt_uploaded_at",
        CollectStage::NeedResultAck => "result_ack_sent_at",
        CollectStage::NeedServiceFeeUpload => "service_fee_uploaded_at",
        CollectStage::FullyBlocked => "finished_at",
    }
}

/// 可能的卡单记录诊断
pub fn is_potentially_stuck(collect: &ApiCollectEntity) -> bool {
    // 已完成或有错误的订单不视为卡单
    if collect.finished_at.is_some() || collect.err_code.is_some() {
        return false;
    }

    // 有推进点的订单不视为卡单
    let diag = diagnose_collect(collect);
    diag.stuck_score >= 2
}
