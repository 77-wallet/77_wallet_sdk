use wallet_transport_backend::request::api_wallet::msg::MsgAckReq;

// biz_type = TRANS_RESULT
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AwmOrderTransResMsg {
    trade_no: String,
    /// 交易类型： 1 提币 / 2 归集 / 3 归集手续费交易
    trade_type: u8,
    /// 交易结果： true 成功 /false 失败
    status: bool,
    uid: String,
}

// 归集和提币
impl AwmOrderTransResMsg {
    pub(crate) async fn exec(
        &self,
        _msg_id: &str,
    ) -> Result<(), crate::error::service::ServiceError> {



        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let mut msg_ack_req = MsgAckReq::default();
        msg_ack_req.push(_msg_id);
        backend.msg_ack(msg_ack_req).await?;
        Ok(())
    }
}
