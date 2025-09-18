use crate::{
    consts::endpoint::api_wallet::{APP_ID_BIND, APP_ID_UNBIND, KEYS_UID_CHECK},
    request::api_wallet::wallet::{BindAppIdReq, UnBindAppIdReq},
    response::BackendResponse,
    response_vo::api_wallet::wallet::KeysUidCheckRes,
};

use crate::api::BackendApi;

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

    // 钱包与 appId 绑定
    pub async fn wallet_bind_appid(&self, req: &BindAppIdReq) -> Result<(), crate::Error> {
        let res = self
            .client
            .post(APP_ID_BIND)
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;

        res.process(&self.aes_cbc_cryptor)
    }

    // 钱包与 appId 解绑
    pub async fn wallet_unbind_appid(&self, req: &UnBindAppIdReq) -> Result<(), crate::Error> {
        let res = self
            .client
            .post(APP_ID_UNBIND)
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;

        res.process(&self.aes_cbc_cryptor)
    }
}
