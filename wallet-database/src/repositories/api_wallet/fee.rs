// ======================= 强顺序保证说明 =======================
// 本文件是 Fee 顺序链中的关键实现：
// TxAck -> BuildTx -> Broadcast -> TxExecReceipt -> TxResAck
//
// ⚠️ 禁止修改以下事实依赖：
// - scan_can_build 必须依赖 tx_ack_sent_at
// - scan_confirmed_need_tx_res_ack 必须依赖 tx_exec_receipt_uploaded_at
//
// 修改这些条件将破坏系统的强顺序与 crash-safe 特性。
// =============================================================

// ======================= 系统不变量 =======================
// 1. need_service_fee 只能由事实层产生 & 消除
//    - 产生：因手续费不足导致的构建失败
//    - 消除：resolve_need_service_fee
// 2. SideEffectWorker 100% 无事实修改能力
// 3. Shadow / Scanner 只负责推进，不负责判断对错
// 4. 所有副作用必须可重复执行（at-least-once）
// =========================================================

// ❗️当前系统不变量（可安全依赖）：
// - need_service_fee 目前**只可能**因手续费不足被设置为 true
// - 因此 resolve_need_service_fee 等价于解决手续费不足问题
//
// ❗️若未来引入其他 need_service_fee 来源，
// 必须：
// 1. 拆分 resolve 方法为明确语义的多个方法
// 2. 或在 SQL 中增加明确的 reason 约束

use crate::{
    CollectDbPool,
    dao::api_fee::ApiFeeDao,
    entities::api_fee::{ApiFeeEntity, ApiFeeStatus, FeeCreatedFact},
};

pub struct ApiFeeRepo;

impl ApiFeeRepo {
    pub async fn list_api_fee(
        pool: &CollectDbPool,
        uid: &str,
    ) -> Result<Vec<ApiFeeEntity>, crate::Error> {
        ApiFeeDao::all_api_fee(pool.as_ref(), uid).await
    }

    pub async fn page_api_fee(
        pool: &CollectDbPool,
        page: i64,
        page_size: i64,
    ) -> Result<(i64, Vec<ApiFeeEntity>), crate::Error> {
        ApiFeeDao::page_api_fee(pool.as_ref(), page, page_size).await
    }

    pub async fn page_api_fee_with_status(
        pool: &CollectDbPool,
        page: i64,
        page_size: i64,
        vec_status: &[ApiFeeStatus],
    ) -> Result<(i64, Vec<ApiFeeEntity>), crate::Error> {
        ApiFeeDao::page_api_fee_with_status(pool.as_ref(), page, page_size, vec_status).await
    }

    pub async fn get_api_fee_by_trade_no(
        pool: &CollectDbPool,
        trade_no: &str,
    ) -> Result<ApiFeeEntity, crate::Error> {
        ApiFeeDao::get_api_fee_by_trade_no(pool.as_ref(), trade_no).await
    }

    pub async fn get_api_fee_by_trade_no_status(
        pool: &CollectDbPool,
        trade_no: &str,
        vec_status: &[ApiFeeStatus],
    ) -> Result<ApiFeeEntity, crate::Error> {
        ApiFeeDao::get_api_fee_by_trade_no_status(pool.as_ref(), trade_no, vec_status).await
    }

    pub async fn upsert_api_fee(
        pool: &CollectDbPool,
        uid: &str,
        name: &str,
        from_addr: &str,
        to_addr: &str,
        value: &str,
        validate: &str,
        chain_code: &str,
        token_addr: Option<String>,
        symbol: &str,
        trade_no: &str,
        trade_type: u8,
    ) -> Result<(), crate::Error> {
        let fee_req = FeeCreatedFact {
            uid: Some(uid.to_string()),
            name: name.to_string(),
            from_addr: from_addr.to_string(),
            to_addr: to_addr.to_string(),
            symbol: symbol.to_string(),
            value: value.to_string(),
            validate: validate.to_string(),
            chain_code: chain_code.to_string(),
            token_addr,
            trade_no: trade_no.to_string(),
            trade_type: trade_type as i64,
            status: ApiFeeStatus::Init,
        };
        ApiFeeDao::add(pool.as_ref(), fee_req).await
    }

    pub async fn update_api_fee_tx_status_nonce(
        pool: &CollectDbPool,
        from_addr: &str,
        chain_code: &str,
        trade_no: &str,
        nonce: i64,
        tx_hash: &str,
        resource_consume: &str,
        transaction_fee: &str,
        status: ApiFeeStatus,
    ) -> Result<(), crate::Error> {
        ApiFeeDao::update_tx_status_nonce(
            &pool.into_inner(),
            from_addr,
            chain_code,
            trade_no,
            nonce,
            tx_hash,
            resource_consume,
            transaction_fee,
            status,
        )
        .await
    }

    pub async fn update_api_fee_tx_status(
        pool: &CollectDbPool,
        trade_no: &str,
        tx_hash: &str,
        resource_consume: &str,
        transaction_fee: &str,
        status: ApiFeeStatus,
    ) -> Result<(), crate::Error> {
        ApiFeeDao::update_tx_status(
            pool.as_ref(),
            trade_no,
            tx_hash,
            resource_consume,
            transaction_fee,
            status,
        )
        .await
    }

    #[deprecated(
        since = "0.1.0",
        note = "Legacy state-machine API. Do not use in fact-driven system. This will be removed in future versions."
    )]
    pub async fn legacy_update_api_fee_status_and_err(
        pool: &CollectDbPool,
        trade_no: &str,
        status: ApiFeeStatus,
        err_code: u32,
        err_msg: &str,
    ) -> Result<u64, crate::Error> {
        ApiFeeDao::update_status_and_err(pool.as_ref(), trade_no, status, err_code, err_msg).await
    }

    // 兼容旧代码，标记为 deprecated
    #[deprecated(since = "0.1.0", note = "Use legacy_update_api_fee_status_and_err instead.")]
    pub async fn update_api_fee_status_and_err(
        pool: &CollectDbPool,
        trade_no: &str,
        status: ApiFeeStatus,
        err_code: u32,
        err_msg: &str,
    ) -> Result<u64, crate::Error> {
        Self::legacy_update_api_fee_status_and_err(pool, trade_no, status, err_code, err_msg).await
    }

    #[deprecated(
        since = "0.1.0",
        note = "LEGACY STATE MACHINE API. Do not use in Shadow / Scanner / fact-driven paths. Use fact-based APIs instead."
    )]
    pub async fn legacy_update_api_fee_next_status(
        pool: &CollectDbPool,
        trade_no: &str,
        status: ApiFeeStatus,
        next_status: ApiFeeStatus,
    ) -> Result<u64, crate::Error> {
        ApiFeeDao::update_next_status(pool.as_ref(), trade_no, status, next_status).await
    }

    // 兼容旧代码，标记为 deprecated
    #[deprecated(since = "0.1.0", note = "Use legacy_update_api_fee_next_status instead.")]
    pub async fn update_api_fee_next_status(
        pool: &CollectDbPool,
        trade_no: &str,
        status: ApiFeeStatus,
        next_status: ApiFeeStatus,
    ) -> Result<u64, crate::Error> {
        Self::legacy_update_api_fee_next_status(pool, trade_no, status, next_status).await
    }

    pub async fn update_api_fee_post_tx_count(
        pool: &CollectDbPool,
        trade_no: &str,
        status: ApiFeeStatus,
    ) -> Result<(), crate::Error> {
        ApiFeeDao::update_post_tx_count(pool.as_ref(), trade_no, status).await
    }

    pub async fn update_api_fee_post_confirm_tx_count(
        pool: &CollectDbPool,
        trade_no: &str,
        status: ApiFeeStatus,
    ) -> Result<(), crate::Error> {
        ApiFeeDao::update_post_confirm_tx_count(pool.as_ref(), trade_no, status).await
    }

    pub async fn update_after_build(
        pool: &CollectDbPool,
        trade_no: &str,
        tx_hash: &str,
        raw_tx: &str,
        transaction_fee: &str,
        nonce: i64,
    ) -> Result<u64, crate::Error> {
        let rows = ApiFeeDao::update_after_build(
            pool.as_ref(),
            trade_no,
            tx_hash,
            raw_tx,
            transaction_fee,
            nonce,
        )
        .await?;

        if rows > 0 {
            Self::recompute_and_update_status(pool, trade_no).await?;
        }

        Ok(rows)
    }

    pub async fn set_tx_ack_sent(pool: &CollectDbPool, trade_no: &str) -> Result<(), crate::Error> {
        ApiFeeDao::mark_tx_ack_sent(pool.as_ref(), trade_no).await.map(|_| ())
    }

    pub async fn set_tx_res_ack_sent(
        pool: &CollectDbPool,
        trade_no: &str,
    ) -> Result<(), crate::Error> {
        ApiFeeDao::mark_tx_res_ack_sent(pool.as_ref(), trade_no).await.map(|_| ())
    }

    /// 标记交易结果 ACK 已发送并标记链上终态
    ///
    /// 语义：
    /// - 交易结果 ACK 已成功发送到后端
    /// - 同时标记链上终态
    /// - 这是一个原子操作，确保两个更新要么都成功，要么都失败
    pub async fn set_tx_res_ack_sent_and_mark_chain_finished(
        pool: &CollectDbPool,
        trade_no: &str,
    ) -> Result<(), crate::Error> {
        let rows =
            ApiFeeDao::mark_tx_res_ack_sent_and_chain_finished(pool.as_ref(), trade_no).await?;

        if rows > 0 {
            Self::recompute_and_update_status(pool, trade_no).await?;
        }

        Ok(())
    }

    pub async fn get_ack_times(
        pool: &CollectDbPool,
        trade_no: &str,
    ) -> Result<
        (
            Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
            Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
        ),
        crate::Error,
    > {
        ApiFeeDao::get_ack_times(pool.as_ref(), trade_no).await
    }

    /// 扫描可构建的交易
    pub async fn scan_can_build(
        pool: &CollectDbPool,
        limit: usize,
    ) -> Result<Vec<ApiFeeEntity>, crate::Error> {
        ApiFeeDao::scan_can_build(pool.as_ref(), limit).await
    }

    /// 扫描可广播的交易
    pub async fn scan_can_broadcast(
        pool: &CollectDbPool,
        limit: usize,
    ) -> Result<Vec<ApiFeeEntity>, crate::Error> {
        ApiFeeDao::scan_can_broadcast(pool.as_ref(), limit).await
    }

    /// 扫描已确认且需要发送交易结果 ACK 的交易
    pub async fn scan_confirmed_need_tx_res_ack(
        pool: &CollectDbPool,
        limit: usize,
    ) -> Result<Vec<ApiFeeEntity>, crate::Error> {
        ApiFeeDao::scan_need_tx_res_ack(pool.as_ref(), limit).await
    }

    /// 扫描需要上传交易执行回执的交易
    ///
    /// 事实条件直接翻译：
    /// - last_broadcast_at IS NOT NULL：交易已成功广播
    /// - finished_at IS NULL：系统生命周期未结束
    /// - tx_exec_receipt_uploaded_at IS NULL：尚未上传执行回执
    pub async fn scan_need_tx_exec_receipt_upload(
        pool: &CollectDbPool,
        limit: usize,
    ) -> Result<Vec<ApiFeeEntity>, crate::Error> {
        ApiFeeDao::scan_need_tx_exec_receipt_upload(pool.as_ref(), limit).await
    }

    /// 扫描需要发送交易 ACK 的交易
    ///
    /// 事实条件直接翻译：
    /// - tx_ack_sent_at IS NULL：尚未发送交易 ACK
    ///
    /// ⚠️ 注意：
    /// - 不检查 tx_ack_attempted_at（这是行为事实，不参与 Scanner 判断）
    /// - attempted 只用于 Worker / 运维观测
    pub async fn scan_need_tx_ack(
        pool: &CollectDbPool,
        limit: usize,
    ) -> Result<Vec<ApiFeeEntity>, crate::Error> {
        ApiFeeDao::scan_need_tx_ack(pool.as_ref(), limit).await
    }

    /// 扫描需要恢复交易的记录
    ///
    /// 事实条件：
    /// - tx_hash IS NOT NULL
    /// - transaction_time IS NULL
    /// - last_broadcast_at IS NULL
    /// - finished_at IS NULL
    /// - err_code IS NULL
    ///
    /// ⚠️ 重要约束：
    /// - SQL必须100%等价于scanner中的need_recover predicate
    /// MUST be equivalent to scanner::need_recover()
    pub async fn scan_need_recover(
        pool: &CollectDbPool,
        limit: usize,
    ) -> Result<Vec<ApiFeeEntity>, crate::Error> {
        ApiFeeDao::scan_need_recover(pool.as_ref(), limit).await
    }

    /// 更新building_at时间
    pub async fn update_building_at(
        pool: &CollectDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiFeeDao::update_building_at(pool.as_ref(), trade_no).await
    }

    /// 更新last_broadcast_at时间
    pub async fn update_last_broadcast_at(
        pool: &CollectDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiFeeDao::update_last_broadcast_at(pool.as_ref(), trade_no).await
    }

    /// Mark successful broadcast execution
    ///
    /// Semantics:
    /// - Represents a successful broadcast attempt
    /// - NOT a chain confirmation
    /// - Idempotent, overwrite allowed
    pub async fn mark_broadcast_executed(
        pool: &CollectDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiFeeDao::mark_broadcast_executed(pool.as_ref(), trade_no).await
    }

    /// 标记 MQTT TxRes 已接收（外部事实）
    ///
    /// 语义：
    /// - 记录业务结果已就绪（来自 MQTT）
    /// - 只写入一次（幂等）
    /// - 不推进链、不修改状态
    ///
    /// ⚠️ 设计约束：
    /// - 禁止写 finished_at
    /// - 禁止修改 status
    /// - Scanner 只在 ResultAck 阶段读取该字段
    pub async fn update_tx_res_received_at(
        pool: &CollectDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiFeeDao::update_tx_res_received_at(pool.as_ref(), trade_no).await
    }

    /// Confirm on-chain transaction finality (fact-based)
    ///
    /// Semantics:
    /// - On-chain transaction has been proven finalized
    /// - This is a fact write, NOT a state-machine transition
    /// - Idempotent
    ///
    /// Does NOT:
    /// - imply broadcast success
    /// - write finished_at
    /// - trigger side effects
    ///
    /// Who can write transaction_time:
    /// | Scenario             | Write transaction_time | Who writes         |
    /// | -------------------- | --------------------- | ------------------ |
    /// | Broadcast success    | ❌                     | No one             |
    /// | MQTT TxRes           | ❌                     | No one             |
    /// | Scanner chain check  | ✅                     | Scanner / Shadow   |
    /// | Recovery chain check | ❌                     | Use confirm_onchain_transaction_fact_with_recover |
    pub async fn confirm_onchain_transaction_fact(
        pool: &CollectDbPool,
        trade_no: &str,
        tx_hash: &str,
        transaction_time: &str,
        transaction_fee: &str,
        resource_consume: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiFeeDao::confirm_onchain_transaction_fact(
            pool.as_ref(),
            trade_no,
            tx_hash,
            transaction_time,
            transaction_fee,
            resource_consume,
        )
        .await?;

        if rows > 0 {
            Self::recompute_and_update_status(pool, trade_no).await?;
        }

        Ok(rows)
    }

    /// Confirm on-chain transaction finality with recover (fact-based)
    ///
    /// Semantics (RECOVER FACT COMPLETION):
    /// - On-chain transaction has been proven finalized via recover
    /// - This implies broadcast MUST have happened (behavior fact)
    /// - Atomically completes both behavior and chain facts
    /// - Idempotent
    ///
    /// Fact completion guarantee:
    /// - If transaction_time is set, last_broadcast_at MUST also be set
    /// - Uses COALESCE to preserve existing broadcast timestamps
    /// - Only updates when transaction_time IS NULL (幂等)
    ///
    /// Who can call this:
    /// | Scenario             | Can call | Reason               |
    /// | -------------------- | -------- | -------------------- |
    /// | Recovery chain check | ✅        | Recover fact completion |
    /// | Scanner chain check  | ❌        | Use regular confirm  |
    /// | Broadcast success    | ❌        | Use mark_broadcast_executed |
    pub async fn confirm_onchain_transaction_fact_with_recover(
        pool: &CollectDbPool,
        trade_no: &str,
        tx_hash: &str,
        last_broadcast_at: &str,
        transaction_time: &str,
        transaction_fee: &str,
        resource_consume: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiFeeDao::confirm_onchain_transaction_fact_with_recover(
            pool.as_ref(),
            trade_no,
            tx_hash,
            last_broadcast_at,
            transaction_time,
            transaction_fee,
            resource_consume,
        )
        .await?;

        if rows > 0 {
            Self::recompute_and_update_status(pool, trade_no).await?;
        }

        Ok(rows)
    }

    /// 兼容旧代码，标记为 deprecated
    ///
    /// ⚠️ DEPRECATED: Use confirm_onchain_transaction_fact_with_recover for recovery scenarios
    /// Use confirm_onchain_transaction_fact for regular confirmation
    #[deprecated(
        since = "0.1.0",
        note = "Use confirm_onchain_transaction_fact_with_recover for recovery or confirm_onchain_transaction_fact for regular confirmation"
    )]
    pub async fn confirm_transaction(
        pool: &CollectDbPool,
        trade_no: &str,
        tx_hash: &str,
        transaction_time: &str,
        transaction_fee: &str,
        resource_consume: &str,
    ) -> Result<u64, crate::Error> {
        // For compatibility, use the new recover method with current time as last_broadcast_at
        // This ensures fact completion even for legacy calls
        let now = chrono::Utc::now().to_rfc3339();
        Self::confirm_onchain_transaction_fact_with_recover(
            pool,
            trade_no,
            tx_hash,
            &now,
            transaction_time,
            transaction_fee,
            resource_consume,
        )
        .await
    }

    /// 兼容旧代码，标记为 deprecated
    ///
    /// ⚠️ DEPRECATED: Legacy state machine API
    #[deprecated(since = "0.1.0", note = "LEGACY STATE MACHINE API. Use fact-based APIs instead.")]
    pub async fn legacy_confirm_transaction(
        pool: &CollectDbPool,
        trade_no: &str,
        tx_hash: &str,
        transaction_time: &str,
        transaction_fee: &str,
        resource_consume: &str,
    ) -> Result<u64, crate::Error> {
        Self::confirm_transaction(
            pool,
            trade_no,
            tx_hash,
            transaction_time,
            transaction_fee,
            resource_consume,
        )
        .await
    }

    /// 标记交易 ACK 尝试（行为事实）
    ///
    /// 语义：
    /// - 只记录第一次尝试时间（COALESCE 幂等写）
    /// - 发送成功后不再变化（WHERE tx_ack_sent_at IS NULL）
    /// - 这是"行为事实"，不是"推进事实"
    /// - 由 SideEffectWorker 调用
    pub async fn mark_tx_ack_attempted(
        pool: &CollectDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiFeeDao::mark_tx_ack_attempted(pool.as_ref(), trade_no).await
    }

    /// 标记交易 ACK 已发送（推进事实）
    ///
    /// 语义：
    /// - 交易 ACK 已成功发送到后端
    /// - 这是副作用完成的事实
    ///
    /// ⚠️ 调用约束：
    /// - 仅允许在交易 ACK 已尝试的前提下调用
    /// - 仅允许调用一次（tx_ack_sent_at IS NULL）
    /// - 由 SideEffectWorker 调用
    pub async fn mark_tx_ack_sent(
        pool: &CollectDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiFeeDao::mark_tx_ack_sent(pool.as_ref(), trade_no).await?;

        if rows > 0 {
            Self::recompute_and_update_status(pool, trade_no).await?;
        }

        Ok(rows)
    }

    /// 标记交易结果 ACK 尝试（行为事实）
    ///
    /// 语义：
    /// - 只记录第一次尝试时间（COALESCE 幂等写）
    /// - 确认后不再变化
    /// - 这是"行为事实"，不是"推进事实"
    pub async fn mark_tx_res_ack_attempted(
        pool: &CollectDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiFeeDao::mark_tx_res_ack_attempted(pool.as_ref(), trade_no).await
    }

    /// 标记交易结果 ACK 发送，并设置终态
    pub async fn mark_tx_res_ack_sent(
        pool: &CollectDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiFeeDao::mark_tx_res_ack_sent(pool.as_ref(), trade_no).await?;

        if rows > 0 {
            Self::recompute_and_update_status(pool, trade_no).await?;
        }

        Ok(rows)
    }

    /// 标记交易执行回执上传尝试（行为事实）
    ///
    /// 语义：
    /// - 只记录第一次尝试时间（COALESCE 幂等写）
    /// - 上传成功后不再变化（WHERE tx_exec_receipt_uploaded_at IS NULL）
    /// - 这是"行为事实"，不是"推进事实"
    /// - 由 SideEffectWorker 调用
    pub async fn mark_tx_exec_receipt_attempted(
        pool: &CollectDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiFeeDao::mark_tx_exec_receipt_attempted(pool.as_ref(), trade_no).await
    }

    /// 标记交易执行回执已上传
    ///
    /// 语义：
    /// - 交易执行回执已成功上传到后端
    /// - 这是副作用完成的事实
    ///
    /// ⚠️ 调用约束：
    /// - 仅允许在回执已上传的前提下调用
    /// - 仅允许调用一次（tx_exec_receipt_uploaded_at IS NULL）
    /// - 由 SideEffectWorker 调用
    pub async fn mark_tx_exec_receipt_uploaded(
        pool: &CollectDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiFeeDao::mark_tx_exec_receipt_uploaded(pool.as_ref(), trade_no).await?;

        if rows > 0 {
            Self::recompute_and_update_status(pool, trade_no).await?;
        }

        Ok(rows)
    }

    /// 标记链上终态
    ///
    /// 语义：
    /// - 链上已确认不可逆（success / failure）
    /// - 系统生命周期结束
    ///
    /// ⚠️ 调用约束：
    /// - 仅允许在链上事实已确认的前提下调用（transaction_time IS NOT NULL）
    /// - 仅允许调用一次（finished_at IS NULL）
    /// - 由链终态确认模块调用
    pub async fn mark_chain_finished(
        pool: &CollectDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiFeeDao::mark_chain_finished(pool.as_ref(), trade_no).await?;

        if rows > 0 {
            Self::recompute_and_update_status(pool, trade_no).await?;
        }

        Ok(rows)
    }

    /// 确认交易时间（如果不存在）
    ///
    /// 语义：
    /// - 只写入 transaction_time 字段
    /// - 仅当 transaction_time IS NULL 时才写入
    /// - 幂等
    /// - 用于 MQTT TxRes 等只知道最终结果已确认的场景
    pub async fn confirm_transaction_time_if_absent(
        pool: &CollectDbPool,
        trade_no: &str,
        transaction_time: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiFeeDao::confirm_transaction_time_if_absent(
            pool.as_ref(),
            trade_no,
            transaction_time,
        )
        .await?;

        if rows > 0 {
            Self::recompute_and_update_status(pool, trade_no).await?;
        }

        Ok(rows)
    }

    /// 作废当前 raw_tx 及其 tx_hash
    ///
    /// ⚠️ 设计铁律：
    /// - 一旦 raw_tx 被判定为不可再广播 / 不可再构建（如手续费不足、前置条件变化）
    /// - 必须同时清空 tx_hash
    /// - 并写入 build_blocked_at 事实
    /// - 确保 scanner / recover 只基于有效事实工作
    ///
    /// 本方法是"事实回滚"，不是状态流转。
    /// 该方法不是重试控制，而是事实作废。
    ///
    /// ⚠️ 调用约束：
    /// - 仅允许对尚未广播的交易调用（transaction_time IS NULL）
    /// - status 仅用于错误标注，不得用于流程推进
    /// - 📌 必须检查返回值 rows_affected()：
    ///   * rows_affected() == 0：表示事实已变更，无需处理
    ///   * rows_affected() == 1：表示成功作废事实
    ///   * 不建议直接忽略返回值
    pub async fn invalidate_raw_tx(
        pool: &CollectDbPool,
        trade_no: &str,
        status: Option<ApiFeeStatus>,
        err_code: Option<u32>,
        err_msg: Option<&str>,
    ) -> Result<u64, crate::Error> {
        ApiFeeDao::invalidate_raw_tx(pool.as_ref(), trade_no, status, err_code, err_msg).await
    }

    /// 重新计算并更新状态
    ///
    /// ⚠️ Repo 写事实铁律
    /// - 任何 *_at / raw_tx / tx_hash 等"事实字段"的成功写入
    /// - 必须紧随 recompute_and_update_status
    ///
    /// ❌ 禁止在 Worker / Scanner / Dispatcher 中直接写 status
    /// ✅ status 只能由 Repo 根据事实统一推导
    pub async fn recompute_and_update_status(
        pool: &CollectDbPool,
        trade_no: &str,
    ) -> Result<(), crate::Error> {
        let entity = Self::get_api_fee_by_trade_no(pool, trade_no).await?;

        let new_status = entity.recompute_status();

        if entity.status != new_status {
            ApiFeeDao::update_status(pool.as_ref(), trade_no, new_status).await?;

            // tracing::info!(
            //     trade_no = %trade_no,
            //     old_status = ?entity.status,
            //     new_status = ?new_status,
            //     "fee status recomputed"
            // );
        }

        Ok(())
    }
}
