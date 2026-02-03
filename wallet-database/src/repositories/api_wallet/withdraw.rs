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
    CollectDbPool,
    dao::api_withdraw::ApiWithdrawDao,
    entities::{
        api_trade_type::ApiTradeType,
        api_withdraw::{ApiWithdrawEntity, ApiWithdrawStatus, ErrCode, WithdrawCreatedFact},
    },
    pagination::Pagination,
};

pub struct ApiWithdrawRepo;

impl ApiWithdrawRepo {
    pub async fn list_api_withdraw(
        pool: &CollectDbPool,
        uid: &str,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::all_api_withdraw(pool.as_ref(), uid).await
    }

    pub async fn list_api_withdraw_with_status(
        pool: &CollectDbPool,
        status: Vec<ApiWithdrawStatus>,
        page: i64,
        page_size: i64,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::list_api_withdraw_with_status(pool.as_ref(), status, page, page_size).await
    }

    pub async fn page_api_withdraw(
        pool: &CollectDbPool,
        uid: &str,
        status: Vec<ApiWithdrawStatus>,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::page_api_withdraw(pool.as_ref(), uid, status, page, page_size).await
    }

    pub async fn page_api_withdraw_with_init_status(
        pool: &CollectDbPool,
        uid: &str,
        init_status: ApiWithdrawStatus,
        status: Vec<ApiWithdrawStatus>,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::page_api_withdraw_with_init_status(
            pool.as_ref(),
            uid,
            init_status,
            status,
            page,
            page_size,
        )
        .await
    }

    pub async fn get_api_withdraw_by_id(
        pool: &CollectDbPool,
        id: &str,
    ) -> Result<ApiWithdrawEntity, crate::Error> {
        ApiWithdrawDao::get_api_withdraw_by_id(pool.as_ref(), id).await
    }

    pub async fn get_api_withdraw_by_trade_no(
        pool: &CollectDbPool,
        trade_no: &str,
        trade_type: ApiTradeType,
    ) -> Result<ApiWithdrawEntity, crate::Error> {
        ApiWithdrawDao::get_api_withdraw_by_trade_no(pool.as_ref(), trade_no, trade_type).await
    }

    pub async fn get_api_withdraw_by_trade_no_status(
        pool: &CollectDbPool,
        trade_no: &str,
        vec_status: &[ApiWithdrawStatus],
    ) -> Result<ApiWithdrawEntity, crate::Error> {
        ApiWithdrawDao::get_api_withdraw_by_trade_no_status(pool.as_ref(), trade_no, vec_status)
            .await
    }

    pub async fn get_by_hash_and_owner(
        pool: &CollectDbPool,
        owner: &str,
        tx_hash: &str,
    ) -> Result<ApiWithdrawEntity, crate::Error> {
        ApiWithdrawDao::get_by_hash_and_owner(pool.as_ref(), owner, tx_hash).await
    }

    pub async fn lists_by_hashs(
        pool: &CollectDbPool,
        owner: &str,
        hashs: Vec<String>,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::lists_by_hashs(pool.as_ref(), owner, hashs).await
    }

    pub async fn recent_bill(
        pool: &CollectDbPool,
        token: &str,
        from_addr: &str,
        chain_code: &str,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<ApiWithdrawEntity>, crate::Error> {
        let lists = ApiWithdrawDao::recent_bill(
            pool.as_ref(),
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
        pool: &CollectDbPool,
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
            pool.as_ref(),
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
        trade_type: ApiTradeType,
        nonce: i64,
        tx_hash: &str,
        init_status: ApiWithdrawStatus,
        status: ApiWithdrawStatus,
        resource_consume: &str,
        transaction_fee: &str,
        transaction_time: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
        block_height: &str,
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
            token_addr,
            symbol: symbol.to_string(),
            trade_no: trade_no.to_string(),
            trade_type,
            init_status,
            status,
            nonce,
            tx_hash: Some(tx_hash.to_string()),
            raw_tx: None,
            resource_consume: resource_consume.to_string(),
            transaction_fee: transaction_fee.to_string(),
            transaction_time,
            block_height: Some(block_height.to_string()),
            notes: None,
            post_tx_count: 0,
            post_confirm_tx_count: 0,
            err_code: None,
            err_msg: None,
            created_at: Default::default(),
            updated_at: None,
            tx_ack_sent_at: None,
            tx_res_ack_sent_at: None,
            tx_ack_attempted_at: None,
            building_at: None,
            last_broadcast_at: None,
            tx_res_ack_attempted_at: None,
            tx_exec_receipt_attempted_at: None,
            tx_exec_receipt_uploaded_at: None,
            finished_at: None,
        };
        ApiWithdrawDao::upsert(pool.as_ref(), withdraw_req).await
    }

    /// 保留原签名，确保兼容性
    pub async fn upsert_api_withdraw_with_fact(
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
            token_addr,
            trade_no: trade_no.to_string(),
            trade_type: trade_type as i64,
            status: ApiWithdrawStatus::Init,
        };
        ApiWithdrawDao::add(pool.as_ref(), withdraw_req).await
    }

    pub async fn update_api_fee_post_tx_count(
        pool: &CollectDbPool,
        trade_no: &str,
        status: ApiWithdrawStatus,
    ) -> Result<(), crate::Error> {
        ApiWithdrawDao::update_post_tx_count(pool.as_ref(), trade_no, status).await
    }

    pub async fn update_api_withdraw_tx_status(
        pool: &CollectDbPool,
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
            pool.as_ref(),
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
        pool: &CollectDbPool,
        from_addr: &str,
        chain_code: &str,
        trade_no: &str,
        nonce: i64,
        tx_hash: &str,
        resource_consume: &str,
        transaction_fee: &str,
        status: ApiWithdrawStatus,
    ) -> Result<u64, crate::Error> {
        ApiWithdrawDao::update_tx_status_nonce(
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

    pub async fn update_api_withdraw_tx(
        pool: &CollectDbPool,
        trade_no: &str,
        resource_consume: &str,
        transaction_fee: &str,
        transaction_time: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
        block_height: &str,
    ) -> Result<(), crate::Error> {
        ApiWithdrawDao::update_tx(
            pool.as_ref(),
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
        pool: &CollectDbPool,
        trade_no: &str,
        status: ApiWithdrawStatus,
        err_code: ErrCode,
        err_msg: &str,
    ) -> Result<u64, crate::Error> {
        ApiWithdrawDao::update_status_and_err(pool.as_ref(), trade_no, status, err_code, err_msg)
            .await
    }

    #[deprecated(
        since = "0.1.0",
        note = "LEGACY STATE MACHINE API. Do not use in Shadow / Scanner / fact-driven paths. Use fact-based APIs instead."
    )]
    pub async fn update_api_withdraw_status(
        pool: &CollectDbPool,
        trade_no: &str,
        status: ApiWithdrawStatus,
    ) -> Result<u64, crate::Error> {
        ApiWithdrawDao::update_status(pool.as_ref(), trade_no, status).await
    }

    #[deprecated(
        since = "0.1.0",
        note = "LEGACY STATE MACHINE API. Do not use in Shadow / Scanner / fact-driven paths. Use fact-based APIs instead."
    )]
    pub async fn update_api_withdraw_next_status(
        pool: &CollectDbPool,
        trade_no: &str,
        status: ApiWithdrawStatus,
        next_status: ApiWithdrawStatus,
    ) -> Result<u64, crate::Error> {
        ApiWithdrawDao::update_next_status(pool.as_ref(), trade_no, status, next_status).await
    }

    #[deprecated(
        since = "0.1.0",
        note = "LEGACY STATE MACHINE API. Do not use in Shadow / Scanner / fact-driven paths. Use fact-based APIs instead."
    )]
    pub async fn update_api_withdraw_post_tx_count(
        pool: &CollectDbPool,
        trade_no: &str,
        status: ApiWithdrawStatus,
    ) -> Result<(), crate::Error> {
        ApiWithdrawDao::update_post_tx_count(pool.as_ref(), trade_no, status).await
    }

    #[deprecated(
        since = "0.1.0",
        note = "LEGACY STATE MACHINE API. Do not use in Shadow / Scanner / fact-driven paths. Use fact-based APIs instead."
    )]
    pub async fn update_api_withdraw_post_confirm_tx_count(
        pool: &CollectDbPool,
        trade_no: &str,
        status: ApiWithdrawStatus,
    ) -> Result<(), crate::Error> {
        ApiWithdrawDao::update_post_confirm_tx_count(pool.as_ref(), trade_no, status).await
    }

    #[deprecated(since = "0.1.0", note = "LEGACY API. Use mark_tx_ack_sent instead.")]
    /// 设置 Tx ACK 发送时间
    pub async fn set_tx_ack_sent(pool: &CollectDbPool, trade_no: &str) -> Result<(), crate::Error> {
        ApiWithdrawDao::mark_tx_ack_sent(pool.as_ref(), trade_no).await.map(|_| ())
    }

    #[deprecated(since = "0.1.0", note = "LEGACY API. Use mark_tx_res_ack_sent instead.")]
    /// 设置 TxRes ACK 发送时间
    pub async fn set_tx_res_ack_sent(
        pool: &CollectDbPool,
        trade_no: &str,
    ) -> Result<(), crate::Error> {
        ApiWithdrawDao::mark_tx_res_ack_sent(pool.as_ref(), trade_no).await.map(|_| ())
    }

    #[deprecated(
        since = "0.1.0",
        note = "LEGACY API. Use mark_tx_res_ack_sent_and_chain_finished instead."
    )]
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
        let rows = ApiWithdrawDao::mark_tx_res_ack_sent_and_chain_finished(pool.as_ref(), trade_no)
            .await?;

        if rows > 0 {
            Self::recompute_and_update_status(pool, trade_no).await?;
        }

        Ok(())
    }

    /// 获取 ACK 发送时间
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
        ApiWithdrawDao::get_ack_times(pool.as_ref(), trade_no).await
    }

    /// 扫描需要发送交易结果 ACK 的交易
    pub async fn scan_confirmed_need_tx_res_ack(
        pool: &CollectDbPool,
        limit: usize,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::scan_need_tx_res_ack(pool.as_ref(), limit).await
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
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::scan_need_tx_ack(pool.as_ref(), limit).await
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
        pool: &CollectDbPool,
        limit: usize,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::scan_need_recover(pool.as_ref(), limit).await
    }

    /// 扫描可构建的交易
    ///
    /// ⚠️ 核心事实驱动原则：
    /// - 只基于不可逆事实字段(raw_tx)决策
    /// - 不依赖时间字段(building_at)进行决策
    /// - 并发通过raw_tx写入唯一性保证
    ///
    /// ⚠️ 强顺序屏障：
    /// - BuildTx 必须发生在 Tx ACK 之后
    /// - 禁止移除 tx_ack_sent_at 条件，否则会破坏强顺序保证
    pub async fn scan_can_build(
        pool: &CollectDbPool,
        limit: usize,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::scan_can_build(pool.as_ref(), limit).await
    }

    /// 扫描可广播的交易
    ///
    /// ⚠️ 核心事实驱动原则：
    /// - 只基于不可逆事实字段(raw_tx, transaction_time)决策
    /// - 不依赖时间字段(last_broadcast_at)进行决策
    /// - 并发通过transaction_time写入唯一性保证
    pub async fn scan_can_broadcast(
        pool: &CollectDbPool,
        limit: usize,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::scan_can_broadcast(pool.as_ref(), limit).await
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
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::scan_need_tx_exec_receipt_upload(pool.as_ref(), limit).await
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
        pool: &CollectDbPool,
        limit: usize,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::Error> {
        ApiWithdrawDao::scan_need_tx_res_ack(pool.as_ref(), limit).await
    }

    /// 更新building_at时间
    pub async fn update_building_at(
        pool: &CollectDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiWithdrawDao::update_building_at(pool.as_ref(), trade_no).await
    }

    /// 更新last_broadcast_at时间
    pub async fn update_last_broadcast_at(
        pool: &CollectDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiWithdrawDao::update_last_broadcast_at(pool.as_ref(), trade_no).await
    }

    /// 构建交易后更新
    pub async fn update_after_build(
        pool: &CollectDbPool,
        trade_no: &str,
        tx_hash: &str,
        raw_tx: &str,
        transaction_fee: &str,
        nonce: i64,
    ) -> Result<u64, crate::Error> {
        let rows = ApiWithdrawDao::update_after_build(
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

    /// 标记广播已执行
    pub async fn mark_broadcast_executed(
        pool: &CollectDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiWithdrawDao::mark_broadcast_executed(pool.as_ref(), trade_no).await
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
        ApiWithdrawDao::update_tx_res_received_at(pool.as_ref(), trade_no).await
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
        ApiWithdrawDao::mark_tx_ack_attempted(pool.as_ref(), trade_no).await
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
        let rows = ApiWithdrawDao::mark_tx_ack_sent(pool.as_ref(), trade_no).await?;

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
        ApiWithdrawDao::mark_tx_exec_receipt_attempted(pool.as_ref(), trade_no).await
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
        let rows = ApiWithdrawDao::mark_tx_exec_receipt_uploaded(pool.as_ref(), trade_no).await?;

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
        ApiWithdrawDao::mark_tx_res_ack_attempted(pool.as_ref(), trade_no).await
    }

    /// 标记交易结果 ACK 已发送
    ///
    /// 语义：
    /// - 交易结果 ACK 已成功发送到后端
    /// - 这是副作用完成的事实
    ///
    /// ⚠️ 调用约束：
    /// - 仅允许在交易结果 ACK 已尝试的前提下调用
    /// - 仅允许调用一次（tx_res_ack_sent_at IS NULL）
    /// - 由 SideEffectWorker 调用
    pub async fn mark_tx_res_ack_sent(
        pool: &CollectDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiWithdrawDao::mark_tx_res_ack_sent(pool.as_ref(), trade_no).await?;

        if rows > 0 {
            Self::recompute_and_update_status(pool, trade_no).await?;
        }

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
        pool: &CollectDbPool,
        trade_no: &str,
        tx_hash: &str,
        transaction_time: &str,
        transaction_fee: &str,
        resource_consume: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiWithdrawDao::confirm_onchain_transaction_fact(
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
        let rows = ApiWithdrawDao::confirm_onchain_transaction_fact_with_recover(
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

    /// 更新交易状态和错误信息
    pub async fn update_status_and_err(
        pool: &CollectDbPool,
        trade_no: &str,
        status: ApiWithdrawStatus,
        err_code: ErrCode,
        err_msg: &str,
    ) -> Result<u64, crate::Error> {
        ApiWithdrawDao::update_status_and_err(pool.as_ref(), trade_no, status, err_code, err_msg)
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
    async fn recompute_and_update_status(
        pool: &CollectDbPool,
        trade_no: &str,
    ) -> Result<(), crate::Error> {
        let entity =
            Self::get_api_withdraw_by_trade_no(pool, trade_no, ApiTradeType::Withdraw).await?;

        // 按终止优先级从高到低判断
        // 1. 终止型错误（最高优先级）
        if entity.err_code.is_some() {
            let new_status = ApiWithdrawStatus::Failure;
            if entity.status != new_status {
                ApiWithdrawDao::update_status(pool.as_ref(), trade_no, new_status).await?;
            }
            return Ok(());
        }

        // 2. 链上终态
        if entity.finished_at.is_some() {
            let new_status = if entity.tx_res_ack_sent_at.is_some() {
                ApiWithdrawStatus::Success
            } else {
                // 使用现有的状态，因为 ApiWithdrawStatus 中没有 ChainFinished
                entity.status
            };
            if entity.status != new_status {
                ApiWithdrawDao::update_status(pool.as_ref(), trade_no, new_status).await?;
            }
            return Ok(());
        }

        // 3. 链上事实已确认（但未终态）
        if entity.tx_hash.is_some() {
            if entity.tx_exec_receipt_uploaded_at.is_none() {
                // 使用现有的状态，因为 ApiWithdrawStatus 中没有 NeedUploadTxExecReceipt
                let new_status = entity.status;
                if entity.status != new_status {
                    ApiWithdrawDao::update_status(pool.as_ref(), trade_no, new_status).await?;
                }
                return Ok(());
            }

            if entity.tx_res_ack_sent_at.is_none() {
                // 使用现有的状态，因为 ApiWithdrawStatus 中没有 NeedTxResAck
                let new_status = entity.status;
                if entity.status != new_status {
                    ApiWithdrawDao::update_status(pool.as_ref(), trade_no, new_status).await?;
                }
                return Ok(());
            }

            // 使用现有的状态，因为 ApiWithdrawStatus 中没有 OnchainConfirmed
            let new_status = entity.status;
            if entity.status != new_status {
                ApiWithdrawDao::update_status(pool.as_ref(), trade_no, new_status).await?;
            }
            return Ok(());
        }

        // 4. raw_tx 阶段
        if entity.raw_tx.is_some() {
            let new_status = if entity.last_broadcast_at.is_none() {
                // 使用现有的状态，因为 ApiWithdrawStatus 中没有 CanBroadcast
                ApiWithdrawStatus::SendingTx
            } else {
                // 使用现有的状态，因为 ApiWithdrawStatus 中没有 Broadcasted
                ApiWithdrawStatus::SendingTx
            };
            if entity.status != new_status {
                ApiWithdrawDao::update_status(pool.as_ref(), trade_no, new_status).await?;
            }
            return Ok(());
        }

        // 5. 可构建
        // 检查是否满足构建条件
        if entity.tx_ack_sent_at.is_some() {
            // 使用现有的状态，因为 ApiWithdrawStatus 中没有 CanBuild
            let new_status = entity.status;
            if entity.status != new_status {
                ApiWithdrawDao::update_status(pool.as_ref(), trade_no, new_status).await?;
            }
            return Ok(());
        }

        // 6. 默认初始态
        let new_status = ApiWithdrawStatus::Init;
        if entity.status != new_status {
            ApiWithdrawDao::update_status(pool.as_ref(), trade_no, new_status).await?;
        }

        Ok(())
    }

    /// 标记链上终态
    ///
    /// 语义：
    /// - 交易已在链上达到终态（成功或失败）
    /// - 所有必要的副作用已完成
    /// - 这是一个不可逆的事实
    pub async fn mark_chain_finished(
        pool: &CollectDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiWithdrawDao::mark_chain_finished(pool.as_ref(), trade_no).await?;

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
        pool: &CollectDbPool,
        trade_no: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiWithdrawDao::mark_tx_res_ack_sent_and_chain_finished(pool.as_ref(), trade_no)
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
        pool: &CollectDbPool,
        trade_no: &str,
        transaction_time: &str,
    ) -> Result<u64, crate::Error> {
        let rows = ApiWithdrawDao::confirm_transaction_time_if_absent(
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
        status: Option<ApiWithdrawStatus>,
        err_code: Option<u32>,
        err_msg: Option<&str>,
    ) -> Result<u64, crate::Error> {
        let rows =
            ApiWithdrawDao::invalidate_raw_tx(pool.as_ref(), trade_no, status, err_code, err_msg)
                .await?;

        if rows > 0 {
            Self::recompute_and_update_status(pool, trade_no).await?;
        }

        Ok(rows)
    }
}
