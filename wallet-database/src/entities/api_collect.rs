use std::fmt::Display;

#[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
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
    /// Order ACK：确认已接收并持久化该订单（不代表已执行）
    pub order_ack_sent_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,

    // ===== Build / Broadcast Execution Facts =====
    #[serde(skip_serializing)]
    pub raw_tx: Option<String>,
    #[serde(skip_serializing)]
    pub resource_consume: String,
    pub building_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>, // BuildTx 执行占位
    pub last_broadcast_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>, // 最近一次 Broadcast 执行占位

    // ===== Result ACK（结果确认事实）=====
    /// Result ACK：确认已将链上结果可靠告知后端
    pub result_ack_sent_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
    /// Result ACK 发送次数：仅用于运维观测
    pub result_ack_send_count: u32,

    // ===== Terminal Fact =====
    pub finished_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>, // 链上终态事实

    // ===== Meta =====
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    pub updated_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
}

/// 归集交易状态（仅用于展示/统计，禁止用于执行决策）
///
/// ❌ 禁止 Scanner / Executor 通过 status 判断是否可执行
/// ✅ status 只能用于：
///   - 后台分页
///   - 运维查看
///   - 用户展示
///   - 统计
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
