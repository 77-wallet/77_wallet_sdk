use std::fmt::Display;

#[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
/// ApiCollectEntity 是一个【事实驱动实体】
/// 
/// 设计原则：
/// - 本表不再是状态机
/// - 所有字段表示“已经发生的事实”
/// - 不允许通过更新字段来表达“即将发生”或“期望状态”
/// 
/// 时间字段语义：
/// - transaction_time / finished_at：
///   仅表示链上事实已完成
/// - *_uploaded_at / *_sent_at / *_confirmed_at：
///   表示副作用完成时间
/// 
/// 严禁：
/// - 在副作用逻辑中修改 finished_at
/// - 使用 status 推导未来行为
/// 
/// 所有行为必须通过【事实谓词扫描】触发。
pub struct ApiCollectEntity {
    // ===== Identity / Business =====
    pub id: i64,
    pub name: String,
    pub uid: String,
    pub from_addr: String,
    pub to_addr: String,
    pub value: String,
    pub validate: String,
    pub chain_code: String,
    pub token_addr: Option<String>,
    pub symbol: String,
    pub trade_no: String,
    pub trade_type: u8,
    /// 0 默认值，无意义 1 正常地址 2 风险地址； 归集交易，表示from地址是否为风险地址；提笔订单，表示to地址是否为风险地址
    pub risk_addr: u8,
    pub status: ApiCollectStatus, // UI/人类可读状态，不参与执行逻辑
    pub nonce: i64,
    pub tx_hash: Option<String>,
    pub transaction_fee: String,
    pub transaction_time: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
    pub block_height: String,
    pub notes: String,
    pub post_tx_count: u32,
    pub post_confirm_tx_count: u32,
    pub err_code: u32,
    pub err_msg: String,

    // ===== Order ACK（接单事实）=====
    /// Order ACK Attempt：尝试发送订单 ACK（行为事实）
    pub order_ack_attempted_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
    /// Order ACK：确认已接收并持久化该订单（不代表已执行）
    pub order_ack_sent_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,

    // ===== Build / Broadcast Execution Facts =====
    #[serde(skip_serializing)]
    pub raw_tx: Option<String>,
    #[serde(skip_serializing)]
    pub resource_consume: String,
    pub building_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>, // BuildTx 执行占位
    pub build_blocked_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>, // 构建被阻断的事实记录
    pub last_broadcast_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>, // 最近一次 Broadcast 执行占位

    // ===== Result ACK（结果确认事实）=====
    /// Result ACK 尝试时间：第一次尝试发送 ACK 的时间（行为事实）
    /// ⚠️ 这是"行为事实"，不是"推进事实"：不参与 Scanner 的事实判断
    pub result_ack_attempted_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
    /// Result ACK：确认已将链上结果可靠告知后端（推进事实）
    pub result_ack_sent_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
    /// Result ACK 发送次数：仅用于运维观测
    pub result_ack_send_count: u32,

    // ===== Service Fee Upload（服务费上传事实）=====
    /// Service Fee Upload Attempt：尝试上传服务费记录（行为事实）
    pub service_fee_attempted_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
    /// Service Fee Upload：确认已上传服务费记录（推进事实）
    pub service_fee_uploaded_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
    /// Need Service Fee：是否需要上传服务费（不可逆事实）
    /// ⚠️ 设计原则：
    /// - 只允许从 NULL → true
    /// - 一旦为 true，永不回退
    /// - 只能由"费用判定模块"写
    pub need_service_fee: Option<bool>,

    // ===== Tx Exec Receipt Upload（交易执行回执上传事实）=====
    /// Tx Exec Receipt Upload Attempt：尝试上传交易执行回执（行为事实）
    pub tx_exec_receipt_attempted_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
    /// Tx Exec Receipt Upload：确认已上传交易执行回执（推进事实）
    pub tx_exec_receipt_uploaded_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,

    // ===== Terminal Fact =====
    pub finished_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>, // 链上终态事实

    // ===== Meta =====
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    pub updated_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
}

/// 归集交易状态（历史兼容字段）
///
/// ⚠️ 架构定位：
/// - 本枚举为历史遗留设计
/// - 与当前事实驱动模型并非一一对应
/// 
/// 当前用途：
/// - UI 展示
/// - 运维查看
/// - 统计分析
/// - 后台分页
///
/// ❌ 禁止：
/// - Scanner / Executor 通过 status 判断是否可执行
/// - 用于执行决策
/// - 用于判断是否可推进某一步
/// 
/// 未来演进：
/// - 可能被更粗粒度的阶段字段替代
/// - 或完全由前端基于事实字段自行映射
///
/// 注意：status 是事实的派生字段，不是事实本身
#[derive(
    sqlx::Type,
    Debug,
    Clone,
    Copy,
    serde_repr::Deserialize_repr,
    serde_repr::Serialize_repr,
    PartialEq,
)]
#[repr(u8)]
pub enum ApiCollectStatus {
    Init,                  // 0, 初始化
    InsufficientBalance,   // 1, 不足资金
    SufficientBalance,     // 2, 足够资金
    SendingTx,             // 3, 发送交易成功，有hash
    SendingTxFailed,       // 4, 发送交易失败
    SendingTxReport,       // 5, 发送交易报告给服务器
    SendingTxFailedReport, // 6, 发送交易失败报告给服务器，结束
    Success,               // 7，收到成功确认
    Failure,               // 8，收到失败确认
    ConfirmSuccessReport,  // 9, 结束
    ConfirmFailureReport,  // 10, 结束
}

impl Display for ApiCollectStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", *self as u8)
    }
}

impl ApiCollectStatus {
    /// 判断状态是否为终态
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            ApiCollectStatus::SendingTxFailedReport
                | ApiCollectStatus::ConfirmSuccessReport
                | ApiCollectStatus::ConfirmFailureReport
        )
    }
}

impl ApiCollectEntity {
    /// 判断是否需要发送订单 ACK
    /// 
    /// 前置事实：
    /// - collect 已落库（id 存在）
    /// - 订单 ACK 尚未发送
    /// 
    /// 注意：
    /// - attempted 仅用于执行保护，不作为调度条件
    /// - 此函数是事实谓词，用于 Scanner 和 Worker 的双重保险
    pub fn need_order_ack(&self) -> bool {
        self.order_ack_sent_at.is_none()
    }

    /// 根据当前事实字段，派生一个“最接近的叙事状态”
    /// 
    /// ⚠️ 重要说明：
    /// - ApiCollectStatus 是【历史遗留枚举】
    /// - 与当前事实字段【不是一一对应关系】
    /// - 本方法仅用于：
    ///   - UI 展示
    ///   - 后台分页 / 运维查看
    ///   - 统计
    /// 
    /// ❌ 严禁：
    /// - 用于执行决策
    /// - 用于判断是否可推进某一步
    /// 
    /// 设计取舍：
    /// - 这是一个“叙事映射（Narrative Mapping）”
    /// - 不是状态机
    /// - 不保证语义完全精确，只保证“人类可理解”
    /// 
    /// 纯派生函数（Pure Function）
    /// 
    /// 特性保证：
    /// - 不读取数据库
    /// - 不修改任何字段
    /// - 不依赖时间先后
    /// - 不依赖当前 status 值
    /// 
    /// 对同一个实体：
    /// - 多次调用结果必然一致
    pub fn recompute_status(&self) -> ApiCollectStatus {
        use ApiCollectStatus::*;

        // 终态优先（不可逆）
        if self.result_ack_sent_at.is_some() {
            return ConfirmSuccessReport;
        }

        if self.tx_exec_receipt_uploaded_at.is_some() {
            return SendingTxReport;
        }

        if self.transaction_time.is_some() {
            return SendingTx;
        }

        if self.raw_tx.is_some() {
            return SufficientBalance;
        }

        if self.order_ack_sent_at.is_some() {
            return Init;
        }

        Init
    }

    /// 是否已进入【事实终态】
    /// 
    /// 事实终态定义：
    /// - 链上结果已确认（finished_at）
    /// - 或 已可靠告知后端（result_ack_sent_at）
    /// 
    /// 一旦进入事实终态：
    /// - 不允许再产生任何推进性副作用
    /// - 只能允许“幂等重试型”行为（如补 ACK）
    pub fn is_fact_terminal(&self) -> bool {
        self.finished_at.is_some() || self.result_ack_sent_at.is_some()
    }
}
