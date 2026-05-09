// messaging/mqtt/topics/api_wallet/trans_result.rs
use crate::{
    domain::api_wallet::trans::{
        collect::ApiCollectDomain, fee::ApiFeeDomain, withdraw::ApiWithdrawDomain,
    },
    messaging::notify::{FrontendNotifyEvent, event::NotifyEvent},
};
use tracing;
use wallet_database::{
    entities::{
        api_resource_delegation::ApiResourceDelegationResultStatus,
        api_resource_gate::ApiResourceGateResult,
    },
    repositories::api_wallet::{
        collect::ApiCollectRepo, fee::ApiFeeRepo, resource_delegation::ApiResourceDelegationRepo,
        resource_operation::ApiResourceOperationRepo, wallet::ApiWalletRepo,
        withdraw::ApiWithdrawRepo,
    },
};
use wallet_transport_backend::request::api_wallet::msg::MsgAckReq;

// biz_type = AWM_ORDER_TRANS_RES
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AwmOrderTransResMsg {
    trade_no: String,
    /// 交易类型： 1 提币 / 2 归集 / 3 归集手续费交易
    #[serde(
        deserialize_with = "wallet_utils::serde_func::string_to_u32",
        serialize_with = "wallet_utils::serde_func::u32_to_string"
    )]
    trade_type: u32,
    /// 交易结果： true 成功 /false 失败
    status: bool,
    /// 订单结失败型：0 默认，无意义 /  1 交易正常失败 / 2 手续费失败
    fail_type: Option<i32>,
    uid: String,
}

// API钱包的订单结果消息
impl AwmOrderTransResMsg {
    pub(crate) async fn exec(
        &self,
        _msg_id: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        tracing::info!(
            msg_id = %_msg_id,
            trade_no = %self.trade_no,
            trade_type = %self.trade_type,
            status = %self.status,
            fail_type = ?self.fail_type,
            "Received AwmOrderTransResMsg"
        );

        tracing::info!(
            msg_id = %_msg_id,
            trade_no = %self.trade_no,
            trade_type = %self.trade_type,
            phase = "pre_ack_local_process",
            "AwmOrderTransResMsg processing before backend msg ack"
        );
        if let Err(e) = self.check_uid().await {
            tracing::warn!(
                msg_id = %_msg_id,
                trade_no = %self.trade_no,
                trade_type = %self.trade_type,
                status = %self.status,
                fail_type = ?self.fail_type,
                error = %e,
                phase = "pre_ack_local_process",
                "AwmOrderTransResMsg check_uid failed (message will NOT be acked; waiting resend)"
            );
            return Err(e);
        }

        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let mut msg_ack_req = MsgAckReq::default();
        msg_ack_req.push(_msg_id);
        tracing::info!(
            msg_id = %_msg_id,
            trade_no = %self.trade_no,
            trade_type = %self.trade_type,
            phase = "ack_backend_msg",
            "AwmOrderTransResMsg local processing done; acking backend message"
        );
        backend.msg_ack(msg_ack_req).await?;
        tracing::info!(
            msg_id = %_msg_id,
            trade_no = %self.trade_no,
            trade_type = %self.trade_type,
            phase = "ack_backend_msg",
            "AwmOrderTransResMsg acked"
        );

        let data = NotifyEvent::AwmOrderTransRes(self.to_owned());
        FrontendNotifyEvent::new(data).send().await?;
        Ok(())
    }

    pub(crate) async fn check_uid(&self) -> Result<(), crate::error::service::ServiceError> {
        // tracing::info!("临时这样做");
        // return Ok(());

        let pool = crate::context::CONTEXT.get().unwrap().api_wallet_pool()?;
        let res = ApiWalletRepo::find_by_uid(&pool, &self.uid).await?;
        if res.is_some() {
            // ✅ 强顺序屏障：先持久化“已收到 SER TxRes”事实，再进入 confirm_tx 路径
            // ⚠️ 若此处失败，必须返回错误并禁止 ack MQTT（让其重投）
            let api_transaction_pool =
                crate::context::CONTEXT.get().unwrap().api_transaction_pool()?;
            match self.trade_type {
                1 => {
                    ApiWithdrawRepo::update_tx_res_received_at(
                        &api_transaction_pool,
                        &self.trade_no,
                    )
                    .await?;
                    self.withdraw().await?;
                }
                2 => {
                    ApiCollectRepo::update_tx_res_received_at(
                        &api_transaction_pool,
                        &self.trade_no,
                    )
                    .await?;
                    let fail_type = self.fail_type.unwrap_or(0);
                    self.collect(fail_type).await?;
                }
                3 => {
                    ApiFeeRepo::update_tx_res_received_at(&api_transaction_pool, &self.trade_no)
                        .await?;
                    self.transfer_fee().await?;
                }
                4 => {
                    self.resource_operation_result(&api_transaction_pool).await?;
                }
                5 => {
                    self.collect_resource_delegation_result(&api_transaction_pool).await?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    async fn resource_operation_result(
        &self,
        api_transaction_pool: &wallet_database::ApiTransactionDbPool,
    ) -> Result<(), crate::error::service::ServiceError> {
        let result_status = if self.status { "success" } else { "fail" };
        let result_payload = wallet_utils::serde_func::serde_to_string(self).ok();
        ApiResourceOperationRepo::mark_result_received(
            api_transaction_pool,
            &self.trade_no,
            result_status,
            self.fail_type.map(i64::from),
            None,
            None,
            result_payload.as_deref(),
        )
        .await?;

        Ok(())
    }

    async fn collect_resource_delegation_result(
        &self,
        api_transaction_pool: &wallet_database::ApiTransactionDbPool,
    ) -> Result<(), crate::error::service::ServiceError> {
        let result_status = if self.status {
            ApiResourceDelegationResultStatus::Success
        } else {
            ApiResourceDelegationResultStatus::Fail
        };
        let result_payload = wallet_utils::serde_func::serde_to_string(self).ok();
        ApiResourceDelegationRepo::mark_result_received(
            api_transaction_pool,
            &self.trade_no,
            result_status,
            self.fail_type.map(i64::from),
            None,
            None,
            result_payload.as_deref(),
        )
        .await?;

        let resource_task = ApiResourceDelegationRepo::get_by_resource_trade_no(
            api_transaction_pool,
            &self.trade_no,
        )
        .await?;

        if self.status {
            if let Some(origin_trade_no) = resource_task.origin_trade_no.as_deref() {
                ApiCollectRepo::mark_resource_released(
                    api_transaction_pool,
                    origin_trade_no,
                    ApiResourceGateResult::PlatformDelegateSuccess,
                )
                .await?;
                tracing::info!(
                    resource_trade_no = %self.trade_no,
                    origin_trade_no = %origin_trade_no,
                    "Collect resource gate released by platform delegation result"
                );

                if let Some(handles) =
                    crate::context::CONTEXT.get().unwrap().get_global_handles().await.upgrade()
                {
                    if let Some(shadow_system) =
                        handles.get_global_processed_collect_tx_handle().get_shadow_system()
                    {
                        if let Err(e) = shadow_system.trigger_collect(origin_trade_no).await {
                            tracing::warn!(
                                resource_trade_no = %self.trade_no,
                                origin_trade_no = %origin_trade_no,
                                "Trigger collect shadow failed after resource result, but continuing: {:?}",
                                e
                            );
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub(crate) async fn transfer_fee(&self) -> Result<(), crate::error::service::ServiceError> {
        ApiFeeDomain::confirm_tx(&self.trade_no, self.status).await.map_err(|e| {
            tracing::warn!(
                trade_no = %self.trade_no,
                trade_type = %self.trade_type,
                status = %self.status,
                error = %e,
                "ApiFeeDomain::confirm_tx failed for AwmOrderTransResMsg"
            );
            e
        })
    }

    pub(crate) async fn collect(
        &self,
        fail_type: i32,
    ) -> Result<(), crate::error::service::ServiceError> {
        ApiCollectDomain::confirm_tx(&self.trade_no, self.status, fail_type).await.map_err(|e| {
            tracing::warn!(
                trade_no = %self.trade_no,
                trade_type = %self.trade_type,
                status = %self.status,
                fail_type = %fail_type,
                error = %e,
                "ApiCollectDomain::confirm_tx failed for AwmOrderTransResMsg"
            );
            e
        })
    }

    pub(crate) async fn withdraw(&self) -> Result<(), crate::error::service::ServiceError> {
        ApiWithdrawDomain::confirm_tx(&self.trade_no, self.status).await.map_err(|e| {
            tracing::warn!(
                trade_no = %self.trade_no,
                trade_type = %self.trade_type,
                status = %self.status,
                fail_type = ?self.fail_type,
                error = %e,
                "ApiWithdrawDomain::confirm_tx failed for AwmOrderTransResMsg"
            );
            e
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wallet_database::{
        SqliteContext, entities::api_resource_operation::NewApiResourceOperation,
        repositories::api_wallet::resource_operation::ApiResourceOperationRepo,
    };

    #[tokio::test]
    async fn resource_operation_result_persists_trade_type_4_result_fact() -> anyhow::Result<()> {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_root = dir.path().to_string_lossy().to_string();
        let pool = SqliteContext::new(&db_root, Some("api_transaction.db"))
            .await?
            .into_transaction_db_pool()?;

        ApiResourceOperationRepo::upsert(
            &pool,
            NewApiResourceOperation::backend_stake("uid_1", "op_result_msg", "owner", "1"),
        )
        .await?;

        let msg = AwmOrderTransResMsg {
            trade_no: "op_result_msg".to_string(),
            trade_type: 4,
            status: false,
            fail_type: Some(2),
            uid: "uid_1".to_string(),
        };

        msg.resource_operation_result(&pool).await?;

        let got =
            ApiResourceOperationRepo::get_by_resource_trade_no(&pool, "op_result_msg").await?;
        assert_eq!(got.result_status.as_deref(), Some("fail"));
        assert_eq!(got.fail_type, Some(2));
        assert!(got.result_received_at.is_some());
        assert!(got.result_payload.as_deref().unwrap_or_default().contains("op_result_msg"));
        assert!(got.result_ack_sent_at.is_none());

        Ok(())
    }
}
