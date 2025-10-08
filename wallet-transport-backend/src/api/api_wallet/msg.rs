use crate::{
    consts::endpoint::api_wallet::MSG_ACK, request::api_wallet::msg::MsgAckReq,
    response::BackendResponse, response_vo::api_wallet::chain::ApiChainListResp,
};

use crate::api::BackendApi;

impl BackendApi {
    // api钱包查询链列表
    pub async fn msg_ack(&self, req: MsgAckReq) -> Result<Option<()>, crate::Error> {
        let res = self.client.post(MSG_ACK).json(req).send::<BackendResponse>().await?;
        tracing::info!("res: {res:#?}");
        res.process(&self.aes_cbc_cryptor)
    }
}
