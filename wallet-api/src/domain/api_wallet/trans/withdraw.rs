use crate::{
    error::{
        business::{
            BusinessError,
            api_wallet::{ApiWalletError, wallet::WalletError},
        },
        service::ServiceError,
    },
    messaging::notify::{FrontendNotifyEvent, api_wallet::WithdrawFront, event::NotifyEvent},
    request::api_wallet::trans::ApiWithdrawReq,
};
use chrono::Utc;
use wallet_database::{
    entities::{
        api_trade_type::ApiTradeType,
        api_withdraw::{ApiWithdrawStatus, ErrCode},
    },
    repositories::api_wallet::{wallet::ApiWalletRepo, withdraw::ApiWithdrawRepo},
};
use wallet_transport_backend::request::api_wallet::transaction::{
    TransAckType, TransEventAckReq, TransType,
};

pub struct ApiWithdrawDomain {}

#[derive(Debug)]
pub(crate) struct WithdrawConfirmOutcome {
    pub tx: wallet_database::entities::api_withdraw::ApiWithdrawEntity,
    pub should_notify: bool,
}

impl ApiWithdrawDomain {
    pub(crate) async fn withdraw(
        req: &ApiWithdrawReq,
    ) -> Result<(), crate::error::service::ServiceError> {
        // 获取数据库连接
        let ctx = crate::context::CONTEXT.get().unwrap();
        let core_pool = ctx.api_wallet_pool()?;
        let api_funds_pool = ctx.api_funds_pool()?;
        // 获取钱包
        tracing::info!(trade_no=%req.trade_no, "查询钱包信息");
        let wallet = ApiWalletRepo::find_by_uid(&core_pool, &req.uid).await?.ok_or(
            BusinessError::ApiWallet(ApiWalletError::Wallet(WalletError::NotFound.into())),
        )?;

        let init_status =
            if req.audit == 1 { ApiWithdrawStatus::AuditPass } else { ApiWithdrawStatus::Init };
        let res = ApiWithdrawRepo::get_api_withdraw_by_trade_no(
            &api_funds_pool,
            &req.trade_no,
            ApiTradeType::Withdraw,
        )
        .await;
        if res.is_err() {
            ApiWithdrawRepo::upsert_api_withdraw(
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
                ApiTradeType::Withdraw,
                0,
                None,
                init_status,
                ApiWithdrawStatus::InitOrder,
                "",
                "",
                None,
                None,
            )
            .await?;
            tracing::info!(trade_no=%req.trade_no, "upsert_api_withdraw (step 5): {}", init_status);

            let data = NotifyEvent::Withdraw(WithdrawFront {
                uid: req.uid.to_string(),
                from_addr: req.from.to_string(),
                to_addr: req.to.to_string(),
                value: req.value.to_string(),
            });
            FrontendNotifyEvent::new(data).send().await?;
        } else {
            tracing::warn!(trade_no=%req.trade_no, "withdraw tx found");
        }

        if req.audit == 1 {
            Self::sign_withdrawal_order(&req.trade_no).await?;
        }

        // fix: 2186 - 添加幂等性检查，防止重复发送 Tx ACK
        let (tx_ack_sent_at, _) =
            ApiWithdrawRepo::get_ack_times(&api_funds_pool, &req.trade_no).await?;
        if tx_ack_sent_at.is_none() {
            tracing::info!(trade_no=%req.trade_no, "Tx ACK 未发送，准备发送");
            let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
            let trans_event_req =
                TransEventAckReq::new(&req.trade_no, TransType::Wd, TransAckType::Tx);
            backend.trans_event_ack(&trans_event_req).await?;

            // 设置 Tx ACK 发送时间
            ApiWithdrawRepo::set_tx_ack_sent(&api_funds_pool, &req.trade_no).await?;
            tracing::info!(trade_no=%req.trade_no, "Tx ACK 发送成功");
        } else {
            tracing::warn!(trade_no=%req.trade_no, ?tx_ack_sent_at, "Tx ACK 已发送，跳过");
        }

        ApiWithdrawRepo::update_api_withdraw_status(&api_funds_pool, &req.trade_no, init_status)
            .await?;

        // 注意：在 v2 架构下，不再需要显式提交交易
        // Shadow Scanner 会在下一轮扫描中自动发现新记录并推进执行
        // 交易执行完全由事实驱动，而不是命令式触发

        // 立即触发一次 Shadow 推进（快速通道）
        if let Some(handles) =
            crate::context::CONTEXT.get().unwrap().get_global_handles().await.upgrade()
        {
            if let Some(shadow_system) =
                handles.get_global_processed_withdraw_tx_handle().get_shadow_system()
            {
                if let Err(e) = shadow_system.trigger_withdraw(&req.trade_no).await {
                    tracing::warn!(trade_no=%req.trade_no, "触发 Shadow 推进失败，但不影响流程: {:?}", e);
                } else {
                    tracing::info!(trade_no=%req.trade_no, "成功触发 Shadow 快速通道推进");
                }
            }
        }
        Ok(())
    }

    pub async fn sign_withdrawal_order(
        trade_no: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().api_funds_pool()?;
        // ApiWithdrawRepo::update_api_withdraw_status(&pool, trade_no, ApiWithdrawStatus::AuditPass)
        //     .await?;

        ApiWithdrawRepo::set_audit_passed(&pool, trade_no).await?;

        // 立即触发一次 Shadow 推进（快速通道）
        if let Some(handles) =
            crate::context::CONTEXT.get().unwrap().get_global_handles().await.upgrade()
        {
            if let Some(shadow_system) =
                handles.get_global_processed_withdraw_tx_handle().get_shadow_system()
            {
                if let Err(e) = shadow_system.trigger_withdraw(trade_no).await {
                    tracing::warn!(trade_no=%trade_no, "触发 Shadow 推进失败，但不影响流程: {:?}", e);
                } else {
                    tracing::info!(trade_no=%trade_no, "成功触发 Shadow 快速通道推进");
                }
            }
        }

        Ok(())
    }

    pub async fn reject_withdrawal_order(
        trade_no: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().api_funds_pool()?;
        // ApiWithdrawRepo::update_api_withdraw_status_and_err(
        //     &pool,
        //     trade_no,
        //     ApiWithdrawStatus::AuditReject,
        //     ErrCode::UnknownError,
        //     "rejected",
        // )
        // .await?;

        ApiWithdrawRepo::set_audit_rejected(&pool, trade_no, "rejected").await?;

        // 立即触发一次 Shadow 推进（快速通道）
        if let Some(handles) =
            crate::context::CONTEXT.get().unwrap().get_global_handles().await.upgrade()
        {
            if let Some(shadow_system) =
                handles.get_global_processed_withdraw_tx_handle().get_shadow_system()
            {
                if let Err(e) = shadow_system.trigger_withdraw(trade_no).await {
                    tracing::warn!(trade_no=%trade_no, "触发 Shadow 推进失败，但不影响流程: {:?}", e);
                } else {
                    tracing::info!(trade_no=%trade_no, "成功触发 Shadow 快速通道推进");
                }
            }
        }

        Ok(())
    }

    pub async fn confirm_tx(trade_no: &str, status: bool) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().api_funds_pool()?;
        let outcome = Self::confirm_tx_with_pool(&pool, trade_no, status).await?;

        // 注意：在 v2 架构下，不再需要显式提交确认报告
        // Shadow Scanner 会在下一轮扫描中自动发现状态变化并触发确认报告
        // 交易执行完全由事实驱动，而不是命令式触发

        // 立即触发一次 Shadow 推进（快速通道）
        if let Some(handles) =
            crate::context::CONTEXT.get().unwrap().get_global_handles().await.upgrade()
        {
            if let Some(shadow_system) =
                handles.get_global_processed_withdraw_tx_handle().get_shadow_system()
            {
                if let Err(e) = shadow_system.trigger_withdraw(trade_no).await {
                    tracing::warn!(trade_no=%trade_no, "触发 Shadow 推进失败，但不影响流程: {:?}", e);
                } else {
                    tracing::info!(trade_no=%trade_no, "成功触发 Shadow 快速通道推进");
                }
            }
        }

        // 仅在本次确实推进了新事实时才通知前端，避免重投导致重复业务侧效应
        if outcome.should_notify {
            let data = NotifyEvent::Withdraw(WithdrawFront {
                uid: outcome.tx.uid.to_string(),
                from_addr: outcome.tx.from_addr.to_string(),
                to_addr: outcome.tx.to_addr.to_string(),
                value: outcome.tx.value.to_string(),
            });
            FrontendNotifyEvent::new(data).send().await?;
        }

        Ok(())
    }

    pub(crate) async fn confirm_tx_with_pool(
        pool: &wallet_database::ApiFundsDbPool,
        trade_no: &str,
        status: bool,
    ) -> Result<WithdrawConfirmOutcome, ServiceError> {
        let mut tx = match ApiWithdrawRepo::get_api_withdraw_by_trade_no(
            pool,
            trade_no,
            ApiTradeType::Withdraw,
        )
        .await
        {
            Ok(tx) => tx,
            Err(e) => {
                tracing::warn!(
                    trade_no = %trade_no,
                    status = %status,
                    error = %e,
                    "withdraw confirm_tx: trade_no not found (will NOT ack)"
                );
                return Err(e.into());
            }
        };

        let mut should_notify = false;

        // ====== 必须先确保 transaction_time 事实存在，再做任何 repeat early return ======
        if tx.transaction_time.is_none() {
            let now = Utc::now().to_rfc3339();
            let rows = ApiWithdrawRepo::confirm_transaction_time_if_absent(pool, trade_no, &now)
                .await
                .map_err(|e| {
                    tracing::warn!(
                        trade_no = %trade_no,
                        status = %status,
                        error = %e,
                        "withdraw confirm_tx: confirm_transaction_time_if_absent failed (will NOT ack)"
                    );
                    e
                })?;

            if rows == 0 {
                tx = ApiWithdrawRepo::get_api_withdraw_by_trade_no(
                    pool,
                    trade_no,
                    ApiTradeType::Withdraw,
                )
                .await?;
                if tx.transaction_time.is_none() {
                    tracing::warn!(
                        trade_no = %trade_no,
                        status = %status,
                        "withdraw confirm_tx: expected transaction_time to be set, but still NULL after retry (will NOT ack)"
                    );
                    return Err(crate::error::system::SystemError::Internal(
                        "transaction_time still NULL after confirm_transaction_time_if_absent"
                            .to_string(),
                    )
                    .into());
                }
            } else {
                should_notify = true;
                tx = ApiWithdrawRepo::get_api_withdraw_by_trade_no(
                    pool,
                    trade_no,
                    ApiTradeType::Withdraw,
                )
                .await?;
            }
        }

        // repeat 判定（在 ensure transaction_time 之后）
        if status {
            if tx.status == ApiWithdrawStatus::Success
                || tx.status == ApiWithdrawStatus::ConfirmSuccessReport
            {
                tracing::warn!(trade_no=%trade_no, "withdraw confirmation repeat");
                return Ok(WithdrawConfirmOutcome { tx, should_notify });
            }

            // 写入【事实】：链上成功
            let rows = ApiWithdrawRepo::set_chain_success(pool, trade_no).await?;
            if rows > 0 {
                should_notify = true;
            }
            tracing::info!(trade_no=%trade_no, "设置链上成功事实");
        } else {
            if tx.status == ApiWithdrawStatus::Failure
                || tx.status == ApiWithdrawStatus::ConfirmFailureReport
            {
                tracing::warn!(trade_no=%trade_no, "withdraw confirmation repeat");
                return Ok(WithdrawConfirmOutcome { tx, should_notify });
            }

            // 写入【事实】：链上失败
            let rows = ApiWithdrawRepo::set_chain_failed(pool, trade_no).await?;
            if rows > 0 {
                should_notify = true;
            }
            tracing::info!(trade_no=%trade_no, "设置链上失败事实");
        }

        // 返回最新事实快照
        let tx =
            ApiWithdrawRepo::get_api_withdraw_by_trade_no(pool, trade_no, ApiTradeType::Withdraw)
                .await?;

        Ok(WithdrawConfirmOutcome { tx, should_notify })
    }
}
