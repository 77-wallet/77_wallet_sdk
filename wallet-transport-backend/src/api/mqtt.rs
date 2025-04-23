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

    pub async fn query_unconfirm_msg(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: &crate::request::QueryUnconfirmMsgReq,
    ) -> Result<crate::response_vo::mqtt::UnconfirmMsgRes, crate::Error> {
        let res = self
            .client
            .post("sendMsg/queryUnConfirmMsg")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;

        res.process(aes_cbc_cryptor)
    }

    pub async fn get_unconfirm_by_msg_id(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: &crate::request::GetUnconfirmById,
    ) -> Result<crate::response_vo::mqtt::UnconfirmMsgResp, crate::Error> {
        let res = self
            .client
            .post("sendMsg/getMsgById")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;

        res.process(aes_cbc_cryptor)
    }
}
