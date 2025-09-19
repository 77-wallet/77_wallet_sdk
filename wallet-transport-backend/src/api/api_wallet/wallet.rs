use crate::{
    consts::endpoint::{
        api_wallet::{
            APP_ID_BIND, APP_ID_UNBIND, INIT_API_WALLET, QUERY_UID_BIND_INFO,
            QUERY_WALLET_ACTIVATION_CONFIG, SAVE_WALLET_ACTIVATION_CONFIG,
        },
        old_wallet::OLD_KEYS_UID_CHECK,
    },
    request::api_wallet::wallet::{BindAppIdReq, SaveWalletActivationConfigReq, UnBindAppIdReq},
    response::BackendResponse,
    response_vo::api_wallet::wallet::{KeysUidCheckRes, QueryWalletActivationInfoResp},
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

    /// 设置UID为API钱包
    pub async fn init_api_wallet(
        &self,
        recharge_uid: &str,
        withdraw_uid: &str,
    ) -> Result<(), crate::Error> {
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

    /// 保存钱包激活配置
    pub async fn save_wallet_activation_config(
        &self,
        req: SaveWalletActivationConfigReq,
    ) -> Result<(), crate::Error> {
        let res = self
            .client
            .post(SAVE_WALLET_ACTIVATION_CONFIG)
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;

        res.process(&self.aes_cbc_cryptor)
    }

    /// 查询钱包激活信息
    pub async fn query_wallet_activation_info(
        &self,
        uid: &str,
    ) -> Result<QueryWalletActivationInfoResp, crate::Error> {
        let res = self
            .client
            .post(QUERY_WALLET_ACTIVATION_CONFIG)
            .json(serde_json::json!({
                "uid": uid
            }))
            .send::<BackendResponse>()
            .await?;

        res.process(&self.aes_cbc_cryptor)
    }

    /// 查询uid 绑定信息
    pub async fn query_uid_bind_info(&self, uid: &str) -> Result<KeysUidCheckRes, crate::Error> {
        let res = self
            .client
            .post(QUERY_UID_BIND_INFO)
            .json(serde_json::json!({
                "uid": uid
            }))
            .send::<BackendResponse>()
            .await?;

        res.process(&self.aes_cbc_cryptor)
    }
}
