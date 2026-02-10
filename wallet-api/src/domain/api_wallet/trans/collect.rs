use crate::{
    error::{business::api_wallet::ApiWalletError, service::ServiceError},
    messaging::notify::{FrontendNotifyEvent, api_wallet::CollectFront, event::NotifyEvent},
    request::api_wallet::trans::ApiCollectReq,
};
use std::time::Instant;
use wallet_database::{
    entities::api_collect::ApiCollectStatus,
    repositories::api_wallet::{collect::ApiCollectRepo, wallet::ApiWalletRepo},
};
use wallet_transport_backend::request::api_wallet::transaction::{
    TransAckType, TransEventAckReq, TransType,
};

pub struct ApiCollectDomain {}

impl ApiCollectDomain {
    pub(crate) async fn collect_v2(
        req: &ApiCollectReq,
    ) -> Result<(), crate::error::service::ServiceError> {
        let start_time = Instant::now();
        tracing::info!(
            "开始处理归集交易v2, trade_no: {}, uid: {}, from: {}, to: {}, value: {}, chain: {}, token: {}, symbol: {}, start_time: {:?}",
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

        let ctx = crate::context::CONTEXT.get().unwrap();
        let core_pool = ctx.api_wallet_pool()?;
        let api_funds_pool = ctx.api_funds_pool()?;

        // 1. 校验 + 查钱包
        let wallet = ApiWalletRepo::find_by_uid(&core_pool, &req.uid).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::ApiWalletError::Wallet(
                    crate::error::business::api_wallet::wallet::WalletError::NotFound,
                ),
            ),
        )?;
        let wallet_find_time = Instant::now();

        tracing::info!(trade_no=%req.trade_no, "找到钱包: name={}, 耗时: {:?}", wallet.name, wallet_find_time - start_time);

        // 2. upsert_api_collect（事实落库）
        let res = ApiCollectRepo::get_api_collect_by_trade_no(&api_funds_pool, &req.trade_no).await;
        let tx_check_time = Instant::now();
        tracing::info!(trade_no=%req.trade_no, "检查交易记录, 耗时: {:?}", tx_check_time - wallet_find_time);

        if res.is_err() {
            tracing::info!(trade_no=%req.trade_no, "未找到现有交易记录，开始插入新记录");
            let insert_time = Instant::now();
            ApiCollectRepo::upsert_api_collect(
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
                ApiCollectStatus::Init,
                req.risk_addr,
            )
            .await?;

            tracing::info!(trade_no=%req.trade_no, "成功插入/更新归集交易记录, 耗时: {:?}", insert_time.elapsed());

            let data = NotifyEvent::Collect(CollectFront {
                uid: req.uid.to_string(),
                from_addr: req.from.to_string(),
                to_addr: req.to.to_string(),
                value: req.value.to_string(),
            });
            tracing::info!(trade_no=%req.trade_no, "发送前端通知");
            let notify_time = Instant::now();
            FrontendNotifyEvent::new(data).send().await?;
            tracing::info!(trade_no=%req.trade_no, "前端通知发送成功, 耗时: {:?}", notify_time.elapsed());
        } else {
            tracing::warn!(trade_no=%req.trade_no, "归集交易记录已存在，跳过插入");
        }

        // 注意：在 v2 架构下，不再需要显式提交交易
        // Shadow Scanner 会在下一轮扫描中自动发现新记录并推进执行
        // 交易执行完全由事实驱动，而不是命令式触发

        // 3. 立即触发一次 Shadow 推进（快速通道）
        if let Some(handles) =
            crate::context::CONTEXT.get().unwrap().get_global_handles().await.upgrade()
        {
            if let Some(shadow_system) =
                handles.get_global_processed_collect_tx_handle().get_shadow_system()
            {
                if let Err(e) = shadow_system.trigger_collect(&req.trade_no).await {
                    tracing::warn!(trade_no=%req.trade_no, "触发 Shadow 推进失败，但不影响流程: {:?}", e);
                } else {
                    tracing::info!(trade_no=%req.trade_no, "成功触发 Shadow 快速通道推进");
                }
            }
        }

        tracing::info!(trade_no=%req.trade_no, "归集交易v2处理完成, 总耗时: {:?}", start_time.elapsed());
        Ok(())
    }

    /// recover 的语义：
    /// 修复“手续费不足”这一事实，使交易重新具备构建条件
    /// 不做任何状态回滚，不保证一定继续推进
    pub async fn recover(trade_no: &str) -> Result<(), crate::error::service::ServiceError> {
        let start_time = Instant::now();
        tracing::info!(trade_no=%trade_no, "开始恢复归集交易");

        let pool = crate::context::CONTEXT.get().unwrap().api_funds_pool()?;

        // 1. 解除事实阻断（核心）
        tracing::info!(trade_no=%trade_no, "清除服务费需求标记");
        let clear_time = Instant::now();
        ApiCollectRepo::clear_need_service_fee(&pool, trade_no).await?;
        tracing::info!(trade_no=%trade_no, "服务费需求标记清除成功, 耗时: {:?}", clear_time.elapsed());

        // 2. 快速触发 Shadow
        if let Some(handles) =
            crate::context::CONTEXT.get().unwrap().get_global_handles().await.upgrade()
        {
            if let Some(shadow_system) =
                handles.get_global_processed_collect_tx_handle().get_shadow_system()
            {
                if let Err(e) = shadow_system.trigger_collect(trade_no).await {
                    tracing::warn!(trade_no=%trade_no, "触发 Shadow 推进失败，但不影响流程: {:?}", e);
                } else {
                    tracing::info!(trade_no=%trade_no, "成功触发 Shadow 快速通道推进");
                }
            }
        }

        tracing::info!(
            trade_no=%trade_no,
            "归集交易 recover 完成, 耗时: {:?}",
            start_time.elapsed()
        );
        Ok(())
    }

    pub async fn confirm_tx(
        trade_no: &str,
        status: bool,
        fail_type: i32,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().api_funds_pool()?;
        Self::confirm_tx_with_pool(&pool, trade_no, status, fail_type).await?;

        // 立即触发一次 Shadow 推进（快速通道）
        if let Some(handles) =
            crate::context::CONTEXT.get().unwrap().get_global_handles().await.upgrade()
        {
            if let Some(shadow_system) =
                handles.get_global_processed_collect_tx_handle().get_shadow_system()
            {
                if let Err(e) = shadow_system.trigger_collect(trade_no).await {
                    tracing::warn!(trade_no=%trade_no, "触发 Shadow 推进失败，但不影响流程: {:?}", e);
                } else {
                    tracing::info!(trade_no=%trade_no, "成功触发 Shadow 快速通道推进");
                }
            }
        }

        Ok(())
    }

    pub(crate) async fn confirm_tx_with_pool(
        pool: &wallet_database::ApiFundsDbPool,
        trade_no: &str,
        status: bool,
        fail_type: i32,
    ) -> Result<(), crate::error::service::ServiceError> {
        let start_time = Instant::now();
        tracing::info!(trade_no=%trade_no, "开始确认归集交易, 状态: {}, 失败类型: {}, start_time: {:?}", status, fail_type, start_time);

        tracing::info!(trade_no=%trade_no, "查询交易记录");
        let query_time = Instant::now();
        let mut tx = match ApiCollectRepo::get_api_collect_by_trade_no(pool, trade_no).await {
            Ok(tx) => tx,
            Err(e) => {
                tracing::warn!(
                    trade_no = %trade_no,
                    status = %status,
                    fail_type = %fail_type,
                    error = %e,
                    "collect confirm_tx: trade_no not found (will NOT ack)"
                );
                return Err(e.into());
            }
        };
        tracing::info!(trade_no=%trade_no, "找到交易记录, 当前状态: {:?}, 耗时: {:?}", tx.status, query_time.elapsed());

        // ====== 必须先确保 transaction_time 事实存在，再做任何 repeat early return ======
        if tx.transaction_time.is_none() {
            let now = chrono::Utc::now().to_rfc3339();
            let rows = ApiCollectRepo::confirm_transaction_time_if_absent(pool, trade_no, &now)
                .await
                .map_err(|e| {
                    tracing::warn!(
                        trade_no = %trade_no,
                        status = %status,
                        fail_type = %fail_type,
                        error = %e,
                        "collect confirm_tx: confirm_transaction_time_if_absent failed (will NOT ack)"
                    );
                    e
                })?;

            if rows == 0 {
                // 并发场景：可能已被其他路径写入；重查一次确认事实
                tx = ApiCollectRepo::get_api_collect_by_trade_no(pool, trade_no).await?;
                if tx.transaction_time.is_none() {
                    tracing::warn!(
                        trade_no = %trade_no,
                        status = %status,
                        fail_type = %fail_type,
                        "collect confirm_tx: expected transaction_time to be set, but still NULL after retry (will NOT ack)"
                    );
                    return Err(crate::error::system::SystemError::Internal(
                        "transaction_time still NULL after confirm_transaction_time_if_absent"
                            .to_string(),
                    )
                    .into());
                }
            } else {
                // 写入成功后刷新一次，保证后续判断基于最新事实
                tx = ApiCollectRepo::get_api_collect_by_trade_no(pool, trade_no).await?;
            }
        }

        let update_time = Instant::now();
        if status {
            if tx.status == ApiCollectStatus::Success
                || tx.status == ApiCollectStatus::ConfirmSuccessReport
            {
                tracing::warn!(trade_no=%trade_no, "归集交易确认重复");
                return Ok(());
            }

            let rows_affected = ApiCollectRepo::update_api_collect_next_status(
                pool,
                trade_no,
                ApiCollectStatus::SendingTxReport,
                ApiCollectStatus::Success,
            )
            .await?;
            if rows_affected != 1 {
                tracing::error!(
                    trade_no = trade_no,
                    "api_collect_next_status returned unexpected rows_affected"
                );
                // return Err(ServiceError::Business(ApiWalletError::StatusNotMatched.into()));
            }
        } else {
            if tx.status == ApiCollectStatus::Failure
                || tx.status == ApiCollectStatus::ConfirmFailureReport
            {
                tracing::warn!(trade_no=%trade_no, "归集交易确认重复");
                return Ok(());
            }

            if tx.status == ApiCollectStatus::InsufficientBalance && fail_type == 2 {
                tracing::info!(trade_no=%trade_no, "更新交易状态为失败(余额不足)");
                let rows_affected = ApiCollectRepo::update_api_collect_next_status_and_err(
                    pool,
                    trade_no,
                    ApiCollectStatus::InsufficientBalance,
                    ApiCollectStatus::Failure,
                    102,
                    "confirm transfer fee failed insufficient balance",
                )
                .await?;
                if rows_affected != 1 {
                    tracing::error!(
                        trade_no = trade_no,
                        "api_collect_next_status returned unexpected rows_affected: {}",
                        rows_affected
                    );
                    // return Err(ServiceError::Business(ApiWalletError::StatusNotMatched.into()));
                }
                tracing::info!(trade_no=%trade_no, "交易状态更新成功");
            } else {
                tracing::info!(trade_no=%trade_no, "更新交易状态为失败");
                let rows_affected = ApiCollectRepo::update_api_collect_next_status(
                    pool,
                    trade_no,
                    ApiCollectStatus::SendingTxReport,
                    ApiCollectStatus::Failure,
                )
                .await?;
                if rows_affected != 1 {
                    tracing::error!(
                        trade_no = trade_no,
                        "api_collect_next_status returned unexpected rows_affected: {}",
                        rows_affected
                    );
                    // return Err(ServiceError::Business(ApiWalletError::StatusNotMatched.into()));
                }
                tracing::info!(trade_no=%trade_no, "交易状态更新成功");
            }
        }
        tracing::info!(trade_no=%trade_no, "更新交易状态, 耗时: {:?}", update_time.elapsed());

        tracing::info!(trade_no=%trade_no, "归集交易确认完成, 总耗时: {:?}", start_time.elapsed());
        Ok(())
    }
}
