// ======================= 强顺序保证说明 =======================
// 本文件是 Withdraw 顺序链中的关键实现：
// TxAck -> BuildTx -> Broadcast -> TxExecReceipt -> TxResAck
//
// ⚠️ 禁止修改以下事实依赖：
// - scan_can_build 必须依赖 tx_ack_sent_at
// - scan_confirmed_need_tx_res_ack 必须依赖 tx_exec_receipt_uploaded_at
//
// 修改这些条件将破坏系统的强顺序与 crash-safe 特性。
// =============================================================

// ======================= 系统不变量 =======================
// 1. SideEffectWorker 100% 无事实修改能力
// 2. Shadow / Scanner 只负责推进，不负责判断对错
// 3. 所有副作用必须可重复执行（at-least-once）
// =========================================================

// ❗️当前系统不变量（可安全依赖）：
// - Withdraw 交易目前**只可能**因手续费不足或地址错误被设置为失败
// - 因此 invalidate_raw_tx 主要用于处理手续费不足和地址错误问题
//
// ❗️若未来引入其他失败原因，
// 必须：
// 1. 拆分 invalidate 方法为明确语义的多个方法
// 2. 或在 SQL 中增加明确的 reason 约束

use crate::{
    ApiFundsDbPool,
    dao::api_withdraw::ApiWithdrawDao,
    entities::{
        api_trade_type::ApiTradeType,
        api_withdraw::{
            ApiWithdrawEntity, ApiWithdrawStatus, ErrCode, WithdrawCreatedFact,
            WithdrawFailureStage,
        },
    },
    pagination::Pagination,
};

pub struct ApiWithdrawRepo;

impl ApiWithdrawRepo {
    pub async fn list_api_withdraw(
        pool: &ApiFundsDbPool,
        uid: &str,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::all_api_withdraw(pool.read_ref(), uid).await
    }

    pub async fn list_api_withdraw_with_status(
        pool: &ApiFundsDbPool,
        status: Vec<ApiWithdrawStatus>,
        page: i64,
        page_size: i64,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::list_api_withdraw_with_status(pool.read_ref(), status, page, page_size)
            .await
    }

    pub async fn page_api_withdraw(
        pool: &ApiFundsDbPool,
        uid: &str,
        status: Vec<ApiWithdrawStatus>,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::page_api_withdraw(pool.read_ref(), uid, status, page, page_size).await
    }

    pub async fn page_api_withdraw_with_init_status(
        pool: &ApiFundsDbPool,
        uid: &str,
        init_status: ApiWithdrawStatus,
        status: Vec<ApiWithdrawStatus>,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::page_api_withdraw_with_init_status(
            pool.read_ref(),
            uid,
            init_status,
            status,
            page,
            page_size,
        )
        .await
    }

    pub async fn get_api_withdraw_by_id(
        pool: &ApiFundsDbPool,
        id: &str,
    ) -> Result<ApiWithdrawEntity, crate::Error> {
        ApiWithdrawDao::get_api_withdraw_by_id(pool.read_ref(), id).await
    }

    pub async fn get_api_withdraw_by_trade_no(
        pool: &ApiFundsDbPool,
        trade_no: &str,
        trade_type: ApiTradeType,
    ) -> Result<ApiWithdrawEntity, crate::Error> {
        ApiWithdrawDao::get_api_withdraw_by_trade_no(pool.read_ref(), trade_no, trade_type).await
    }

    pub async fn get_api_withdraw_by_trade_no_status(
        pool: &ApiFundsDbPool,
        trade_no: &str,
        vec_status: &[ApiWithdrawStatus],
    ) -> Result<ApiWithdrawEntity, crate::Error> {
        ApiWithdrawDao::get_api_withdraw_by_trade_no_status(pool.read_ref(), trade_no, vec_status)
            .await
    }

    /// Runtime repair helper: query withdraw candidates from acct_change facts.
    ///
    /// Caller must still perform Rust-side filtering (amount/time window/uniqueness)
    /// and conflict checks before calling backfill.
    pub async fn find_candidates_for_acct_change_hash_backfill(
        pool: &ApiFundsDbPool,
        chain_code: &str,
        from_addr: &str,
        to_addr: &str,
        token_addr: Option<&str>,
        symbol: &str,
        limit: i64,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::find_candidates_for_acct_change_hash_backfill(
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

    pub async fn get_by_hash_and_owner(
        pool: &ApiFundsDbPool,
        owner: &str,
        tx_hash: &str,
    ) -> Result<ApiWithdrawEntity, crate::Error> {
        ApiWithdrawDao::get_by_hash_and_owner(pool.read_ref(), owner, tx_hash).await
    }

    pub async fn lists_by_hashs(
        pool: &ApiFundsDbPool,
        owner: &str,
        hashs: Vec<String>,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::lists_by_hashs(pool.read_ref(), owner, hashs).await
    }

    pub async fn recent_bill(
        pool: &ApiFundsDbPool,
        token: &str,
        from_addr: &str,
        chain_code: &str,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<ApiWithdrawEntity>, crate::Error> {
        let lists = ApiWithdrawDao::recent_bill(
            pool.read_ref(),
            token,
            from_addr,
            chain_code,
            page,
            page_size,
        )
        .await?;
        Ok(lists)
    }

    pub async fn bill_lists(
        pool: &ApiFundsDbPool,
        uid: &str,
        addr: &[String],
        chain_code: Option<&str>,
        symbol: Option<&str>,
        is_multisig: Option<i64>,
        min_value: Option<f64>,
        start: Option<i64>,
        end: Option<i64>,
        transfer_type: Vec<i32>,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<ApiWithdrawEntity>, crate::Error> {
        let lists = ApiWithdrawDao::bill_lists(
            pool.read_ref(),
            uid,
            addr,
            chain_code,
            symbol,
            is_multisig,
            min_value,
            start,
            end,
            transfer_type,
            page,
            page_size,
        )
        .await?;
        Ok(lists)
    }

    pub async fn upsert_api_withdraw(
        pool: &ApiFundsDbPool,
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
        trade_type: ApiTradeType,
        nonce: i64,
        tx_hash: Option<String>,
        init_status: ApiWithdrawStatus,
        status: ApiWithdrawStatus,
        resource_consume: &str,
        transaction_fee: &str,
        transaction_time: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
        block_height: Option<String>,
    ) -> Result<(), crate::Error> {
        let withdraw_req = ApiWithdrawEntity {
            id: 0,
            name: name.to_string(),
            uid: uid.to_string(),
            from_addr: from_addr.to_string(),
            to_addr: to_addr.to_string(),
            value: value.to_string(),
            validate: validate.to_string(),
            chain_code: chain_code.to_string(),
            token_addr: token_addr.into(),
            symbol: symbol.to_string(),
            trade_no: trade_no.to_string(),
            trade_type,
            init_status,
            status,
            nonce,
            tx_hash,
            raw_tx: None,
            resource_consume: resource_consume.to_string(),
            transaction_fee: transaction_fee.to_string(),
            transaction_time,
            block_height,
            notes: None,
            post_tx_count: 0,
            post_confirm_tx_count: 0,
            err_code: None,
            err_msg: None,
            created_at: Default::default(),
            updated_at: None,
            tx_ack_sent_at: None,
            tx_res_ack_sent_at: None,
            tx_res_received_at: None,
            tx_ack_attempted_at: None,
            building_at: None,
            last_broadcast_at: None,
            broadcast_uncertain_since_at: None,
            broadcast_uncertain_retry_count: 0,
            broadcast_uncertain_last_checked_at: None,
            broadcast_uncertain_reconciled_at: None,
            broadcast_uncertain_rebroadcast_count: 0,
            tx_res_ack_attempted_at: None,
            tx_exec_receipt_attempted_at: None,
            tx_exec_receipt_uploaded_at: None,
            finished_at: None,
            audit_passed_at: None,
            audit_rejected_at: None,
            audit_reason: None,
            chain_success_at: None,
            chain_failed_at: None,
            failure_stage: None,
        };
        ApiWithdrawDao::upsert(pool.write_ref(), withdraw_req).await
    }

    /// 保留原签名，确保兼容性
    pub async fn upsert_api_withdraw_with_fact(
        pool: &ApiFundsDbPool,
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
        trade_type: ApiTradeType,
    ) -> Result<(), crate::Error> {
        let withdraw_req = WithdrawCreatedFact {
            uid: Some(uid.to_string()),
            name: name.to_string(),
            from_addr: from_addr.to_string(),
            to_addr: to_addr.to_string(),
            symbol: symbol.to_string(),
            value: value.to_string(),
            validate: validate.to_string(),
            chain_code: chain_code.to_string(),
            token_addr: token_addr.into(),
            trade_no: trade_no.to_string(),
            trade_type: trade_type as i64,
            status: ApiWithdrawStatus::Init,
        };
        ApiWithdrawDao::add(pool.write_ref(), withdraw_req).await
    }

    pub async fn update_api_fee_post_tx_count(
        pool: &ApiFundsDbPool,
        trade_no: &str,
        status: ApiWithdrawStatus,
    ) -> Result<(), crate::Error> {
        ApiWithdrawDao::update_post_tx_count(pool.write_ref(), trade_no, status).await
    }

    pub async fn update_api_withdraw_tx_status(
        pool: &ApiFundsDbPool,
        trade_no: &str,
        nonce: i64,
        tx_hash: &str,
        resource_consume: &str,
        transaction_fee: &str,
        transaction_time: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
        block_height: &str,
        status: ApiWithdrawStatus,
    ) -> Result<u64, crate::Error> {
        ApiWithdrawDao::update_tx_status(
            pool.write_ref(),
            trade_no,
            nonce,
            tx_hash,
            resource_consume,
            transaction_fee,
            transaction_time,
            block_height,
            status,
        )
        .await
    }

    pub async fn update_api_withdraw_tx_status_nonce(
        pool: &ApiFundsDbPool,
        from_addr: &str,
        chain_code: &str,
        trade_no: &str,
        nonce: i64,
        tx_hash: &str,
        resource_consume: &str,
        transaction_fee: &str,
        status: ApiWithdrawStatus,
    ) -> Result<u64, crate::Error> {
        let _write_guard = pool.lock_write_with_metric("update_api_withdraw_tx_status_nonce").await;
        let tx_start = std::time::Instant::now();
        let result = ApiWithdrawDao::update_tx_status_nonce(
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
        .await;
        let elapsed_ms = tx_start.elapsed().as_secs_f64() * 1000.0;
        tracing::info!(
            metric = "write_tx_duration_ms",
            db = "api_funds.db",
            op = "update_api_withdraw_tx_status_nonce",
            value_ms = %elapsed_ms,
            ok = %result.is_ok(),
            "withdraw write finished"
        );
        result
    }

    pub async fn update_api_withdraw_tx(
        pool: &ApiFundsDbPool,
        trade_no: &str,
        resource_consume: &str,
        transaction_fee: &str,
        transaction_time: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
        block_height: &str,
    ) -> Result<(), crate::Error> {
        ApiWithdrawDao::update_tx(
            pool.write_ref(),
            trade_no,
            resource_consume,
            transaction_fee,
            transaction_time,
            block_height,
        )
        .await
    }

    #[deprecated(
        since = "0.1.0",
        note = "Legacy state-machine API. Do not use in fact-driven system. This will be removed in future versions."
    )]
    pub async fn update_api_withdraw_status_and_err(
        pool: &ApiFundsDbPool,
        trade_no: &str,
        status: ApiWithdrawStatus,
        err_code: ErrCode,
        err_msg: &str,
    ) -> Result<u64, crate::Error> {
        ApiWithdrawDao::update_status_and_err(pool.write_ref(), trade_no, status, err_code, err_msg)
            .await
    }

    #[deprecated(
        since = "0.1.0",
        note = "LEGACY STATE MACHINE API. Do not use in Shadow / Scanner / fact-driven paths. Use fact-based APIs instead."
    )]
    pub async fn update_api_withdraw_status(
        pool: &ApiFundsDbPool,
        trade_no: &str,
        status: ApiWithdrawStatus,
    ) -> Result<u64, crate::Error> {
        ApiWithdrawDao::update_status(pool.write_ref(), trade_no, status).await
    }

    #[deprecated(
        since = "0.1.0",
        note = "LEGACY STATE MACHINE API. Do not use in Shadow / Scanner / fact-driven paths. Use fact-based APIs instead."
    )]
    pub async fn update_api_withdraw_next_status(
        pool: &ApiFundsDbPool,
        trade_no: &str,
        status: ApiWithdrawStatus,
        next_status: ApiWithdrawStatus,
    ) -> Result<u64, crate::Error> {
        ApiWithdrawDao::update_next_status(pool.write_ref(), trade_no, status, next_status).await
    }

    #[deprecated(
        since = "0.1.0",
        note = "LEGACY STATE MACHINE API. Do not use in Shadow / Scanner / fact-driven paths. Use fact-based APIs instead."
    )]
    pub async fn update_api_withdraw_post_tx_count(
        pool: &ApiFundsDbPool,
        trade_no: &str,
        status: ApiWithdrawStatus,
    ) -> Result<(), crate::Error> {
        ApiWithdrawDao::update_post_tx_count(pool.write_ref(), trade_no, status).await
    }

    #[deprecated(
        since = "0.1.0",
        note = "LEGACY STATE MACHINE API. Do not use in Shadow / Scanner / fact-driven paths. Use fact-based APIs instead."
    )]
    pub async fn update_api_withdraw_post_confirm_tx_count(
        pool: &ApiFundsDbPool,
        trade_no: &str,
        status: ApiWithdrawStatus,
    ) -> Result<(), crate::Error> {
        ApiWithdrawDao::update_post_confirm_tx_count(pool.write_ref(), trade_no, status).await
    }

    #[deprecated(since = "0.1.0", note = "LEGACY API. Use mark_tx_ack_sent instead.")]
    /// 设置 Tx ACK 发送时间
    pub async fn set_tx_ack_sent(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<(), crate::Error> {
        ApiWithdrawDao::mark_tx_ack_sent(pool.write_ref(), trade_no).await.map(|_| ())
    }

    #[deprecated(since = "0.1.0", note = "LEGACY API. Use mark_tx_res_ack_sent instead.")]
    /// 设置 TxRes ACK 发送时间
    pub async fn set_tx_res_ack_sent(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<(), crate::Error> {
        ApiWithdrawDao::mark_tx_res_ack_sent(pool.write_ref(), trade_no).await.map(|_| ())
    }

    /// 获取 ACK 发送时间
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
        ApiWithdrawDao::get_ack_times(pool.read_ref(), trade_no).await
    }

    /// 扫描需要发送交易结果 ACK 的交易
    pub async fn scan_confirmed_need_tx_res_ack(
        pool: &ApiFundsDbPool,
        limit: usize,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::scan_need_tx_res_ack(pool.read_ref(), limit).await
    }

    /// 周期性卡单预筛选：扫描“可能卡住”的交易（低成本）
    pub async fn scan_possible_stuck(
        pool: &ApiFundsDbPool,
        limit: usize,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::scan_possible_stuck(pool.read_ref(), limit).await
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
        pool: &ApiFundsDbPool,
        limit: usize,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::scan_need_tx_ack(pool.read_ref(), limit).await
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
    pub async fn scan_need_recover(
        pool: &ApiFundsDbPool,
        limit: usize,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::scan_need_recover(pool.read_ref(), limit).await
    }

    // ============================================================================
    // STRONG ORDER GATE — BuildTx 不可逆事实屏障
    // ============================================================================
    //
    // BuildTx 只能在以下事实全部满足时发生：
    //
    // [FACT REQUIRED]
    // ✔ tx_ack_sent_at IS NOT NULL   — 后端确认已发送
    // ✔ audit_passed_at IS NOT NULL  — 审计通过（强顺序屏障）
    //
    // [FACT MUST NOT EXIST]
    // ✘ raw_tx IS NOT NULL           — 防止重复构建
    // ✘ finished_at IS NOT NULL      — 已终态
    // ✘ err_code IS NOT NULL         — 终止错误
    //
    // ⚠️ DO NOT REMOVE ANY CONDITION
    // ⚠️ Scanner 与 DAO predicate 必须完全一致
    // ============================================================================
    // ============================================================================
    // ⚠️ Scanner / DAO Predicate Symmetry Rule
    //
    // 本方法必须与以下两者保持一致：
    // 1. ApiWithdrawDao::scan_can_build
    // 2. wallet_api::infrastructure::withdraw::shadow::scanner::can_build
    //
    // 修改任一侧时必须同步修改另一侧，否则会导致：
    // - Phantom Task
    // - Double Build
    // - 永久卡死
    //
    // Repository 是 DAO 的代理，Scanner 是安全网
    // ============================================================================
    pub async fn scan_can_build(
        pool: &ApiFundsDbPool,
        limit: usize,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::scan_can_build(pool.read_ref(), limit).await
    }

    /// 扫描可广播的交易
    ///
    /// ⚠️ 核心事实驱动原则：
    /// - 只基于不可逆事实字段(raw_tx, transaction_time)决策
    /// - 不依赖时间字段(last_broadcast_at)进行决策
    /// - 并发通过transaction_time写入唯一性保证
    pub async fn scan_can_broadcast(
        pool: &ApiFundsDbPool,
        limit: usize,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::scan_can_broadcast(pool.read_ref(), limit).await
    }

    /// 扫描需要上传交易执行回执的交易
    ///
    /// 事实条件直接翻译：
    /// - tx_exec_receipt_uploaded_at IS NULL：尚未上传执行回执
    /// - finished_at IS NULL：系统生命周期未结束
    ///
    /// ⚠️ 重要说明：
    /// - 即使没有 tx_hash（未广播），如果发生错误也需要上传回执
    /// - 本扫描在 err_code != NULL 时仍然允许
    /// - 因为 UploadTxExecReceipt 属于【行为事实补齐副作用】
    /// - 不属于推进，不受 err_code 冻结
    pub async fn scan_need_tx_exec_receipt_upload(
        pool: &ApiFundsDbPool,
        limit: usize,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::scan_need_tx_exec_receipt_upload(pool.read_ref(), limit).await
    }

    /// 扫描需要发送交易结果 ACK 的交易
    ///
    /// 事实条件直接翻译：
    /// - tx_exec_receipt_uploaded_at IS NOT NULL：交易执行回执已上传
    /// - finished_at IS NULL：系统生命周期未结束
    /// - tx_res_ack_sent_at IS NULL：尚未发送交易结果 ACK（推进事实）
    ///
    /// ⚠️ 强顺序屏障：
    /// - TxResAck 必须发生在 TxExecReceipt 上传之后
    /// - 禁止使用 transaction_time 作为前置条件（共享前提事实）
    ///
    /// ⚠️ 注意：
    /// - 不检查 tx_res_ack_attempted_at（这是行为事实，不参与 Scanner 判断）
    /// - attempted 只用于 Worker / 运维观测
    pub async fn scan_need_tx_res_ack(
        pool: &ApiFundsDbPool,
        limit: usize,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::scan_need_tx_res_ack(pool.read_ref(), limit).await
    }

    /// 更新building_at时间
    pub async fn update_building_at(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiWithdrawDao::update_building_at(pool.write_ref(), trade_no).await
    }

    /// 更新last_broadcast_at时间
    pub async fn update_last_broadcast_at(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiWithdrawDao::update_last_broadcast_at(pool.write_ref(), trade_no).await
    }

    /// 构建交易后更新
    pub async fn update_after_build(
        pool: &ApiFundsDbPool,
        trade_no: &str,
        tx_hash: &str,
        raw_tx: &str,
        transaction_fee: &str,
        nonce: i64,
    ) -> Result<u64, crate::Error> {
        let rows = ApiWithdrawDao::update_after_build(
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

    /// 标记广播已执行
    pub async fn mark_broadcast_executed(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiWithdrawDao::mark_broadcast_executed(pool.write_ref(), trade_no).await?;

        if rows > 0 {
            Self::recompute_and_update_status(pool, trade_no).await?;
        }

        Ok(rows)
    }

    pub async fn mark_broadcast_uncertain_attempt(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiWithdrawDao::mark_broadcast_uncertain_attempt(pool.write_ref(), trade_no).await
    }

    pub async fn mark_broadcast_uncertain_reconciled(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiWithdrawDao::mark_broadcast_uncertain_reconciled(pool.write_ref(), trade_no).await
    }

    pub async fn mark_broadcast_uncertain_rebroadcast_attempted(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiWithdrawDao::mark_broadcast_uncertain_rebroadcast_attempted(pool.write_ref(), trade_no)
            .await
    }

    pub async fn clear_broadcast_uncertain_tracking(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiWithdrawDao::clear_broadcast_uncertain_tracking(pool.write_ref(), trade_no).await
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
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiWithdrawDao::update_tx_res_received_at(pool.write_ref(), trade_no).await
    }

    /// 标记交易 ACK 尝试（行为事实）
    ///
    /// 语义：
    /// - 只记录第一次尝试时间（COALESCE 幂等写）
    /// - 发送成功后不再变化（WHERE tx_ack_sent_at IS NULL）
    /// - 这是"行为事实"，不是"推进事实"
    /// - 由 SideEffectWorker 调用
    pub async fn mark_tx_ack_attempted(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiWithdrawDao::mark_tx_ack_attempted(pool.write_ref(), trade_no).await
    }

    /// 标记交易 ACK 已发送（推进事实）
    ///
    /// 语义：
    /// - 交易 ACK 已成功发送到后端
    /// - 这是副作用完成的事实
    /// - 不参与状态推导
    ///
    /// ⚠️ 调用约束：
    /// - 仅允许在交易 ACK 已尝试的前提下调用
    /// - 仅允许调用一次（tx_ack_sent_at IS NULL）
    /// - 由 SideEffectWorker 调用
    pub async fn mark_tx_ack_sent(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiWithdrawDao::mark_tx_ack_sent(pool.write_ref(), trade_no).await?;
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
        ApiWithdrawDao::mark_tx_exec_receipt_attempted(pool.write_ref(), trade_no).await
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
        let rows =
            ApiWithdrawDao::mark_tx_exec_receipt_uploaded(pool.write_ref(), trade_no).await?;

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
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiWithdrawDao::mark_tx_res_ack_attempted(pool.write_ref(), trade_no).await
    }

    /// 标记交易结果 ACK 已发送
    ///
    /// 语义：
    /// - 交易结果 ACK 已成功发送到后端
    /// - 这是副作用完成的事实
    /// - 不参与状态推导
    ///
    /// ⚠️ 调用约束：
    /// - 仅允许在交易结果 ACK 已尝试的前提下调用
    /// - 仅允许调用一次（tx_res_ack_sent_at IS NULL）
    /// - 由 SideEffectWorker 调用
    pub async fn mark_tx_res_ack_sent(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiWithdrawDao::mark_tx_res_ack_sent(pool.write_ref(), trade_no).await?;
        Ok(rows)
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
        let rows = ApiWithdrawDao::confirm_onchain_transaction_fact(
            pool.write_ref(),
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

    /// Repair-only: backfill tx_hash when local fact is missing but execution has progressed.
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

        let before =
            Self::get_api_withdraw_by_trade_no(pool, trade_no, ApiTradeType::Withdraw).await.ok();
        let rows =
            ApiWithdrawDao::backfill_tx_hash_if_missing(pool.write_ref(), trade_no, normalized)
                .await?;
        let after = if rows > 0 {
            Self::get_api_withdraw_by_trade_no(pool, trade_no, ApiTradeType::Withdraw).await.ok()
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
            before_chain_success_at_present = %before.as_ref().and_then(|r| r.chain_success_at.as_ref()).is_some(),
            after_tx_hash = ?after.as_ref().and_then(|r| r.tx_hash.as_ref()),
            after_last_broadcast_at_present = %after.as_ref().and_then(|r| r.last_broadcast_at.as_ref()).is_some(),
            after_transaction_time_present = %after.as_ref().and_then(|r| r.transaction_time.as_ref()).is_some(),
            after_chain_success_at_present = %after.as_ref().and_then(|r| r.chain_success_at.as_ref()).is_some(),
            "withdraw backfill_tx_hash_if_missing attempted"
        );

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
        pool: &ApiFundsDbPool,
        trade_no: &str,
        tx_hash: &str,
        last_broadcast_at: &str,
        transaction_time: &str,
        transaction_fee: &str,
        resource_consume: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiWithdrawDao::confirm_onchain_transaction_fact_with_recover(
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

    /// 更新交易状态和错误信息
    pub async fn update_status_and_err(
        pool: &ApiFundsDbPool,
        trade_no: &str,
        status: ApiWithdrawStatus,
        err_code: ErrCode,
        err_msg: &str,
    ) -> Result<u64, crate::Error> {
        ApiWithdrawDao::update_status_and_err(pool.write_ref(), trade_no, status, err_code, err_msg)
            .await
    }

    /// 重新计算并更新交易状态
    ///
    /// ⚠️ Repo 写事实铁律
    /// - 任何 *_at / raw_tx / tx_hash 等"事实字段"的成功写入
    /// - 必须紧随 recompute_and_update_status
    ///
    /// ❌ 禁止在 Worker / Scanner / Dispatcher 中直接写 status
    /// ✅ status 只能由 Repo 根据事实统一推导
    ///
    /// ⚠️ 状态推导铁律
    /// - Report 状态 > 一切非 Report 状态
    /// - 一旦进入 Report 空间，recompute 永远不能回到非 Report
    /// - TxExecReceipt / TxResAck 是业务流程状态推进事实，不是纯 SideEffect
    async fn recompute_and_update_status(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<(), crate::Error> {
        let entity =
            Self::get_api_withdraw_by_trade_no(pool, trade_no, ApiTradeType::Withdraw).await?;

        // Step 1: 纯事实推导（无 guard）
        let new_status = Self::derive_status(&entity);

        // Step 2: 单调 Guard
        if !Self::monotonic_allow(entity.status, new_status) {
            let old_layer = Self::layer(entity.status);
            let new_layer = Self::layer(new_status);
            tracing::warn!(
                trade_no = %entity.trade_no,
                old_status = ?entity.status,
                new_status = ?new_status,
                old_layer = old_layer,
                new_layer = new_layer,
                "Monotonic guard prevented status regression"
            );
            return Ok(());
        }

        // Step 3: 更新状态
        if entity.status != new_status {
            tracing::debug!(
                trade_no = %entity.trade_no,
                old_status = ?entity.status,
                new_status = ?new_status,
                chain_success = ?entity.chain_success_at.is_some(),
                chain_failed = ?entity.chain_failed_at.is_some(),
                failure_stage = ?entity.failure_stage,
                report_flags = ?Self::report_trigger(&entity),
                "Withdraw status recomputed"
            );
            ApiWithdrawDao::update_status(pool.write_ref(), trade_no, new_status).await?;
        }

        Ok(())
    }

    /// 纯事实推导函数
    /// 只基于事实字段推导状态，不考虑旧状态
    fn derive_status(entity: &ApiWithdrawEntity) -> ApiWithdrawStatus {
        let has_failure_stage = matches!(
            entity.failure_stage,
            Some(
                WithdrawFailureStage::Build
                    | WithdrawFailureStage::Broadcast
                    | WithdrawFailureStage::Chain
                    | WithdrawFailureStage::TxResultAck
            )
        );

        // 检测 audit/chain 互斥
        let audit_chain_conflict = entity.audit_rejected_at.is_some()
            && (entity.chain_success_at.is_some() || entity.chain_failed_at.is_some());

        // 开发阶段断言
        debug_assert!(
            !audit_chain_conflict,
            "Audit reject and chain facts cannot coexist for trade_no: {}",
            entity.trade_no
        );

        // 生产阶段错误日志
        if audit_chain_conflict {
            // 这里可以实现 warn_once_per_trade_no，避免日志刷爆
            tracing::error!(
                trade_no = %entity.trade_no,
                invariant = "audit_chain_conflict",
                "Audit reject and chain facts conflict detected"
            );
        }

        // 审核拒绝为永久压制链结果
        if entity.audit_rejected_at.is_some() {
            return ApiWithdrawStatus::AuditReject;
        }

        // Report阶段
        if Self::report_trigger(entity)
            && (entity.chain_success_at.is_some()
                || entity.chain_failed_at.is_some()
                || has_failure_stage)
        {
            // Invariant: Report 空间必须有明确的链结果或发送失败事实
            debug_assert!(
                entity.chain_success_at.is_some()
                    || entity.chain_failed_at.is_some()
                    || has_failure_stage
            );

            if entity.chain_success_at.is_some() {
                return ApiWithdrawStatus::ConfirmSuccessReport;
            }
            if entity.chain_failed_at.is_some() {
                return ApiWithdrawStatus::ConfirmFailureReport;
            }
            if has_failure_stage {
                return ApiWithdrawStatus::SendingTxFailedReport;
            }
        }

        // Result阶段
        if entity.chain_failed_at.is_some() {
            return ApiWithdrawStatus::Failure;
        }
        if entity.chain_success_at.is_some() {
            return ApiWithdrawStatus::Success;
        }
        if has_failure_stage {
            return ApiWithdrawStatus::SendingTxFailed;
        }

        // Flow阶段
        if entity.last_broadcast_at.is_some() {
            return ApiWithdrawStatus::SendingTx;
        }

        if entity.audit_passed_at.is_some() {
            return ApiWithdrawStatus::AuditPass;
        }

        // 默认初始态
        ApiWithdrawStatus::Init
    }

    /// Report触发条件
    fn report_trigger(entity: &ApiWithdrawEntity) -> bool {
        entity.tx_exec_receipt_uploaded_at.is_some() || entity.tx_res_ack_sent_at.is_some()
    }

    /// 单调保护函数
    /// 确保状态只能向前推进，不能回退
    fn monotonic_allow(old_status: ApiWithdrawStatus, new_status: ApiWithdrawStatus) -> bool {
        let old_layer = Self::layer(old_status);
        let new_layer = Self::layer(new_status);

        if new_layer < old_layer {
            return false;
        }

        if new_layer == old_layer {
            let old_rank = Self::rank(old_status);
            let new_rank = Self::rank(new_status);
            if new_rank < old_rank {
                return false;
            }
        }

        true
    }

    /// 最大已知 layer 值，用于防止未来与默认值冲突
    const MAX_KNOWN_LAYER: u8 = 32;

    /// 状态层
    /// 用于跨层保护
    fn layer(status: ApiWithdrawStatus) -> u8 {
        let layer = match status {
            // InitOrder 是历史遗留的“更早初始态”（-1），用于兼容旧逻辑
            // 语义上应视为最早层，禁止从 Init/Flow 回退到 InitOrder
            ApiWithdrawStatus::InitOrder => 0, // Pre-Flow
            ApiWithdrawStatus::Init
            | ApiWithdrawStatus::AuditPass
            | ApiWithdrawStatus::SendingTx => 1, // Flow
            ApiWithdrawStatus::Success
            | ApiWithdrawStatus::Failure
            | ApiWithdrawStatus::AuditReject
            | ApiWithdrawStatus::SendingTxFailed => 2, // Result
            ApiWithdrawStatus::SendingTxFailedReport
            | ApiWithdrawStatus::ConfirmFailureReport
            | ApiWithdrawStatus::ConfirmSuccessReport => 3, // Report
            #[cfg(debug_assertions)]
            _ => unreachable!("Unknown ApiWithdrawStatus: {:?}", status),
            #[cfg(not(debug_assertions))]
            _ => {
                tracing::error!(status = ?status, "Unknown ApiWithdrawStatus detected in layer function");
                u8::MAX
            }
        };

        // 确保所有合法 layer < MAX_KNOWN_LAYER
        debug_assert!(
            layer < Self::MAX_KNOWN_LAYER,
            "Layer value {} exceeds MAX_KNOWN_LAYER {}",
            layer,
            Self::MAX_KNOWN_LAYER
        );

        layer
    }

    /// 状态秩
    /// 用于层内保护
    fn rank(status: ApiWithdrawStatus) -> u8 {
        match status {
            // Result 层
            ApiWithdrawStatus::Failure => 1,
            ApiWithdrawStatus::AuditReject => 1, // AuditReject 与 Failure 同 rank，由 derive 负责语义
            ApiWithdrawStatus::Success => 2,
            // Report 层
            ApiWithdrawStatus::SendingTxFailedReport => 1,
            ApiWithdrawStatus::ConfirmFailureReport => 2,
            ApiWithdrawStatus::ConfirmSuccessReport => 3,
            _ => 0,
        }
    }

    /// 标记链上终态
    ///
    /// 语义：
    /// - 交易已在链上达到终态（成功或失败）
    /// - 所有必要的副作用已完成
    /// - 这是一个不可逆的事实
    pub async fn mark_chain_finished(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiWithdrawDao::mark_chain_finished(pool.write_ref(), trade_no).await?;

        if rows > 0 {
            Self::recompute_and_update_status(pool, trade_no).await?;
        }

        Ok(rows)
    }

    /// 标记交易结果 ACK 已发送并标记链上终态
    ///
    /// 语义：
    /// - 交易结果 ACK 已成功发送到后端
    /// - 同时标记链上终态
    /// - 这是一个原子操作，确保两个更新要么都成功，要么都失败
    pub async fn mark_tx_res_ack_sent_and_chain_finished(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        let rows =
            ApiWithdrawDao::mark_tx_res_ack_sent_and_chain_finished(pool.write_ref(), trade_no)
                .await?;

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
        pool: &ApiFundsDbPool,
        trade_no: &str,
        transaction_time: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiWithdrawDao::confirm_transaction_time_if_absent(
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

    /// 设置审核通过事实
    ///
    /// 语义：
    /// - 标记审核通过
    /// - 清空审核拒绝事实（互斥）
    /// - 幂等
    pub async fn set_audit_passed(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiWithdrawDao::set_audit_passed(pool.write_ref(), trade_no).await?;

        if rows > 0 {
            Self::recompute_and_update_status(pool, trade_no).await?;
        }

        Ok(rows)
    }

    /// 设置审核拒绝事实
    ///
    /// 语义：
    /// - 标记审核拒绝
    /// - 清空审核通过事实（互斥）
    /// - 幂等
    pub async fn set_audit_rejected(
        pool: &ApiFundsDbPool,
        trade_no: &str,
        reason: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiWithdrawDao::set_audit_rejected(pool.write_ref(), trade_no, reason).await?;

        if rows > 0 {
            Self::recompute_and_update_status(pool, trade_no).await?;
        }

        Ok(rows)
    }

    /// 设置链成功事实
    ///
    /// 语义：
    /// - 标记链上执行成功
    /// - 清空链失败事实（互斥）
    /// - 幂等
    pub async fn set_chain_success(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiWithdrawDao::set_chain_success(pool.write_ref(), trade_no).await?;

        if rows > 0 {
            Self::recompute_and_update_status(pool, trade_no).await?;
        }

        Ok(rows)
    }

    /// 设置链失败事实
    ///
    /// 语义：
    /// - 标记链上执行失败
    /// - 清空链成功事实（互斥）
    /// - 幂等
    pub async fn set_chain_failed(
        pool: &ApiFundsDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiWithdrawDao::set_chain_failed(pool.write_ref(), trade_no).await?;

        if rows > 0 {
            Self::recompute_and_update_status(pool, trade_no).await?;
        }

        Ok(rows)
    }

    /// 设置失败阶段事实
    ///
    /// 语义：
    /// - 标记失败发生的阶段
    /// - 使用枚举类型确保语义明确
    /// - 幂等
    pub async fn set_failure_stage(
        pool: &ApiFundsDbPool,
        trade_no: &str,
        stage: WithdrawFailureStage,
    ) -> Result<u64, crate::Error> {
        let rows = ApiWithdrawDao::set_failure_stage(pool.write_ref(), trade_no, stage).await?;

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
        pool: &ApiFundsDbPool,
        trade_no: &str,
        status: Option<ApiWithdrawStatus>,
        err_code: Option<u32>,
        err_msg: Option<&str>,
    ) -> Result<u64, crate::Error> {
        let rows = ApiWithdrawDao::invalidate_raw_tx(
            pool.write_ref(),
            trade_no,
            status,
            err_code,
            err_msg,
        )
        .await?;

        if rows > 0 {
            Self::recompute_and_update_status(pool, trade_no).await?;
        }

        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::ApiWithdrawRepo;
    use crate::{
        dao::api_withdraw::ApiWithdrawDao,
        entities::{
            api_trade_type::ApiTradeType,
            api_withdraw::{ApiWithdrawStatus, WithdrawCreatedFact},
        },
        error::Error,
        repositories::test_helper::setup_api_funds_pool,
    };

    #[tokio::test]
    async fn withdraw_repo_upsert_and_get_success() {
        let pool = setup_api_funds_pool("wallet_db_withdraw_success").await;
        let trade_no = "withdraw_trade_success_1";
        let uid = "uid_wd_s_1";
        let from_addr = "0xfrom_wd_s_1";
        let to_addr = "0xto_wd_s_1";

        ApiWithdrawRepo::upsert_api_withdraw(
            &pool,
            uid,
            "wd_name",
            from_addr,
            to_addr,
            "20",
            "v",
            wallet_types::constant::chain_code::ETHEREUM,
            None,
            "ETH",
            trade_no,
            ApiTradeType::Withdraw,
            0,
            None,
            ApiWithdrawStatus::Init,
            ApiWithdrawStatus::Init,
            "",
            "0",
            None,
            None,
        )
        .await
        .unwrap();

        let got =
            ApiWithdrawRepo::get_api_withdraw_by_trade_no(&pool, trade_no, ApiTradeType::Withdraw)
                .await
                .unwrap();
        assert_eq!(got.trade_no, trade_no);
        assert_eq!(got.uid, uid);
        assert_eq!(got.from_addr, from_addr);
        assert_eq!(got.to_addr, to_addr);
        assert_eq!(got.symbol, "ETH");
        assert_eq!(got.value, "20");

        let rows = ApiWithdrawRepo::list_api_withdraw(&pool, uid).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].trade_no, trade_no);
    }

    #[tokio::test]
    async fn withdraw_repo_missing_trade_no_returns_database_error() {
        let pool = setup_api_funds_pool("wallet_db_withdraw_edge").await;
        let err = ApiWithdrawRepo::get_api_withdraw_by_trade_no(
            &pool,
            "withdraw_missing_trade",
            ApiTradeType::Withdraw,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, Error::Database(_)));
    }

    #[tokio::test]
    async fn withdraw_repo_tx_rollback_keeps_db_unchanged() {
        let pool = setup_api_funds_pool("wallet_db_withdraw_rollback").await;
        let trade_no = "withdraw_trade_rollback_1";

        let mut tx = pool.write_ref().begin().await.unwrap();
        let fact = WithdrawCreatedFact {
            uid: Some("uid_wd_rb_1".to_string()),
            name: "wd_rb".to_string(),
            from_addr: "0xfrom_wd_rb_1".to_string(),
            to_addr: "0xto_wd_rb_1".to_string(),
            symbol: "ETH".to_string(),
            value: "88".to_string(),
            validate: "v".to_string(),
            chain_code: wallet_types::constant::chain_code::ETHEREUM.to_string(),
            token_addr: None,
            trade_no: trade_no.to_string(),
            trade_type: ApiTradeType::Withdraw as i64,
            status: ApiWithdrawStatus::Init,
        };
        ApiWithdrawDao::add(tx.as_mut(), fact).await.unwrap();
        tx.rollback().await.unwrap();

        let got =
            ApiWithdrawRepo::get_api_withdraw_by_trade_no(&pool, trade_no, ApiTradeType::Withdraw)
                .await;
        assert!(matches!(got, Err(Error::Database(_))));

        let rows = ApiWithdrawRepo::list_api_withdraw(&pool, "uid_wd_rb_1").await.unwrap();
        assert!(rows.is_empty());
    }
}
