use crate::{consts::endpoint::api_wallet::MSG_ACK, request::api_wallet::msg::MsgAckReq};
use wallet_ecdh::GLOBAL_KEY;

use crate::{api::BackendApi, api_request::ApiBackendRequest, api_response::ApiBackendResponse};

impl BackendApi {
    // api钱包查询链列表
    pub async fn msg_ack(&self, req: MsgAckReq) -> Result<Option<()>, crate::Error> {
        GLOBAL_KEY.is_exchange_shared_secret()?;
        let api_req = ApiBackendRequest::new(req)?;
        let res = self.client.post(MSG_ACK).json(api_req).send::<ApiBackendResponse>().await?;
        tracing::info!("res: {res:#?}");
        res.process(MSG_ACK)
    }
}
