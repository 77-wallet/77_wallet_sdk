use crate::{
    consts::endpoint::api_wallet::KEYS_UID_CHECK, response::BackendResponse,
    response_vo::api_wallet::wallet::KeysUidCheckRes,
};

use super::BackendApi;

impl BackendApi {
    // uid类型检查
    pub async fn keys_uid_check(&self, uid: &str) -> Result<KeysUidCheckRes, crate::Error> {
        let res = self
            .client
            .post(KEYS_UID_CHECK)
            .json(serde_json::json!({
                "uid": uid
            }))
            .send::<BackendResponse>()
            .await?;

        res.process(&self.aes_cbc_cryptor)
    }
}
