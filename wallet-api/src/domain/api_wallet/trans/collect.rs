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
        let core_pool = ctx.core_pool()?;
        let api_funds_pool = ctx.api_funds_pool()?;

        // 获取钱包信息（core数据库）
        let wallet = ApiWalletRepo::find_by_uid(&core_pool, &req.uid).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::ApiWalletError::NotFoundAccount,
            ),
        )?;
        let wallet_find_time = Instant::now();

        tracing::info!(trade_no=%req.trade_no, "找到钱包: name={}, 耗时: {:?}", wallet.name, wallet_find_time - start_time);

        // fix: 2186 - 将trans_event_ack移到前面，确保只有在确认后才将交易插入数据库
        let backend = ctx.get_global_backend_api();
        // 检查Tx ACK 是否已发送
        let (order_ack_sent_at, _) =
            ApiCollectRepo::get_ack_times(&api_funds_pool, &req.trade_no).await?;
        if order_ack_sent_at.is_none() {
            let trans_event_req =
                TransEventAckReq::new(&req.trade_no, TransType::Col, TransAckType::Tx);
            tracing::info!(trade_no=%req.trade_no, "发送交易事件确认请求");
            backend.trans_event_ack(&trans_event_req).await?;
            // 设置Tx ACK 发送时间
            ApiCollectRepo::set_order_ack_sent(&api_funds_pool, &req.trade_no).await?;
        } else {
            tracing::warn!(trade_no=%req.trade_no, ?order_ack_sent_at, "Order ack 已发送，跳过");
        }
        let event_ack_time = Instant::now();
        tracing::info!(trade_no=%req.trade_no, "交易事件确认处理完成, 耗时: {:?}", event_ack_time - wallet_find_time);

        let res = ApiCollectRepo::get_api_collect_by_trade_no(&api_funds_pool, &req.trade_no).await;
        let tx_check_time = Instant::now();
        tracing::info!(trade_no=%req.trade_no, "检查交易记录, 耗时: {:?}", tx_check_time - event_ack_time);

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

        // 可能发交易
        tracing::info!(trade_no=%req.trade_no, "准备获取全局句柄");
        let handles_time = Instant::now();
        let handles = crate::context::CONTEXT.get().unwrap().get_global_handles().await;
        tracing::info!(trade_no=%req.trade_no, "获取全局句柄, 耗时: {:?}", handles_time.elapsed());

        if let Some(handles) = handles.upgrade() {
            tracing::info!(trade_no=%req.trade_no, "提交交易到处理队列");
            let submit_time = Instant::now();
            handles.get_global_processed_collect_tx_handle().submit_tx(&req.trade_no).await?;
            tracing::info!(trade_no=%req.trade_no, "交易提交成功, 耗时: {:?}", submit_time.elapsed());
        } else {
            tracing::error!(trade_no=%req.trade_no, "无法获取全局句柄，交易提交失败");
        }

        tracing::info!(trade_no=%req.trade_no, "归集交易v2处理完成, 总耗时: {:?}", start_time.elapsed());
        Ok(())
    }

    pub async fn recover(trade_no: &str) -> Result<(), crate::error::service::ServiceError> {
        let start_time = Instant::now();
        tracing::info!(trade_no=%trade_no, "开始恢复归集交易, start_time: {:?}", start_time);

        let pool = crate::context::CONTEXT.get().unwrap().api_funds_pool()?;
        tracing::info!(trade_no=%trade_no, "更新交易状态为初始化");
        let update_time = Instant::now();
        ApiCollectRepo::update_api_collect_next_status_and_err(
            &pool,
            trade_no,
            ApiCollectStatus::InsufficientBalance,
            ApiCollectStatus::Init,
            0,
            "recover",
        )
        .await?;
        tracing::info!(trade_no=%trade_no, "交易状态更新成功, 耗时: {:?}", update_time.elapsed());

        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        // 发送交易费用结果确认 - 不需要幂等性检查，因为TxFeeRes ACK字段不在collect实体中
        let trans_event_req =
            TransEventAckReq::new(trade_no, TransType::Col, TransAckType::TxFeeRes);
        tracing::info!(trade_no=%trade_no, "发送交易费用结果确认");
        let event_ack_time = Instant::now();
        backend.trans_event_ack(&trans_event_req).await?;
        tracing::info!(trade_no=%trade_no, "交易费用结果确认发送成功, 耗时: {:?}", event_ack_time.elapsed());

        let handles_time = Instant::now();
        let handles = crate::context::CONTEXT.get().unwrap().get_global_handles().await;
        tracing::info!(trade_no=%trade_no, "获取全局句柄, 耗时: {:?}", handles_time.elapsed());

        if let Some(handles) = handles.upgrade() {
            tracing::info!(trade_no=%trade_no, "重新提交交易到处理队列");
            let submit_time = Instant::now();
            handles.get_global_processed_collect_tx_handle().submit_tx(trade_no).await?;
            tracing::info!(trade_no=%trade_no, "交易重新提交成功, 耗时: {:?}", submit_time.elapsed());
        } else {
            tracing::error!(trade_no=%trade_no, "无法获取全局句柄，交易重新提交失败");
        };

        tracing::info!(trade_no=%trade_no, "归集交易恢复完成, 总耗时: {:?}", start_time.elapsed());
        Ok(())
    }

    pub async fn confirm_tx(
        trade_no: &str,
        status: bool,
        fail_type: i32,
    ) -> Result<(), crate::error::service::ServiceError> {
        let start_time = Instant::now();
        tracing::info!(trade_no=%trade_no, "开始确认归集交易, 状态: {}, 失败类型: {}, start_time: {:?}", status, fail_type, start_time);

        let pool = crate::context::CONTEXT.get().unwrap().api_funds_pool()?;
        tracing::info!(trade_no=%trade_no, "查询交易记录");
        let query_time = Instant::now();
        let tx = match ApiCollectRepo::get_api_collect_by_trade_no(&pool, trade_no).await {
            Ok(tx) => tx,
            Err(e) => {
                tracing::error!(trade_no=%trade_no, "查询交易记录失败: {:?}", e);
                let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
                backend_api
                    .trans_event_ack(&TransEventAckReq::new(
                        trade_no,
                        TransType::Col,
                        TransAckType::TxRes,
                    ))
                    .await?;
                return Ok(());
            }
        };
        tracing::info!(trade_no=%trade_no, "找到交易记录, 当前状态: {:?}, 耗时: {:?}", tx.status, query_time.elapsed());

        let update_time = Instant::now();
        if status {
            if tx.status == ApiCollectStatus::Success
                || tx.status == ApiCollectStatus::ConfirmSuccessReport
            {
                tracing::warn!(trade_no=%trade_no, "归集交易确认重复");
                return Ok(());
            }
            let rows_affected = ApiCollectRepo::update_api_collect_next_status(
                &pool,
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
                return Err(ServiceError::Business(ApiWalletError::StatusNotMatched.into()));
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
                    &pool,
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
                    return Err(ServiceError::Business(ApiWalletError::StatusNotMatched.into()));
                }
                tracing::info!(trade_no=%trade_no, "交易状态更新成功");
            } else {
                tracing::info!(trade_no=%trade_no, "更新交易状态为失败");
                let rows_affected = ApiCollectRepo::update_api_collect_next_status(
                    &pool,
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
                    return Err(ServiceError::Business(ApiWalletError::StatusNotMatched.into()));
                }
                tracing::info!(trade_no=%trade_no, "交易状态更新成功");
            }
        }
        tracing::info!(trade_no=%trade_no, "更新交易状态, 耗时: {:?}", update_time.elapsed());

        let handles_time = Instant::now();
        let handles = crate::context::CONTEXT.get().unwrap().get_global_handles().await;
        tracing::info!(trade_no=%trade_no, "获取全局句柄, 耗时: {:?}", handles_time.elapsed());

        if let Some(handles) = handles.upgrade() {
            tracing::info!(trade_no=%trade_no, "提交确认报告到处理队列");
            let submit_time = Instant::now();
            handles
                .get_global_processed_collect_tx_handle()
                .submit_confirm_report_tx(trade_no)
                .await?;
            tracing::info!(trade_no=%trade_no, "确认报告提交成功, 耗时: {:?}", submit_time.elapsed());
        } else {
            tracing::error!(trade_no=%trade_no, "无法获取全局句柄，确认报告提交失败");
        }

        tracing::info!(trade_no=%trade_no, "归集交易确认完成, 总耗时: {:?}", start_time.elapsed());
        Ok(())
    }
}
