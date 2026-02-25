#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CollectStage {
    NeedOrderAck,
    CanBuild,
    NeedTxFeeResAck,
    CanBroadcast,
    NeedRecover,
    NeedTxExecReceiptUpload,
    NeedResultAck,
    NeedServiceFeeUpload,
    FullyBlocked,
}

/// 推进点顺序常量
/// - 顺序与 scan_round 完全一致
/// - try_advance 必须使用此常量，确保行为一致性
/// - 诊断逻辑也必须使用此顺序
pub const COLLECT_ADVANCEMENT_ORDER: &[CollectStage] = &[
    CollectStage::NeedOrderAck,
    CollectStage::CanBuild,
    CollectStage::NeedTxFeeResAck,
    CollectStage::CanBroadcast,
    CollectStage::NeedRecover,
    CollectStage::NeedTxExecReceiptUpload,
    CollectStage::NeedResultAck,
    CollectStage::NeedServiceFeeUpload,
];

impl CollectStage {
    /// 转换为静态字符串，用于 metrics label
    pub fn as_str(&self) -> &'static str {
        match self {
            CollectStage::NeedOrderAck => "need_order_ack",
            CollectStage::CanBuild => "can_build",
            CollectStage::NeedTxFeeResAck => "need_tx_fee_res_ack",
            CollectStage::CanBroadcast => "can_broadcast",
            CollectStage::NeedRecover => "need_recover",
            CollectStage::NeedTxExecReceiptUpload => "need_tx_exec_receipt_upload",
            CollectStage::NeedResultAck => "need_result_ack",
            CollectStage::NeedServiceFeeUpload => "need_service_fee_upload",
            CollectStage::FullyBlocked => "fully_blocked",
        }
    }

    /// 获取阶段基础严重程度
    pub fn base_severity(&self) -> u8 {
        match self {
            CollectStage::NeedOrderAck => 1,
            CollectStage::CanBuild => 0,
            CollectStage::NeedTxFeeResAck => 1,
            CollectStage::CanBroadcast => 0,
            CollectStage::NeedRecover => 2,
            CollectStage::NeedTxExecReceiptUpload => 1,
            CollectStage::NeedResultAck => 1,
            CollectStage::NeedServiceFeeUpload => 1,
            CollectStage::FullyBlocked => 4,
        }
    }

    /// 获取阶段等待时间阈值（分钟）
    pub fn wait_threshold_minutes(&self) -> i64 {
        match self {
            CollectStage::NeedOrderAck => 2,
            CollectStage::CanBuild => 5,
            CollectStage::NeedTxFeeResAck => 3,
            CollectStage::CanBroadcast => 5,
            CollectStage::NeedRecover => 10,
            CollectStage::NeedTxExecReceiptUpload => 15,
            CollectStage::NeedResultAck => 8,
            CollectStage::NeedServiceFeeUpload => 5,
            CollectStage::FullyBlocked => 1,
        }
    }
}

/// 阶段查询构建器
/// 统一 SQL predicate 和 Rust predicate 的来源
/// 确保扫描和推进使用相同的阶段定义
pub trait StageQueryBuilder {
    /// 获取阶段的 SQL 过滤条件
    fn sql_filter(stage: CollectStage) -> String;

    /// 获取阶段的 Rust predicate 函数
    fn rust_predicate(
        stage: CollectStage,
    ) -> fn(&wallet_database::entities::api_collect::ApiCollectEntity) -> bool;
}

/// 默认的阶段查询构建器实现
pub struct DefaultStageQueryBuilder;

impl StageQueryBuilder for DefaultStageQueryBuilder {
    /// 获取阶段的 SQL 过滤条件
    fn sql_filter(stage: CollectStage) -> String {
        match stage {
            CollectStage::NeedOrderAck => {
                "order_ack_sent_at IS NULL".to_string()
            }
            CollectStage::CanBuild => {
                "order_ack_sent_at IS NOT NULL AND raw_tx IS NULL AND (need_service_fee IS NULL OR need_service_fee = false)".to_string()
            }
            CollectStage::NeedTxFeeResAck => {
                "need_service_fee != true AND ever_needed_service_fee = true AND tx_fee_res_ack_sent_at IS NULL AND last_broadcast_at IS NULL AND finished_at IS NULL AND transaction_time IS NULL".to_string()
            }
            CollectStage::CanBroadcast => {
                "raw_tx IS NOT NULL AND last_broadcast_at IS NULL AND finished_at IS NULL AND (ever_needed_service_fee = false OR tx_fee_res_ack_sent_at IS NOT NULL) AND (chain_code NOT IN ('bnb','eth') OR broadcast_uncertain_since_at IS NULL)".to_string()
            }
            CollectStage::NeedRecover => {
                "tx_hash IS NOT NULL AND transaction_time IS NULL AND last_broadcast_at IS NULL AND tx_exec_receipt_uploaded_at IS NULL AND finished_at IS NULL AND err_code IS NULL".to_string()
            }
            CollectStage::NeedTxExecReceiptUpload => {
                "last_broadcast_at IS NOT NULL AND tx_exec_receipt_uploaded_at IS NULL".to_string()
            }
            CollectStage::NeedResultAck => {
                "transaction_time IS NOT NULL AND result_ack_sent_at IS NULL AND finished_at IS NULL".to_string()
            }
            CollectStage::NeedServiceFeeUpload => {
                "need_service_fee = true AND service_fee_uploaded_at IS NULL".to_string()
            }
            CollectStage::FullyBlocked => {
                "".to_string()
            }
        }
    }

    /// 获取阶段的 Rust predicate 函数
    fn rust_predicate(
        stage: CollectStage,
    ) -> fn(&wallet_database::entities::api_collect::ApiCollectEntity) -> bool {
        match stage {
            CollectStage::NeedOrderAck => |collect| collect.order_ack_sent_at.is_none(),
            CollectStage::CanBuild => |collect| {
                collect.order_ack_sent_at.is_some()
                    && collect.raw_tx.is_none()
                    && collect.need_service_fee != Some(true)
            },
            CollectStage::NeedTxFeeResAck => |collect| {
                collect.need_service_fee != Some(true)
                    && collect.ever_needed_service_fee == true
                    && collect.tx_fee_res_ack_sent_at.is_none()
                    && collect.last_broadcast_at.is_none()
                    && collect.finished_at.is_none()
                    && collect.transaction_time.is_none()
            },
            CollectStage::CanBroadcast => |collect| {
                let evm_uncertain_in_progress =
                    matches!(collect.chain_code.as_str(), "bnb" | "eth")
                        && collect.broadcast_uncertain_since_at.is_some();
                collect.raw_tx.is_some()
                    && collect.last_broadcast_at.is_none()
                    && collect.finished_at.is_none()
                    && (collect.ever_needed_service_fee == false
                        || collect.tx_fee_res_ack_sent_at.is_some())
                    && !evm_uncertain_in_progress
            },
            CollectStage::NeedRecover => |collect| {
                collect.tx_hash.is_some()
                    && collect.transaction_time.is_none()
                    && collect.last_broadcast_at.is_none()
                    && collect.tx_exec_receipt_uploaded_at.is_none()
                    && collect.finished_at.is_none()
                    && collect.err_code.is_none()
            },
            CollectStage::NeedTxExecReceiptUpload => |collect| {
                collect.last_broadcast_at.is_some() && collect.tx_exec_receipt_uploaded_at.is_none()
            },
            CollectStage::NeedResultAck => |collect| {
                collect.transaction_time.is_some()
                    && collect.result_ack_sent_at.is_none()
                    && collect.finished_at.is_none()
            },
            CollectStage::NeedServiceFeeUpload => |collect| {
                collect.need_service_fee == Some(true) && collect.service_fee_uploaded_at.is_none()
            },
            CollectStage::FullyBlocked => |_| false,
        }
    }
}
