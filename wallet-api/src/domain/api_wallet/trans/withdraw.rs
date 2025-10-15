use crate::{
    error::business::{BusinessError, api_wallet::ApiWalletError},
    messaging::notify::{FrontendNotifyEvent, api_wallet::WithdrawFront, event::NotifyEvent},
    request::api_wallet::trans::ApiWithdrawReq,
};
use tracing::event;
use wallet_database::{
    entities::api_withdraw::ApiWithdrawStatus,
    repositories::api_wallet::{
        account::ApiAccountRepo, wallet::ApiWalletRepo, withdraw::ApiWithdrawRepo,
    },
};
use wallet_transport_backend::request::api_wallet::{
    msg::{MsgAckItem, MsgAckReq},
    transaction::{TransAckType, TransEventAckReq, TransType},
};

pub struct ApiWithdrawDomain {}

impl ApiWithdrawDomain {
    pub(crate) async fn withdraw(
        req: &ApiWithdrawReq,
    ) -> Result<(), crate::error::service::ServiceError> {
        // 验证金额是否需要输入密码
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        // 获取钱包
        let wallet = ApiWalletRepo::find_by_uid(&pool, &req.uid)
            .await?
            .ok_or(BusinessError::ApiWallet(ApiWalletError::NotFound))?;

        let status =
            if req.audit == 1 { ApiWithdrawStatus::AuditPass } else { ApiWithdrawStatus::Init };
        let res = ApiWithdrawRepo::get_api_withdraw_by_trade_no(&pool, &req.trade_no).await;
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
                &req.symbol,
                &req.trade_no,
                req.trade_type,
                status,
            )
            .await?;
            tracing::info!("upsert_api_withdraw  ------------------- 5: {}", status);

            let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
            let trans_event_req =
                TransEventAckReq::new(&req.trade_no, TransType::Wd, TransAckType::Tx);
            backend.trans_event_ack(&trans_event_req).await?;

            let data = NotifyEvent::Withdraw(WithdrawFront {
                uid: req.uid.to_string(),
                from_addr: req.from.to_string(),
                to_addr: req.to.to_string(),
                value: req.value.to_string(),
            });
            FrontendNotifyEvent::new(data).send().await?;

            // 可能发交易
            if let Some(handles) =
                crate::context::CONTEXT.get().unwrap().get_global_handles().upgrade()
            {
                handles.get_global_processed_withdraw_tx_handle().submit_tx(&req.trade_no).await?;
            }
        }

        Ok(())
    }

    pub async fn sign_withdrawal_order(
        trade_no: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        ApiWithdrawRepo::update_api_withdraw_status(
            &pool,
            trade_no,
            ApiWithdrawStatus::AuditPass,
            "OK",
        )
        .await?;
        Ok(())
    }

    pub async fn reject_withdrawal_order(
        trade_no: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        ApiWithdrawRepo::update_api_withdraw_status(
            &pool,
            trade_no,
            ApiWithdrawStatus::AuditReject,
            "rejected",
        )
        .await?;
        Ok(())
    }

    pub async fn confirm_tx(
        trade_no: &str,
        status: ApiWithdrawStatus,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        ApiWithdrawRepo::update_api_withdraw_status(&pool, trade_no, status, "confirm").await?;

        if let Some(handles) = crate::context::CONTEXT.get().unwrap().get_global_handles().upgrade()
        {
            handles
                .get_global_processed_withdraw_tx_handle()
                .submit_confirm_report_tx(trade_no)
                .await?;
        }
        Ok(())
    }
}
