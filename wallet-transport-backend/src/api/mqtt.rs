use super::BackendApi;
use crate::response::BackendResponse;

impl BackendApi {
    pub async fn send_msg_confirm(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: &crate::request::SendMsgConfirmReq,
    ) -> Result<(), crate::Error> {
        let res = self
            .client
            .post("sendMsg/confirm")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;

        res.process(aes_cbc_cryptor)
    }

    pub async fn send_msg_query_unconfirm_msg(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: &crate::request::SendMsgQueryUnconfirmMsgReq,
    ) -> Result<crate::response_vo::mqtt::SendMsgQueryUnconfirmMsgRes, crate::Error> {
        let res = self
            .client
            .post("sendMsg/queryUnConfirmMsg")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;

        res.process(aes_cbc_cryptor)
    }
}
