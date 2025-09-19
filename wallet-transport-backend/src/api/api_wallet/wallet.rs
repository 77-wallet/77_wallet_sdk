use crate::{
    consts::endpoint::{
        api_wallet::{
            APP_ID_BIND, APP_ID_UNBIND, CHECK_WITHDRAWAL_WALLET_ACTIVATED, INIT_API_WALLET,
        },
        old_wallet::OLD_KEYS_UID_CHECK,
    },
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
            .post(OLD_KEYS_UID_CHECK)
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

    pub async fn check_withdrawal_wallet_activated(
        &self,
        wallet_address: &str,
    ) -> Result<bool, crate::Error> {
        let res = self
            .client
            .post(CHECK_WITHDRAWAL_WALLET_ACTIVATED)
            .json(serde_json::json!({
                "wallet_address": wallet_address
            }))
            .send::<BackendResponse>()
            .await?;

        res.process(&self.aes_cbc_cryptor)
    }

    /// 设置UID为API钱包
    pub async fn init_api_wallet(
        &self,
        recharge_uid: &str,
        withdraw_uid: &str,
    ) -> Result<bool, crate::Error> {
        let res = self
            .client
            .post(INIT_API_WALLET)
            .json(serde_json::json!({
                "rechargeUid": recharge_uid,
                "withdrawUid": withdraw_uid
            }))
            .send::<BackendResponse>()
            .await?;

        res.process(&self.aes_cbc_cryptor)
    }
}
