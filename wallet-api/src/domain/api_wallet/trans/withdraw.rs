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
use wallet_database::{
    entities::{api_trade_type::ApiTradeType, api_withdraw::ApiWithdrawStatus},
    repositories::api_wallet::{wallet::ApiWalletRepo, withdraw::ApiWithdrawRepo},
};
use wallet_transport_backend::request::api_wallet::transaction::{
    TransAckType, TransEventAckReq, TransType,
};

pub struct ApiWithdrawDomain {}

impl ApiWithdrawDomain {
    pub(crate) async fn withdraw(
        req: &ApiWithdrawReq,
    ) -> Result<(), crate::error::service::ServiceError> {
        // 验证金额是否需要输入密码
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        // 获取钱包
        tracing::info!(trade_no=%req.trade_no, "查询钱包信息");
        let wallet = ApiWalletRepo::find_by_uid(pool.clone(), &req.uid).await?.ok_or(
            BusinessError::ApiWallet(ApiWalletError::Wallet(WalletError::NotFound.into())),
        )?;

        let init_status =
            if req.audit == 1 { ApiWithdrawStatus::AuditPass } else { ApiWithdrawStatus::Init };
        let res = ApiWithdrawRepo::get_api_withdraw_by_trade_no(
            &pool,
            &req.trade_no,
            ApiTradeType::Withdraw,
        )
        .await;
        if res.is_err() {
            ApiWithdrawRepo::upsert_api_withdraw(
                &pool,
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
                "",
                init_status,
                ApiWithdrawStatus::InitOrder,
                "",
                "",
                None,
                "",
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

        // fix: 2186
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let trans_event_req = TransEventAckReq::new(&req.trade_no, TransType::Wd, TransAckType::Tx);
        backend.trans_event_ack(&trans_event_req).await?;

        ApiWithdrawRepo::update_api_withdraw_status_and_err(
            &pool,
            &req.trade_no,
            init_status,
            0,
            "",
        )
        .await?;

        // 可能发交易
        let handles = crate::context::CONTEXT.get().unwrap().get_global_handles().await;
        if let Some(handles) = handles.upgrade() {
            handles.get_global_processed_withdraw_tx_handle().submit_tx(&req.trade_no).await?;
        }
        Ok(())
    }

    pub async fn sign_withdrawal_order(
        trade_no: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        ApiWithdrawRepo::update_api_withdraw_status_and_err(
            &pool,
            trade_no,
            ApiWithdrawStatus::AuditPass,
            0,
            "",
        )
        .await?;
        Ok(())
    }

    pub async fn reject_withdrawal_order(
        trade_no: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        ApiWithdrawRepo::update_api_withdraw_status_and_err(
            &pool,
            trade_no,
            ApiWithdrawStatus::AuditReject,
            100,
            "rejected",
        )
        .await?;
        Ok(())
    }

    pub async fn confirm_tx(trade_no: &str, status: bool) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let tx =
            ApiWithdrawRepo::get_api_withdraw_by_trade_no(&pool, trade_no, ApiTradeType::Withdraw)
                .await?;
        if status {
            if (tx.status == ApiWithdrawStatus::Success
                || tx.status == ApiWithdrawStatus::ConfirmSuccessReport)
            {
                tracing::warn!(trade_no=%trade_no, "fee confirmation repeat");
                return Ok(());
            }
        } else {
            if (tx.status == ApiWithdrawStatus::Failure
                || tx.status == ApiWithdrawStatus::ConfirmFailureReport)
            {
                tracing::warn!(trade_no=%trade_no, "fee confirmation repeat");
                return Ok(());
            }
        }
        let next_status: ApiWithdrawStatus =
            if status { ApiWithdrawStatus::Success } else { ApiWithdrawStatus::Failure };

        let rows_affected = ApiWithdrawRepo::update_api_withdraw_next_status(
            &pool,
            trade_no,
            ApiWithdrawStatus::SendingTxReport,
            next_status,
        )
        .await?;
        if rows_affected != 1 {
            return Err(ServiceError::Business(ApiWalletError::StatusNotMatched.into()));
        }

        let handles = crate::context::CONTEXT.get().unwrap().get_global_handles().await;
        if let Some(handles) = handles.upgrade() {
            handles
                .get_global_processed_withdraw_tx_handle()
                .submit_confirm_report_tx(trade_no)
                .await?;
        }
        let data = NotifyEvent::Withdraw(WithdrawFront {
            uid: tx.uid.to_string(),
            from_addr: tx.from_addr.to_string(),
            to_addr: tx.to_addr.to_string(),
            value: tx.value.to_string(),
        });
        FrontendNotifyEvent::new(data).send().await?;

        Ok(())
    }
}
