use crate::{
    domain::api_wallet::trans::collect::ApiCollectDomain,
    messaging::notify::{FrontendNotifyEvent, event::NotifyEvent},
};
use wallet_transport_backend::request::api_wallet::msg::MsgAckReq;

// biz_type = AWM_CMD_FEE_RES
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AwmCmdFeeResMsg {
    trade_no: String,
    /// 交易类型： 1 提币 / 2 归集 / 3 归集手续费交易
    #[serde(deserialize_with = "wallet_utils::serde_func::string_to_u32")]
    trade_type: u32,
    /// 交易结果： true 成功 /false 失败
    status: bool,
    uid: String,
}

// API手续费结果事件
impl AwmCmdFeeResMsg {
    pub(crate) async fn exec(
        &self,
        _msg_id: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        if self.status {
            ApiCollectDomain::recover(&self.trade_no).await?;
        }

        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let mut msg_ack_req = MsgAckReq::default();
        msg_ack_req.push(_msg_id);
        backend.msg_ack(msg_ack_req).await?;

        let data = NotifyEvent::AwmCmdFeeRes(self.to_owned());
        FrontendNotifyEvent::new(data).send().await?;
        Ok(())
    }
}
