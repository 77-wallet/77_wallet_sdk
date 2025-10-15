use crate::{
    domain::api_wallet::trans::{
        collect::ApiCollectDomain, fee::ApiFeeDomain, withdraw::ApiWithdrawDomain,
    },
    messaging::notify::{FrontendNotifyEvent, event::NotifyEvent},
};
use wallet_database::entities::{
    api_collect::ApiCollectStatus, api_fee::ApiFeeStatus, api_withdraw::ApiWithdrawStatus,
};
use wallet_transport_backend::request::api_wallet::msg::MsgAckReq;

// biz_type = AWM_ORDER_TRANS_RES
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AwmOrderTransResMsg {
    trade_no: String,
    /// 交易类型： 1 提币 / 2 归集 / 3 归集手续费交易
    #[serde(deserialize_with = "wallet_utils::serde_func::string_to_u32", serialize_with = "wallet_utils::serde_func::u32_to_string")]
    trade_type: u32,
    /// 交易结果： true 成功 /false 失败
    status: bool,
    uid: String,
}

// API钱包的订单结果消息
impl AwmOrderTransResMsg {
    pub(crate) async fn exec(
        &self,
        _msg_id: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        match self.trade_type {
            1 => self.withdraw().await?,
            2 => self.collect().await?,
            3 => self.transfer_fee().await?,
            _ => {}
        }

        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let mut msg_ack_req = MsgAckReq::default();
        msg_ack_req.push(_msg_id);
        backend.msg_ack(msg_ack_req).await?;
        let data = NotifyEvent::AwmOrderTransRes(self.to_owned());
        FrontendNotifyEvent::new(data).send().await?;
        Ok(())
    }

    pub(crate) async fn transfer_fee(&self) -> Result<(), crate::error::service::ServiceError> {
        let status: ApiFeeStatus =
            if self.status { ApiFeeStatus::Success } else { ApiFeeStatus::Failure };
        ApiFeeDomain::confirm_tx(&self.trade_no, status).await?;
        Ok(())
    }

    pub(crate) async fn collect(&self) -> Result<(), crate::error::service::ServiceError> {
        let status: ApiCollectStatus =
            if self.status { ApiCollectStatus::Success } else { ApiCollectStatus::Failure };
        ApiCollectDomain::confirm_tx(&self.trade_no, status).await?;
        Ok(())
    }

    pub(crate) async fn withdraw(&self) -> Result<(), crate::error::service::ServiceError> {
        let status: ApiWithdrawStatus =
            if self.status { ApiWithdrawStatus::Success } else { ApiWithdrawStatus::Failure };
        ApiWithdrawDomain::confirm_tx(&self.trade_no, status).await?;
        Ok(())
    }
}
