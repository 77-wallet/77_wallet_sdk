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

        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        // 获取钱包
        tracing::info!(trade_no=%req.trade_no, "查询钱包信息");
        let wallet_find_time = Instant::now();
        let wallet = ApiWalletRepo::find_by_uid(&pool, &req.uid).await?.ok_or(
            BusinessError::ApiWallet(ApiWalletError::Wallet(WalletError::NotFound.into())),
        )?;
        tracing::info!(trade_no=%req.trade_no, "找到钱包: name={}, 耗时: {:?}", wallet.name, wallet_find_time - start_time);

        tracing::info!(trade_no=%req.trade_no, "检查手续费交易记录");
        let tx_check_time = Instant::now();
        let res = ApiFeeRepo::get_api_fee_by_trade_no(&pool, &req.trade_no).await;
        tracing::info!(trade_no=%req.trade_no, "检查交易记录, 耗时: {:?}", tx_check_time - wallet_find_time);

        if res.is_err() {
            tracing::info!(trade_no=%req.trade_no, "未找到现有手续费交易记录，开始插入新记录");
            let insert_time = Instant::now();
            ApiFeeRepo::upsert_api_fee(
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
                req.trade_type,
            )
            .await?;
            tracing::info!(trade_no=%req.trade_no, "落盘手续费: 耗时: {:?}", insert_time.elapsed());

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
        }

        // fix: 2186
        tracing::info!(trade_no=%req.trade_no, "发送交易事件确认请求");
        let event_ack_time = Instant::now();
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let trans_event_req =
            TransEventAckReq::new(&req.trade_no, TransType::ColFee, TransAckType::Tx);
        backend.trans_event_ack(&trans_event_req).await?;
        tracing::info!(trade_no=%req.trade_no, "交易事件确认成功, 耗时: {:?}", event_ack_time - tx_check_time);

        tracing::info!(trade_no=%req.trade_no, "准备获取全局句柄");
        let handles_time = Instant::now();
        let handles = crate::context::CONTEXT.get().unwrap().get_global_handles().await;
        tracing::info!(trade_no=%req.trade_no, "获取全局句柄, 耗时: {:?}", handles_time.elapsed());

        if let Some(handles) = handles.upgrade() {
            tracing::info!(trade_no=%req.trade_no, "提交手续费交易到处理队列");
            let submit_time = Instant::now();
            handles.get_global_processed_fee_tx_handle().submit_tx(&req.trade_no).await?;
            tracing::info!(trade_no=%req.trade_no, "手续费交易提交成功, 耗时: {:?}", submit_time.elapsed());
        } else {
            tracing::error!(trade_no=%req.trade_no, "无法获取全局句柄，手续费交易提交失败");
        }

        tracing::info!(trade_no=%req.trade_no, "手续费交易处理完成, 总耗时: {:?}", start_time.elapsed());
        Ok(())
    }

    pub async fn confirm_tx(trade_no: &str, status: bool) -> Result<(), ServiceError> {
        let start_time = Instant::now();
        tracing::info!(trade_no=%trade_no, "开始确认手续费交易, 状态: {}, start_time: {:?}", status, start_time);

        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
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

        tracing::info!(trade_no=%trade_no, "准备获取全局句柄");
        let handles_time = Instant::now();
        let handles = crate::context::CONTEXT.get().unwrap().get_global_handles().await;
        tracing::info!(trade_no=%trade_no, "获取全局句柄, 耗时: {:?}", handles_time.elapsed());

        if let Some(handles) = handles.upgrade() {
            tracing::info!(trade_no=%trade_no, "提交手续费确认报告到处理队列");
            let submit_time = Instant::now();
            handles.get_global_processed_fee_tx_handle().submit_confirm_report_tx(trade_no).await?;
            tracing::info!(trade_no=%trade_no, "手续费确认报告提交成功, 耗时: {:?}", submit_time.elapsed());
        } else {
            tracing::error!(trade_no=%trade_no, "无法获取全局句柄，手续费确认报告提交失败");
        }

        tracing::info!(trade_no=%trade_no, "手续费交易确认完成, 总耗时: {:?}", start_time.elapsed());
        Ok(())
    }
}
