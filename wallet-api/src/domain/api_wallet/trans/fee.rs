use crate::{
    error::{
        business::{
            BusinessError,
            api_wallet::{ApiWalletError, wallet::WalletError},
        },
        service::ServiceError,
    },
    messaging::notify::{FrontendNotifyEvent, api_wallet::FeeFront, event::NotifyEvent},
    request::api_wallet::trans::ApiTransferFeeReq,
};
use std::time::Instant;
use wallet_database::{
    entities::api_fee::ApiFeeStatus,
    repositories::api_wallet::{fee::ApiFeeRepo, wallet::ApiWalletRepo},
};
use wallet_transport_backend::request::api_wallet::transaction::{
    TransAckType, TransEventAckReq, TransType,
};

pub struct ApiFeeDomain {}

impl ApiFeeDomain {
    pub(crate) async fn transfer_fee(
        req: &ApiTransferFeeReq,
    ) -> Result<(), crate::error::service::ServiceError> {
        let start_time = Instant::now();
        tracing::info!(
            "开始处理手续费交易, trade_no: {}, uid: {}, from: {}, to: {}, value: {}, chain: {}, token: {}, symbol: {}, start_time: {:?}",
            req.trade_no,
            req.uid,
            req.from,
            req.to,
            req.value,
            req.chain_code,
            req.token_address.as_deref().unwrap_or(""),
            req.symbol,
            start_time
        );

        // 获取数据库连接
        let ctx = crate::context::CONTEXT.get().unwrap();
        let core_pool = ctx.core_pool()?;
        let api_funds_pool = ctx.api_funds_pool()?;

        // 获取钱包
        tracing::info!(trade_no=%req.trade_no, "查询钱包信息");
        let wallet = ApiWalletRepo::find_by_uid(&core_pool, &req.uid).await?.ok_or(
            BusinessError::ApiWallet(ApiWalletError::Wallet(WalletError::NotFound.into())),
        )?;
        let wallet_find_time = Instant::now();
        tracing::info!(trade_no=%req.trade_no, "找到钱包: name={}, 耗时: {:?}", wallet.name, wallet_find_time - start_time);

        // fix: 2186
        tracing::info!(trade_no=%req.trade_no, "发送交易事件确认请求");
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        // 检查 Tx ACK 是否已发送
        let (tx_ack_sent_at, _) =
            ApiFeeRepo::get_ack_times(&api_funds_pool, &req.trade_no).await.unwrap_or((None, None));
        if tx_ack_sent_at.is_none() {
            let trans_event_req =
                TransEventAckReq::new(&req.trade_no, TransType::ColFee, TransAckType::Tx);
            backend.trans_event_ack(&trans_event_req).await?;
            // 设置 Tx ACK 发送时间
            ApiFeeRepo::set_tx_ack_sent(&api_funds_pool, &req.trade_no).await?
        } else {
            tracing::warn!(trade_no=%req.trade_no, ?tx_ack_sent_at, "Tx ack 已发送，跳过");
        }
        let event_ack_time = Instant::now();
        tracing::info!(trade_no=%req.trade_no, "交易事件确认处理完成, 耗时: {:?}", event_ack_time - wallet_find_time);

        tracing::info!(trade_no=%req.trade_no, "检查手续费交易记录");
        let res = ApiFeeRepo::get_api_fee_by_trade_no(&api_funds_pool, &req.trade_no).await;
        let tx_check_time = Instant::now();
        tracing::info!(trade_no=%req.trade_no, "检查交易记录, 耗时: {:?}", tx_check_time - wallet_find_time);

        if res.is_err() {
            tracing::info!(trade_no=%req.trade_no, "未找到现有手续费交易记录，开始插入新记录");
            let insert_time = Instant::now();
            ApiFeeRepo::upsert_api_fee(
                &api_funds_pool,
                &req.uid,
                &wallet.name,
                &req.from,
                &req.to,
                &req.value,
                &req.validate,
                &req.chain_code,
                req.token_address.clone(),
                &req.symbol.to_uppercase(),
                &req.trade_no,
                req.trade_type,
            )
            .await?;
            tracing::info!(trade_no=%req.trade_no, "落盘手续费: 耗时: {:?}", insert_time.elapsed());

            // 关键新增：清除构建阻断标记
            // ⚠️ 系统不变量（当前成立）：
            // - build_blocked_at 目前**只可能**因 InsufficientBalance 被设置
            // - 因此此方法等价于 clear_build_blocked_if_insufficient_balance
            //
            // ❗️若未来引入其他 build_blocked 来源，
            // 必须：
            // 1. 拆分 clear 方法
            // 2. 或在 SQL 中增加明确约束
            let cleared = ApiFeeRepo::clear_build_blocked(&api_funds_pool, &req.trade_no).await?;
            if cleared > 0 {
                tracing::info!(trade_no=%req.trade_no, "Build blocked cleared due to sufficient balance");
            }

            tracing::info!(trade_no=%req.trade_no, "准备发送前端通知");
            let notify_time = Instant::now();
            let data = NotifyEvent::Fee(FeeFront {
                uid: req.uid.to_string(),
                from_addr: req.from.to_string(),
                to_addr: req.to.to_string(),
                value: req.value.to_string(),
            });
            FrontendNotifyEvent::new(data).send().await?;
            tracing::info!(trade_no=%req.trade_no, "前端通知发送成功, 耗时: {:?}", notify_time.elapsed());
        } else {
            tracing::warn!(trade_no=%req.trade_no, "fee tx found, 交易记录已存在");
            
            // 关键新增：即使交易记录已存在，也清除构建阻断标记
            // 因为手续费可能是后来入账的
            // ⚠️ 系统不变量（当前成立）：
            // - build_blocked_at 目前**只可能**因 InsufficientBalance 被设置
            // - 因此此方法等价于 clear_build_blocked_if_insufficient_balance
            //
            // ❗️若未来引入其他 build_blocked 来源，
            // 必须：
            // 1. 拆分 clear 方法
            // 2. 或在 SQL 中增加明确约束
            let cleared = ApiFeeRepo::clear_build_blocked(&api_funds_pool, &req.trade_no).await?;
            if cleared > 0 {
                tracing::info!(trade_no=%req.trade_no, "Build blocked cleared due to sufficient balance (existing record)");
            }
        }

        // 注意：在 v2 架构下，不再需要显式提交交易
        // Shadow Scanner 会在下一轮扫描中自动发现新记录并推进执行
        // 交易执行完全由事实驱动，而不是命令式触发

        // 立即触发一次 Shadow 推进（快速通道）
        if let Some(handles) =
            crate::context::CONTEXT.get().unwrap().get_global_handles().await.upgrade()
        {
            if let Some(shadow_system) =
                handles.get_global_processed_fee_tx_handle().get_shadow_system()
            {
                if let Err(e) = shadow_system.trigger_fee(&req.trade_no).await {
                    tracing::warn!(trade_no=%req.trade_no, "触发 Shadow 推进失败，但不影响流程: {:?}", e);
                } else {
                    tracing::info!(trade_no=%req.trade_no, "成功触发 Shadow 快速通道推进");
                }
            }
        }

        tracing::info!(trade_no=%req.trade_no, "手续费交易处理完成, 总耗时: {:?}", start_time.elapsed());
        Ok(())
    }

    pub async fn confirm_tx(trade_no: &str, status: bool) -> Result<(), ServiceError> {
        let start_time = Instant::now();
        tracing::info!(trade_no=%trade_no, "开始确认手续费交易, 状态: {}, start_time: {:?}", status, start_time);

        let pool = crate::context::CONTEXT.get().unwrap().api_funds_pool()?;
        tracing::info!(trade_no=%trade_no, "查询手续费交易记录");
        let query_time = Instant::now();
        let tx = ApiFeeRepo::get_api_fee_by_trade_no(&pool, trade_no).await?;
        tracing::info!(trade_no=%trade_no, "找到手续费交易记录, 当前状态: {:?}, 耗时: {:?}", tx.status, query_time.elapsed());

        if status {
            if tx.status == ApiFeeStatus::Success || tx.status == ApiFeeStatus::ConfirmSuccessReport
            {
                tracing::warn!(trade_no=%trade_no, "fee confirmation repeat, 确认重复");
                return Ok(());
            }
        } else {
            if tx.status == ApiFeeStatus::Failure || tx.status == ApiFeeStatus::ConfirmFailureReport
            {
                tracing::warn!(trade_no=%trade_no, "fee confirmation repeat, 确认重复");
                return Ok(());
            }
        }

        tracing::info!(trade_no=%trade_no, "更新手续费交易状态");
        let update_time = Instant::now();
        let next_status: ApiFeeStatus =
            if status { ApiFeeStatus::Success } else { ApiFeeStatus::Failure };
        let rows_affected = ApiFeeRepo::update_api_fee_next_status(
            &pool,
            trade_no,
            ApiFeeStatus::SendingTxReport,
            next_status,
        )
        .await?;
        tracing::info!(trade_no=%trade_no, "更新交易状态, 影响行数: {}, 耗时: {:?}", rows_affected, update_time.elapsed());

        if rows_affected != 1 {
            tracing::error!(trade_no=%trade_no, "更新手续费交易状态失败，影响行数不符合预期");
            return Err(ServiceError::Business(ApiWalletError::StatusNotMatched.into()));
        }

        // 注意：在 v2 架构下，不再需要显式提交确认报告
        // Shadow Scanner 会在下一轮扫描中自动发现状态变化并触发确认报告
        // 交易执行完全由事实驱动，而不是命令式触发

        // 立即触发一次 Shadow 推进（快速通道）
        if let Some(handles) =
            crate::context::CONTEXT.get().unwrap().get_global_handles().await.upgrade()
        {
            if let Some(shadow_system) =
                handles.get_global_processed_fee_tx_handle().get_shadow_system()
            {
                if let Err(e) = shadow_system.trigger_fee(trade_no).await {
                    tracing::warn!(trade_no=%trade_no, "触发 Shadow 推进失败，但不影响流程: {:?}", e);
                } else {
                    tracing::info!(trade_no=%trade_no, "成功触发 Shadow 快速通道推进");
                }
            }
        }

        tracing::info!(trade_no=%trade_no, "手续费交易确认完成, 总耗时: {:?}", start_time.elapsed());
        Ok(())
    }
}
