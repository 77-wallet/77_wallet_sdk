use super::BackendApi;
use crate::{response::BackendResponse, response_vo::app::FindConfigByKeyRes};

impl BackendApi {
    pub async fn find_config_by_key(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: crate::request::FindConfigByKey,
    ) -> Result<FindConfigByKeyRes, crate::Error> {
        let res = self
            .client
            .post("sys/config/findConfigByKey")
            .json(req)
            .send::<serde_json::Value>()
            .await?;
        let res: BackendResponse = wallet_utils::serde_func::serde_from_value(res)?;
        res.process(aes_cbc_cryptor)
    }
}
