// ======================= 强顺序保证说明 =======================
// 本文件是 Collect 顺序链中的关键实现：
// OrderAck -> BuildTx -> Broadcast -> TxExecReceipt -> ResultAck
//
// ⚠️ 禁止修改以下事实依赖：
// - scan_can_build 必须依赖 order_ack_sent_at
// - scan_confirmed_need_result_ack 必须依赖 tx_exec_receipt_uploaded_at
//
// 修改这些条件将破坏系统的强顺序与 crash-safe 特性。
// =============================================================

use crate::{
    ApiFundsDbPool,
    dao::api_collect::ApiCollectDao,
    entities::{
        api_collect::{ApiCollectEntity, ApiCollectStatus, CollectCreatedFact, ErrCode},
        asset_token_key::AssetTokenKey,
    },
};

pub struct ApiCollectRepo;

impl ApiCollectRepo {
    pub async fn list_api_collect(
        pool: &ApiFundsDbPool,
    ) -> Result<Vec<ApiCollectEntity>, crate::Error> {
        ApiCollectDao::all_api_collect(pool.read_ref()).await
    }

    pub async fn page_api_collect(
        pool: &ApiFundsDbPool,
        _page: i64,
        _page_size: i64,
    ) -> Result<Vec<ApiCollectEntity>, crate::Error> {
        ApiCollectDao::all_api_collect(pool.read_ref()).await
    }

    pub async fn page_api_collect_with_status(
        pool: &ApiFundsDbPool,
        page: i64,
        page_size: i64,
        vec_status: &[ApiCollectStatus],
    ) -> Result<(i64, Vec<ApiCollectEntity>), crate::Error> {
        ApiCollectDao::page_api_collect_with_status(pool.read_ref(), page, page_size, vec_status)
            .await
    }

    pub async fn get_api_collect_by_trade_no(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<ApiCollectEntity, crate::Error> {
        ApiCollectDao::get_api_collect_by_trade_no(pool.read_ref(), trade_no).await
    }

    pub async fn get_api_collect_by_trade_no_status(
        pool: &ApiFundsDbPool,
        trade_no: &str,
        vec_status: &[ApiCollectStatus],
    ) -> Result<ApiCollectEntity, crate::Error> {
        ApiCollectDao::get_api_collect_by_trade_no_status(pool.read_ref(), trade_no, vec_status)
            .await
    }

    /// Runtime repair helper: query collect candidates from acct_change facts.
    ///
    /// The caller must still perform Rust-side filtering (amount/time window/uniqueness)
    /// and decide whether to backfill `tx_hash` or defer to another onchain-confirm path.
    pub async fn find_candidates_for_acct_change_repair(
        pool: &ApiFundsDbPool,
        chain_code: &str,
        from_addr: &str,
        to_addr: &str,
        token_addr: Option<&str>,
        symbol: &str,
        limit: i64,
    ) -> Result<Vec<ApiCollectEntity>, crate::Error> {
        ApiCollectDao::find_candidates_for_acct_change_repair(
            pool.read_ref(),
            chain_code,
            from_addr,
            to_addr,
            token_addr,
            symbol,
            limit,
        )
        .await
    }

    pub async fn upsert_api_collect(
        pool: &ApiFundsDbPool,
        uid: &str,
        name: &str,
        from_addr: &str,
        to_addr: &str,
        value: &str,
        validate: &str,
        chain_code: &str,
        token_addr: impl Into<AssetTokenKey>,
        symbol: &str,
        trade_no: &str,
        trade_type: u8,
        status: ApiCollectStatus,
        risk_addr: u8,
    ) -> Result<(), crate::Error> {
        let token_addr = token_addr.into();
        let collect_req = CollectCreatedFact {
            uid: Some(uid.to_string()),
            name: name.to_string(),
            from_addr: from_addr.to_string(),
            to_addr: to_addr.to_string(),
            value: value.to_string(),
            validate: validate.to_string(),
            chain_code: chain_code.to_string(),
            token_addr,
            symbol: symbol.to_string(),
            trade_no: trade_no.to_string(),
            trade_type: trade_type as i64,
            risk_addr: risk_addr.to_string(),
            status,
        };
        ApiCollectDao::add(pool.write_ref(), collect_req).await
    }

    pub async fn update_api_collect_to_addr(
        pool: &ApiFundsDbPool,
        trade_no: &str,
        to_addr: &str,
    ) -> Result<(), crate::Error> {
        ApiCollectDao::update_to_addr(pool.write_ref(), trade_no, to_addr).await
    }

    pub async fn update_api_collect_tx_status_nonce(
        pool: &ApiFundsDbPool,
        from_addr: &str,
        chain_code: &str,
        trade_no: &str,
        nonce: i64,
        tx_hash: &str,
        resource_consume: &str,
        transaction_fee: &str,
        status: ApiCollectStatus,
    ) -> Result<u64, crate::Error> {
        let _write_guard = pool.lock_write_with_metric("update_api_collect_tx_status_nonce").await;
        let tx_start = std::time::Instant::now();
        let rows = ApiCollectDao::update_tx_status_nonce(
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
        .await?;
        let elapsed_ms = tx_start.elapsed().as_secs_f64() * 1000.0;
        tracing::info!(
            metric = "write_tx_duration_ms",
            db = "api_funds.db",
            op = "update_api_collect_tx_status_nonce",
            value_ms = %elapsed_ms,
            rows = %rows,
            "collect write finished"
        );

        if rows > 0 {
            Self::recompute_and_update_status(pool, trade_no).await?;
        }

        Ok(rows)
    }
    pub async fn update_api_collect_tx_status(
        pool: &ApiFundsDbPool,
        trade_no: &str,
        tx_hash: &str,
        resource_consume: &str,
        transaction_fee: &str,
        status: ApiCollectStatus,
    ) -> Result<u64, crate::Error> {
        let rows = ApiCollectDao::update_tx_status(
            pool.write_ref(),
            trade_no,
            tx_hash,
            resource_consume,
            transaction_fee,
            status,
        )
        .await?;

        if rows > 0 {
            Self::recompute_and_update_status(pool, trade_no).await?;
        }

        Ok(rows)
    }

    #[deprecated(
        since = "0.1.0",
        note = "Legacy state-machine API. Do not use in fact-driven system. This will be removed in future versions."
    )]
    pub async fn legacy_update_api_collect_status_and_err(
        pool: &ApiFundsDbPool,
        trade_no: &str,
        status: ApiCollectStatus,
        err_code: ErrCode,
        err_msg: &str,
    ) -> Result<u64, crate::Error> {
        ApiCollectDao::update_status_and_err(pool.write_ref(), trade_no, status, err_code, err_msg)
            .await
    }

    // 兼容旧代码，标记为 deprecated
    #[deprecated(since = "0.1.0", note = "Use legacy_update_api_collect_status_and_err instead.")]
    pub async fn update_api_collect_status_and_err(
        pool: &ApiFundsDbPool,
        trade_no: &str,
        status: ApiCollectStatus,
        err_code: ErrCode,
        err_msg: &str,
    ) -> Result<u64, crate::Error> {
        Self::legacy_update_api_collect_status_and_err(pool, trade_no, status, err_code, err_msg)
            .await
    }

    #[deprecated(
        since = "0.1.0",
        note = "LEGACY STATE MACHINE API. Do not use in Shadow / Scanner / fact-driven paths. Use fact-based APIs instead."
    )]
    pub async fn legacy_update_api_collect_next_status(
        pool: &ApiFundsDbPool,
        trade_no: &str,
        status: ApiCollectStatus,
        next_status: ApiCollectStatus,
    ) -> Result<u64, crate::Error> {
        ApiCollectDao::legacy_update_next_status(pool.write_ref(), trade_no, status, next_status)
            .await
    }

    // 兼容旧代码，标记为 deprecated
    #[deprecated(since = "0.1.0", note = "Use legacy_update_api_collect_next_status instead.")]
    pub async fn update_api_collect_next_status(
        pool: &ApiFundsDbPool,
        trade_no: &str,
        status: ApiCollectStatus,
        next_status: ApiCollectStatus,
    ) -> Result<u64, crate::Error> {
        Self::legacy_update_api_collect_next_status(pool, trade_no, status, next_status).await
    }

    #[deprecated(
        since = "0.1.0",
        note = "LEGACY STATE MACHINE API. Do not use in Shadow / Scanner / fact-driven paths. Use fact-based APIs instead."
    )]
    pub async fn legacy_update_api_collect_next_status_and_err(
        pool: &ApiFundsDbPool,
        trade_no: &str,
        status: ApiCollectStatus,
        next_status: ApiCollectStatus,
        err_code: u32,
        err_msg: &str,
    ) -> Result<u64, crate::Error> {
        ApiCollectDao::legacy_update_next_status_and_err(
            pool.write_ref(),
            trade_no,
            status,
            next_status,
            err_code,
            err_msg,
        )
        .await
    }

    // 兼容旧代码，标记为 deprecated
    #[deprecated(
        since = "0.1.0",
        note = "Use legacy_update_api_collect_next_status_and_err instead."
    )]
    pub async fn update_api_collect_next_status_and_err(
        pool: &ApiFundsDbPool,
        trade_no: &str,
        status: ApiCollectStatus,
        next_status: ApiCollectStatus,
        err_code: u32,
        err_msg: &str,
    ) -> Result<u64, crate::Error> {
        Self::legacy_update_api_collect_next_status_and_err(
            pool,
            trade_no,
            status,
            next_status,
            err_code,
            err_msg,
        )
        .await
    }

    pub async fn update_api_collect_post_tx_count(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiCollectDao::update_post_tx_count(pool.write_ref(), trade_no).await
    }

    /// 标记已收到 SER TxRes 推送（AWM_ORDER_TRANS_RES）
    ///
    /// 语义：
    /// - 仅表示“SDK 已收到并持久化 SER 的交易执行结果推送”
    /// - 与链上确认（transaction_time）不是同一事实
    /// - 用于强顺序屏障：TX_RES ACK 禁止早于该事实发送
    pub async fn update_tx_res_received_at(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiCollectDao::update_tx_res_received_at(pool.write_ref(), trade_no).await
    }

    pub async fn update_api_collect_post_confirm_tx_count(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiCollectDao::update_post_confirm_tx_count(pool.write_ref(), trade_no).await
    }

    pub async fn update_after_build(
        pool: &ApiFundsDbPool,
        trade_no: &str,
        tx_hash: &str,
        raw_tx: &str,
        transaction_fee: &str,
        nonce: i64,
    ) -> Result<u64, crate::Error> {
        let rows = ApiCollectDao::update_after_build(
            pool.write_ref(),
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

    pub async fn set_order_ack_sent(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<(), crate::Error> {
        ApiCollectDao::mark_order_ack_sent(pool.write_ref(), trade_no).await.map(|_| ())
    }

    pub async fn get_ack_times(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<
        (
            Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
            Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
        ),
        crate::Error,
    > {
        ApiCollectDao::get_ack_times(pool.read_ref(), trade_no).await
    }

    /// 扫描可构建的交易
    pub async fn scan_can_build(
        pool: &ApiFundsDbPool,
        limit: usize,
    ) -> Result<Vec<ApiCollectEntity>, crate::Error> {
        ApiCollectDao::scan_can_build(pool.read_ref(), limit).await
    }

    /// 扫描可广播的交易
    ///
    /// 事实条件：
    /// - raw_tx IS NOT NULL
    /// - (last_broadcast_at IS NULL) OR (last_broadcast_at IS NOT NULL AND tx_exec_receipt_uploaded_at IS NOT NULL)
    /// - finished_at IS NULL
    /// - (ever_needed_service_fee = false OR tx_fee_res_ack_sent_at IS NOT NULL)
    ///
    /// ⚠️ 重要约束：
    /// - SQL必须100%等价于scanner中的can_broadcast predicate
    /// - 特别注意：ever_needed_service_fee = true的记录
    ///   在tx_fee_res_ack_sent_at IS NULL时永远不能被扫出来
    pub async fn scan_can_broadcast(
        pool: &ApiFundsDbPool,
        limit: usize,
    ) -> Result<Vec<ApiCollectEntity>, crate::Error> {
        ApiCollectDao::scan_can_broadcast(pool.read_ref(), limit).await
    }

    /// 扫描已确认且需要发送Result ACK的交易
    pub async fn scan_confirmed_need_result_ack(
        pool: &ApiFundsDbPool,
        limit: usize,
    ) -> Result<Vec<ApiCollectEntity>, crate::Error> {
        ApiCollectDao::scan_confirmed_need_result_ack(pool.read_ref(), limit).await
    }

    /// 扫描已确认但未上传服务费的交易
    pub async fn scan_confirmed_need_service_fee_upload(
        pool: &ApiFundsDbPool,
        limit: usize,
    ) -> Result<Vec<ApiCollectEntity>, crate::Error> {
        ApiCollectDao::scan_confirmed_need_service_fee_upload(pool.read_ref(), limit).await
    }

    /// 扫描需要发送手续费结果确认 ACK 的交易
    pub async fn scan_confirmed_need_tx_fee_res_ack(
        pool: &ApiFundsDbPool,
        limit: usize,
    ) -> Result<Vec<ApiCollectEntity>, crate::Error> {
        ApiCollectDao::scan_confirmed_need_tx_fee_res_ack(pool.read_ref(), limit).await
    }

    /// 更新building_at时间
    pub async fn update_building_at(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiCollectDao::update_building_at(pool.write_ref(), trade_no).await
    }

    /// 更新last_broadcast_at时间
    pub async fn update_last_broadcast_at(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiCollectDao::update_last_broadcast_at(pool.write_ref(), trade_no).await
    }

    /// Mark successful broadcast execution
    ///
    /// Semantics:
    /// - Represents a successful broadcast attempt
    /// - NOT a chain confirmation
    /// - Idempotent, overwrite allowed
    pub async fn mark_broadcast_executed(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiCollectDao::mark_broadcast_executed(pool.write_ref(), trade_no).await
    }

    /// 记录 EVM 广播/恢复不确定态（RPC 返回 hash 但同节点不可见）
    pub async fn mark_broadcast_uncertain_attempt(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiCollectDao::mark_broadcast_uncertain_attempt(pool.write_ref(), trade_no).await
    }

    /// 标记已执行不确定态超时 reconcile（每个生命周期最多一次）
    pub async fn mark_broadcast_uncertain_reconciled(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiCollectDao::mark_broadcast_uncertain_reconciled(pool.write_ref(), trade_no).await
    }

    /// 记录一次不确定态超时后的自动重建/重播尝试
    pub async fn mark_broadcast_uncertain_rebroadcast_attempted(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiCollectDao::mark_broadcast_uncertain_rebroadcast_attempted(pool.write_ref(), trade_no)
            .await
    }

    /// 清理不确定态追踪字段（广播可见/链上确认后）
    pub async fn clear_broadcast_uncertain_tracking(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiCollectDao::clear_broadcast_uncertain_tracking(pool.write_ref(), trade_no).await
    }

    /// 标记 Result ACK 尝试（行为事实）
    ///
    /// 语义：
    /// - 只记录第一次尝试时间（COALESCE 幂等写）
    /// - confirmed 之后不再变化
    /// - 这是"行为事实"，不是"推进事实"
    pub async fn mark_result_ack_attempted(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiCollectDao::mark_result_ack_attempted(pool.write_ref(), trade_no).await
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
        pool: &ApiFundsDbPool,
        trade_no: &str,
        tx_hash: &str,
        transaction_time: &str,
        transaction_fee: &str,
        resource_consume: &str,
    ) -> Result<u64, crate::Error> {
        ApiCollectDao::confirm_onchain_transaction_fact(
            pool.write_ref(),
            trade_no,
            tx_hash,
            transaction_time,
            transaction_fee,
            resource_consume,
        )
        .await
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
        pool: &ApiFundsDbPool,
        trade_no: &str,
        tx_hash: &str,
        last_broadcast_at: &str,
        transaction_time: &str,
        transaction_fee: &str,
        resource_consume: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiCollectDao::confirm_onchain_transaction_fact_with_recover(
            pool.write_ref(),
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

    /// Repair-only: backfill tx_hash when local fact is missing but execution has progressed.
    ///
    /// Safety:
    /// - never overwrites non-empty tx_hash
    /// - requires transaction_time or last_broadcast_at to exist
    pub async fn backfill_tx_hash_if_missing(
        pool: &ApiFundsDbPool,
        trade_no: &str,
        tx_hash: &str,
        source: &str,
    ) -> Result<u64, crate::Error> {
        let normalized = tx_hash.trim();
        if normalized.is_empty() {
            return Err(crate::Error::Other("tx_hash must not be empty".to_string()));
        }

        let before = Self::get_api_collect_by_trade_no(pool, trade_no).await.ok();
        let rows =
            ApiCollectDao::backfill_tx_hash_if_missing(pool.write_ref(), trade_no, normalized)
                .await?;
        let after = if rows > 0 {
            Self::get_api_collect_by_trade_no(pool, trade_no).await.ok()
        } else {
            None
        };

        tracing::warn!(
            trade_no = %trade_no,
            source = %source,
            tx_hash = %normalized,
            rows_affected = %rows,
            before_tx_hash = ?before.as_ref().and_then(|r| r.tx_hash.as_ref()),
            before_last_broadcast_at_present = %before.as_ref().and_then(|r| r.last_broadcast_at.as_ref()).is_some(),
            before_transaction_time_present = %before.as_ref().and_then(|r| r.transaction_time.as_ref()).is_some(),
            after_tx_hash = ?after.as_ref().and_then(|r| r.tx_hash.as_ref()),
            after_last_broadcast_at_present = %after.as_ref().and_then(|r| r.last_broadcast_at.as_ref()).is_some(),
            after_transaction_time_present = %after.as_ref().and_then(|r| r.transaction_time.as_ref()).is_some(),
            "backfill_tx_hash_if_missing attempted"
        );

        if rows > 0 {
            Self::recompute_and_update_status(pool, trade_no).await?;
        }

        Ok(rows)
    }

    /// 标记 Result ACK 确认（推进事实）
    ///
    /// 语义：
    /// - 只能在 attempted 之后调用
    /// - 防止重复确认
    pub async fn mark_result_ack_confirmed(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiCollectDao::mark_result_ack_confirmed(pool.write_ref(), trade_no).await?;

        if rows > 0 {
            Self::recompute_and_update_status(pool, trade_no).await?;
        }

        Ok(rows)
    }

    /// 标记 Result ACK 已确认并标记链上终态（原子操作）
    ///
    /// 语义：
    /// - Result ACK 已成功发送到后端（result_ack_sent_at）
    /// - 同时标记链上终态（finished_at）
    /// - 单条 SQL 原子更新，防止 kill -9 产生"半完成事实"
    pub async fn mark_result_ack_confirmed_and_mark_chain_finished(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        let rows =
            ApiCollectDao::mark_result_ack_confirmed_and_chain_finished(pool.write_ref(), trade_no)
                .await?;

        if rows > 0 {
            Self::recompute_and_update_status(pool, trade_no).await?;
        }

        Ok(rows)
    }

    /// 标记ACK尝试，并设置终态
    pub async fn mark_result_ack_sent(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiCollectDao::mark_result_ack_sent(pool.write_ref(), trade_no).await?;

        if rows > 0 {
            Self::recompute_and_update_status(pool, trade_no).await?;
        }

        Ok(rows)
    }

    /// 标记手续费结果确认 ACK 已发送
    ///
    /// 语义：
    /// - 手续费结果确认 ACK 已成功发送到后端
    /// - 这是副作用完成的事实
    ///
    /// ⚠️ 调用约束：
    /// - 仅允许调用一次（tx_fee_res_ack_sent_at IS NULL）
    /// - 由 SideEffectWorker 调用
    pub async fn mark_tx_fee_res_ack_sent(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiCollectDao::mark_tx_fee_res_ack_sent(pool.write_ref(), trade_no).await?;

        if rows > 0 {
            Self::recompute_and_update_status(pool, trade_no).await?;
        }

        Ok(rows)
    }

    /// 标记服务费上传尝试（行为事实）
    ///
    /// 语义：
    /// - 只记录第一次尝试时间（COALESCE 幂等写）
    /// - 上传成功后不再变化（WHERE service_fee_uploaded_at IS NULL）
    /// - 这是"行为事实"，不是"推进事实"
    /// - 由 SideEffectWorker 调用
    pub async fn mark_service_fee_attempted(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiCollectDao::mark_service_fee_attempted(pool.write_ref(), trade_no).await
    }

    /// 标记服务费已上传
    ///
    /// 语义：
    /// - 服务费记录已成功上传到后端
    /// - 这是副作用完成的事实
    ///
    /// ⚠️ 调用约束：
    /// - 仅允许在服务费已上传的前提下调用
    /// - 仅允许调用一次（service_fee_uploaded_at IS NULL）
    /// - 由 SideEffectWorker 调用
    pub async fn mark_service_fee_uploaded(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiCollectDao::mark_service_fee_uploaded(pool.write_ref(), trade_no).await?;

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
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiCollectDao::mark_tx_exec_receipt_attempted(pool.write_ref(), trade_no).await
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
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiCollectDao::mark_tx_exec_receipt_uploaded(pool.write_ref(), trade_no).await?;

        if rows > 0 {
            Self::recompute_and_update_status(pool, trade_no).await?;
        }

        Ok(rows)
    }

    /// 扫描需要上传交易执行回执的交易
    ///
    /// 事实条件直接翻译：
    /// - last_broadcast_at IS NOT NULL：交易已成功广播
    /// - finished_at IS NULL：系统生命周期未结束
    /// - tx_exec_receipt_uploaded_at IS NULL：尚未上传执行回执
    pub async fn scan_need_tx_exec_receipt_upload(
        pool: &ApiFundsDbPool,
        limit: usize,
    ) -> Result<Vec<ApiCollectEntity>, crate::Error> {
        ApiCollectDao::scan_need_tx_exec_receipt_upload(pool.read_ref(), limit).await
    }

    /// 标记订单 ACK 尝试（行为事实）
    ///
    /// 语义：
    /// - 只记录第一次尝试时间（COALESCE 幂等写）
    /// - 发送成功后不再变化（WHERE order_ack_sent_at IS NULL）
    /// - 这是"行为事实"，不是"推进事实"
    /// - 由 SideEffectWorker 调用
    pub async fn mark_order_ack_attempted(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiCollectDao::mark_order_ack_attempted(pool.write_ref(), trade_no).await
    }

    /// 标记订单 ACK 已发送（推进事实）
    ///
    /// 语义：
    /// - 订单 ACK 已成功发送到后端
    /// - 这是副作用完成的事实
    ///
    /// ⚠️ 调用约束：
    /// - 仅允许在订单 ACK 已尝试的前提下调用
    /// - 仅允许调用一次（order_ack_sent_at IS NULL）
    /// - 由 SideEffectWorker 调用
    pub async fn mark_order_ack_sent(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiCollectDao::mark_order_ack_sent(pool.write_ref(), trade_no).await?;

        if rows > 0 {
            Self::recompute_and_update_status(pool, trade_no).await?;
        }

        Ok(rows)
    }

    /// 扫描需要发送订单 ACK 的交易
    ///
    /// 事实条件直接翻译：
    /// - order_ack_sent_at IS NULL：尚未发送订单 ACK
    ///
    /// ⚠️ 注意：
    /// - 不检查 order_ack_attempted_at（这是行为事实，不参与 Scanner 判断）
    /// - attempted 只用于 Worker / 运维观测
    pub async fn scan_need_order_ack(
        pool: &ApiFundsDbPool,
        limit: usize,
    ) -> Result<Vec<ApiCollectEntity>, crate::Error> {
        ApiCollectDao::scan_need_order_ack(pool.read_ref(), limit).await
    }

    /// 扫描需要恢复交易的记录
    ///
    /// 事实条件：
    /// - tx_hash IS NOT NULL
    /// - transaction_time IS NULL
    /// - last_broadcast_at IS NULL
    /// - tx_exec_receipt_uploaded_at IS NULL
    /// - finished_at IS NULL
    /// - err_code IS NULL
    ///
    /// ⚠️ 重要约束：
    /// - SQL必须100%等价于scanner中的need_recover predicate
    /// MUST be equivalent to scanner::need_recover()
    pub async fn scan_need_recover(
        pool: &ApiFundsDbPool,
        limit: usize,
    ) -> Result<Vec<ApiCollectEntity>, crate::Error> {
        ApiCollectDao::scan_need_recover(pool.read_ref(), limit).await
    }

    /// 扫描可能卡住的交易
    ///
    /// 事实条件：
    /// - finished_at IS NULL：系统生命周期未结束
    /// - err_code IS NULL：无错误
    /// - created_at < now() - interval '5 minutes'：至少等待 5 分钟
    /// - (order_ack_sent_at IS NOT NULL OR raw_tx IS NOT NULL OR last_broadcast_at IS NOT NULL)：有一定进展
    ///
    /// ⚠️ 重要约束：
    /// - 只返回可能卡住的交易
    /// - 使用 LIMIT 控制返回数量
    /// - ORDER BY created_at 优先处理 older 的交易
    pub async fn scan_possible_stuck(
        pool: &ApiFundsDbPool,
        limit: usize,
    ) -> Result<Vec<ApiCollectEntity>, crate::Error> {
        ApiCollectDao::scan_possible_stuck(pool.read_ref(), limit).await
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
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiCollectDao::mark_chain_finished(pool.write_ref(), trade_no).await?;

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
        pool: &ApiFundsDbPool,
        trade_no: &str,
        transaction_time: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiCollectDao::confirm_transaction_time_if_absent(
            pool.write_ref(),
            trade_no,
            transaction_time,
        )
        .await?;

        if rows > 0 {
            Self::recompute_and_update_status(pool, trade_no).await?;
        }

        Ok(rows)
    }

    /// 重新计算并更新状态
    ///
    /// ⚠️ Repo 写事实铁律
    /// - 任何 *_at / raw_tx / tx_hash 等“事实字段”的成功写入
    /// - 必须紧随 recompute_and_update_status
    ///
    /// ❌ 禁止在 Worker / Scanner / Dispatcher 中直接写 status
    /// ✅ status 只能由 Repo 根据事实统一推导
    pub async fn recompute_and_update_status(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<(), crate::Error> {
        let entity = Self::get_api_collect_by_trade_no(pool, trade_no).await?;

        let new_status = entity.recompute_status();

        if entity.status != new_status {
            ApiCollectDao::update_status(pool.write_ref(), trade_no, new_status).await?;

            tracing::info!(
                trade_no = %trade_no,
                old_status = ?entity.status,
                new_status = ?new_status,
                "collect status recomputed"
            );
        }

        Ok(())
    }

    /// 作废当前 raw_tx 及其 tx_hash
    ///
    /// ⚠️ 设计铁律：
    /// - 一旦 raw_tx 被判定为不可再广播 / 不可再构建（如手续费不足、前置条件变化）
    /// - 必须同时清空 tx_hash
    /// - 并写入 need_service_fee = true 事实
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
    pub async fn invalidate_raw_tx_need_service_fee(
        pool: &ApiFundsDbPool,
        trade_no: &str,
        status: Option<ApiCollectStatus>,
    ) -> Result<u64, crate::Error> {
        Self::invalidate_raw_tx(pool, trade_no, status).await
    }

    /// 作废当前 raw_tx/tx_hash，仅用于触发重建，不写 need_service_fee 事实。
    pub async fn invalidate_raw_tx_for_rebuild(
        pool: &ApiFundsDbPool,
        trade_no: &str,
        status: Option<ApiCollectStatus>,
    ) -> Result<u64, crate::Error> {
        let before = Self::get_api_collect_by_trade_no(pool, trade_no).await.ok();

        let rows = ApiCollectDao::invalidate_raw_tx_for_rebuild(pool.write_ref(), trade_no, status)
            .await?;

        let after = if rows > 0 {
            Self::get_api_collect_by_trade_no(pool, trade_no).await.ok()
        } else {
            None
        };

        tracing::warn!(
            trade_no = %trade_no,
            requested_status = ?status,
            rows_affected = %rows,
            before_need_service_fee = ?before.as_ref().and_then(|r| r.need_service_fee),
            before_service_fee_uploaded_at = ?before.as_ref().and_then(|r| r.service_fee_uploaded_at.as_ref()),
            before_tx_fee_res_ack_sent_at = ?before.as_ref().and_then(|r| r.tx_fee_res_ack_sent_at.as_ref()),
            before_raw_tx_present = %before.as_ref().and_then(|r| r.raw_tx.as_ref()).is_some(),
            before_tx_hash_present = %before.as_ref().and_then(|r| r.tx_hash.as_ref()).is_some(),
            before_last_broadcast_at_present = %before.as_ref().and_then(|r| r.last_broadcast_at.as_ref()).is_some(),
            before_transaction_time_present = %before.as_ref().and_then(|r| r.transaction_time.as_ref()).is_some(),
            before_status = ?before.as_ref().map(|r| &r.status),
            after_need_service_fee = ?after.as_ref().and_then(|r| r.need_service_fee),
            after_service_fee_uploaded_at = ?after.as_ref().and_then(|r| r.service_fee_uploaded_at.as_ref()),
            after_tx_fee_res_ack_sent_at = ?after.as_ref().and_then(|r| r.tx_fee_res_ack_sent_at.as_ref()),
            after_raw_tx_present = %after.as_ref().and_then(|r| r.raw_tx.as_ref()).is_some(),
            after_tx_hash_present = %after.as_ref().and_then(|r| r.tx_hash.as_ref()).is_some(),
            after_last_broadcast_at_present = %after.as_ref().and_then(|r| r.last_broadcast_at.as_ref()).is_some(),
            after_transaction_time_present = %after.as_ref().and_then(|r| r.transaction_time.as_ref()).is_some(),
            after_status = ?after.as_ref().map(|r| &r.status),
            "invalidate_raw_tx_for_rebuild applied (rebuild-only invalidation, fee facts preserved)"
        );

        Ok(rows)
    }

    pub async fn invalidate_raw_tx(
        pool: &ApiFundsDbPool,
        trade_no: &str,
        status: Option<ApiCollectStatus>,
    ) -> Result<u64, crate::Error> {
        let before = Self::get_api_collect_by_trade_no(pool, trade_no).await.ok();

        let rows =
            ApiCollectDao::invalidate_raw_tx_need_service_fee(pool.write_ref(), trade_no, status)
                .await?;

        let after = if rows > 0 {
            Self::get_api_collect_by_trade_no(pool, trade_no).await.ok()
        } else {
            None
        };

        tracing::warn!(
            trade_no = %trade_no,
            requested_status = ?status,
            rows_affected = %rows,
            before_need_service_fee = ?before.as_ref().and_then(|r| r.need_service_fee),
            before_service_fee_uploaded_at = ?before.as_ref().and_then(|r| r.service_fee_uploaded_at.as_ref()),
            before_tx_fee_res_ack_sent_at = ?before.as_ref().and_then(|r| r.tx_fee_res_ack_sent_at.as_ref()),
            before_raw_tx_present = %before.as_ref().and_then(|r| r.raw_tx.as_ref()).is_some(),
            before_tx_hash_present = %before.as_ref().and_then(|r| r.tx_hash.as_ref()).is_some(),
            before_last_broadcast_at_present = %before.as_ref().and_then(|r| r.last_broadcast_at.as_ref()).is_some(),
            before_transaction_time_present = %before.as_ref().and_then(|r| r.transaction_time.as_ref()).is_some(),
            before_status = ?before.as_ref().map(|r| &r.status),
            after_need_service_fee = ?after.as_ref().and_then(|r| r.need_service_fee),
            after_service_fee_uploaded_at = ?after.as_ref().and_then(|r| r.service_fee_uploaded_at.as_ref()),
            after_tx_fee_res_ack_sent_at = ?after.as_ref().and_then(|r| r.tx_fee_res_ack_sent_at.as_ref()),
            after_raw_tx_present = %after.as_ref().and_then(|r| r.raw_tx.as_ref()).is_some(),
            after_tx_hash_present = %after.as_ref().and_then(|r| r.tx_hash.as_ref()).is_some(),
            after_last_broadcast_at_present = %after.as_ref().and_then(|r| r.last_broadcast_at.as_ref()).is_some(),
            after_transaction_time_present = %after.as_ref().and_then(|r| r.transaction_time.as_ref()).is_some(),
            after_status = ?after.as_ref().map(|r| &r.status),
            "invalidate_raw_tx applied (fee cycle facts reset on reopen)"
        );

        Ok(rows)
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
    pub async fn resolve_need_service_fee(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiCollectDao::resolve_need_service_fee(pool.write_ref(), trade_no).await
    }

    /// 清除服务费需求标记（recover 专用）
    ///
    /// 语义：
    /// - 修复“手续费不足”这一事实，使交易重新具备构建条件
    /// - 不做任何状态回滚，不保证一定继续推进
    ///
    /// 调用场景：
    /// - 手续费问题已解决，需要重新构建交易
    pub async fn clear_need_service_fee(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiCollectDao::clear_need_service_fee(pool.write_ref(), trade_no).await?;
        tracing::info!(
            trade_no = %trade_no,
            rows_affected = %rows,
            "clear_need_service_fee applied"
        );
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::ApiCollectRepo;
    use crate::{
        dao::api_collect::ApiCollectDao,
        entities::api_collect::{ApiCollectStatus, CollectCreatedFact},
        error::Error,
        repositories::test_helper::setup_api_funds_pool,
    };

    #[tokio::test]
    async fn collect_upsert_and_get_success() {
        let pool = setup_api_funds_pool("wallet_db_collect_success").await;
        let trade_no = "collect_trade_success_1";
        let from_addr = "0xfrom_collect_s";
        let to_addr = "0xto_collect_s";

        ApiCollectRepo::upsert_api_collect(
            &pool,
            "u1",
            "collect_name",
            from_addr,
            to_addr,
            "100",
            "v",
            wallet_types::constant::chain_code::ETHEREUM,
            None,
            "ETH",
            trade_no,
            0,
            ApiCollectStatus::Init,
            0,
        )
        .await
        .unwrap();

        let got = ApiCollectRepo::get_api_collect_by_trade_no(&pool, trade_no).await.unwrap();
        assert_eq!(got.trade_no, trade_no);
        assert_eq!(got.from_addr, from_addr);
        assert_eq!(got.to_addr, to_addr);
        assert_eq!(got.symbol, "ETH");
        assert_eq!(got.value, "100");
        assert_eq!(got.status, ApiCollectStatus::Init);

        let (count, rows) =
            ApiCollectRepo::page_api_collect_with_status(&pool, 1, 20, &[ApiCollectStatus::Init])
                .await
                .unwrap();
        assert_eq!(count, 1);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].trade_no, trade_no);
    }

    #[tokio::test]
    async fn collect_missing_trade_no_returns_database_error() {
        let pool = setup_api_funds_pool("wallet_db_collect_edge").await;
        let err = ApiCollectRepo::get_api_collect_by_trade_no(&pool, "collect_missing_trade_no")
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Database(_)));
    }

    #[tokio::test]
    async fn collect_tx_rollback_keeps_db_unchanged() {
        let pool = setup_api_funds_pool("wallet_db_collect_rollback").await;
        let trade_no = "collect_trade_rollback_1";

        let mut tx = pool.write_ref().begin().await.unwrap();
        let fact = CollectCreatedFact {
            uid: Some("u2".to_string()),
            name: "collect_rb".to_string(),
            from_addr: "0xfrom_collect_rb".to_string(),
            to_addr: "0xto_collect_rb".to_string(),
            symbol: "ETH".to_string(),
            value: "88".to_string(),
            validate: "v".to_string(),
            chain_code: wallet_types::constant::chain_code::ETHEREUM.to_string(),
            token_addr: None,
            trade_no: trade_no.to_string(),
            trade_type: 0,
            risk_addr: "0".to_string(),
            status: ApiCollectStatus::Init,
        };
        ApiCollectDao::add(tx.as_mut(), fact).await.unwrap();
        tx.rollback().await.unwrap();

        let got = ApiCollectRepo::get_api_collect_by_trade_no(&pool, trade_no).await;
        assert!(matches!(got, Err(Error::Database(_))));

        let (count, rows) =
            ApiCollectRepo::page_api_collect_with_status(&pool, 1, 20, &[ApiCollectStatus::Init])
                .await
                .unwrap();
        assert_eq!(count, 0);
        assert!(rows.is_empty());
    }
}
