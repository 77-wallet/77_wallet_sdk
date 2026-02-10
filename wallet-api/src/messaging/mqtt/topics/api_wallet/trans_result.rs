// messaging/mqtt/topics/api_wallet/trans_result.rs
use crate::{
    domain::api_wallet::trans::{
        collect::ApiCollectDomain, fee::ApiFeeDomain, withdraw::ApiWithdrawDomain,
    },
    messaging::notify::{FrontendNotifyEvent, event::NotifyEvent},
};
use tracing;
use wallet_database::repositories::api_wallet::wallet::ApiWalletRepo;
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
        if let Err(e) = self.check_uid().await {
            tracing::warn!(
                msg_id = %_msg_id,
                trade_no = %self.trade_no,
                trade_type = %self.trade_type,
                status = %self.status,
                fail_type = ?self.fail_type,
                error = %e,
                "AwmOrderTransResMsg check_uid failed (message will NOT be acked)"
            );
            return Err(e);
        }
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let mut msg_ack_req = MsgAckReq::default();
        msg_ack_req.push(_msg_id);
        backend.msg_ack(msg_ack_req).await?;
        tracing::info!(
            msg_id = %_msg_id,
            trade_no = %self.trade_no,
            trade_type = %self.trade_type,
            "AwmOrderTransResMsg acked"
        );
        let data = NotifyEvent::AwmOrderTransRes(self.to_owned());
        FrontendNotifyEvent::new(data).send().await?;
        Ok(())
    }

    pub(crate) async fn check_uid(&self) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().api_wallet_pool()?;
        let res = ApiWalletRepo::find_by_uid(&pool, &self.uid).await?;
        if res.is_some() {
            match self.trade_type {
                1 => {
                    self.withdraw().await?;
                }
                2 => {
                    let fail_type = self.fail_type.unwrap_or(0);
                    self.collect(fail_type).await?;
                }
                3 => {
                    self.transfer_fee().await?;
                }
                _ => {}
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
