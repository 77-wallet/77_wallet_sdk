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

use crate::{
    CollectDbPool,
    dao::api_fee::ApiFeeDao,
    entities::api_fee::{ApiFeeEntity, ApiFeeStatus},
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
        ApiFeeDao::page_api_fee_with_status(pool.as_ref(), page, page_size, vec_status)
            .await
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
        let fee_req = ApiFeeEntity {
            id: 0,
            name: name.to_string(),
            uid: uid.to_string(),
            from_addr: from_addr.to_string(),
            to_addr: to_addr.to_string(),
            value: value.to_string(),
            validate: validate.to_string(),
            chain_code: chain_code.to_string(),
            token_addr,
            symbol: symbol.to_string(),
            trade_no: trade_no.to_string(),
            trade_type,
            status: ApiFeeStatus::Init,
            nonce: 0,
            tx_hash: None,
            raw_tx: None,
            resource_consume: "".to_string(),
            transaction_fee: "".to_string(),
            transaction_time: None,
            block_height: "".to_string(),
            notes: "".to_string(),
            post_tx_count: 0,
            post_confirm_tx_count: 0,
            err_code: 0,
            err_msg: "".to_string(),
            created_at: Default::default(),
            updated_at: None,
            tx_ack_attempted_at: None,
            tx_ack_sent_at: None,
            building_at: None,
            build_blocked_at: None,
            last_broadcast_at: None,
            tx_res_ack_attempted_at: None,
            tx_res_ack_sent_at: None,
            tx_exec_receipt_attempted_at: None,
            tx_exec_receipt_uploaded_at: None,
            finished_at: None,
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
        ApiFeeDao::update_status_and_err(pool.as_ref(), trade_no, status, err_code, err_msg)
            .await
    }

    // 兼容旧代码，标记为 deprecated
    #[deprecated(
        since = "0.1.0",
        note = "Use legacy_update_api_fee_status_and_err instead."
    )]
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
        ApiFeeDao::legacy_update_next_status(pool.as_ref(), trade_no, status, next_status).await
    }

    // 兼容旧代码，标记为 deprecated
    #[deprecated(
        since = "0.1.0",
        note = "Use legacy_update_api_fee_next_status instead."
    )]
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
    ) -> Result<u64, crate::Error> {
        ApiFeeDao::update_post_tx_count(pool.as_ref(), trade_no).await
    }

    pub async fn update_api_fee_post_confirm_tx_count(
        pool: &CollectDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiFeeDao::update_post_confirm_tx_count(pool.as_ref(), trade_no).await
    }

    pub async fn update_after_build(
        pool: &CollectDbPool,
        trade_no: &str,
        tx_hash: &str,
        raw_tx: &str,
        transaction_fee: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiFeeDao::update_after_build(pool.as_ref(), trade_no, tx_hash, raw_tx, transaction_fee)
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
        ApiFeeDao::scan_confirmed_need_tx_res_ack(pool.as_ref(), limit).await
    }

    /// 扫描需要上传交易执行回执的交易
    ///
    /// 事实条件直接翻译：
    /// - transaction_time IS NOT NULL：链上已给出结果
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
    /// - confirmed 之后不再变化
    /// - 这是"行为事实"，不是"推进事实"
    /// - send_count 记录"尝试次数"，attempted_at 仅记录"首次尝试时间"
    /// - 二者语义不同，不得互相替代
    pub async fn mark_tx_res_ack_attempted(
        pool: &CollectDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiFeeDao::mark_tx_res_ack_attempted(pool.as_ref(), trade_no).await
    }

    /// 标记交易结果 ACK 确认（推进事实）
    ///
    /// 语义：
    /// - 只能在 attempted 之后调用
    /// - 防止重复确认
    /// - 设置终态 finished_at
    pub async fn mark_tx_res_ack_confirmed(
        pool: &CollectDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiFeeDao::mark_tx_res_ack_confirmed(pool.as_ref(), trade_no).await?;

        if rows > 0 {
            Self::recompute_and_update_status(pool, trade_no).await?;
        }

        Ok(rows)
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
        block_height: Option<&str>,
    ) -> Result<u64, crate::Error> {
        let rows = ApiFeeDao::mark_chain_finished(pool.as_ref(), trade_no, block_height).await?;

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

    /// 清除构建阻断标记
    ///
    /// ⚠️ 设计约束：
    /// - 仅允许在"外部事实已发生"的前提下调用（如 fee 到账）
    /// - 本方法不会构建 raw_tx，只是解除构建阻断
    /// - 语义是：解除"不可构建"的事实，允许重新构建
    ///
    /// ⚠️ 调用约定：
    /// - 必须由产生新事实的一方调用（如 fee mqtt 处理器）
    /// - 禁止在 scanner / worker / retry 逻辑中调用
    pub async fn clear_build_blocked(
        pool: &CollectDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiFeeDao::clear_build_blocked(pool.as_ref(), trade_no).await
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
            ApiFeeDao::update_status(
                pool.as_ref(),
                trade_no,
                new_status,
            ).await?;

            tracing::info!(
                trade_no = %trade_no,
                old_status = ?entity.status,
                new_status = ?new_status,
                "fee status recomputed"
            );
        }

        Ok(())
    }
}
