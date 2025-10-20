use crate::{
    consts::endpoint::{
        api_wallet::{
            APP_ID_BIND, APP_ID_UNBIND, APPID_IMPORT_WALLET, APPID_WITHDRAWAL_WALLET_CHANGE,
            INIT_API_WALLET, QUERY_UID_BIND_INFO, QUERY_WALLET_ACTIVATION_CONFIG,
            SAVE_WALLET_ACTIVATION_CONFIG,
        },
        old_wallet::OLD_KEYS_UID_CHECK,
    },
    request::api_wallet::wallet::{
        AppIdImportReq, BindAppIdReq, InitApiWalletReq, SaveWalletActivationConfigReq,
        UnBindAppIdReq,
    },
    response::BackendResponse,
    response_vo::api_wallet::wallet::{
        KeysUidCheckRes, QueryUidBindInfoRes, QueryWalletActivationInfoResp,
    },
};
use std::collections::HashMap;
use wallet_ecdh::GLOBAL_KEY;

use crate::{
    Error::Backend, api::BackendApi, api_request::ApiBackendRequest,
    api_response::ApiBackendResponse,
};

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

    /// 钱包与 appId 绑定
    pub async fn wallet_bind_appid(&self, req: &BindAppIdReq) -> Result<(), crate::Error> {
        GLOBAL_KEY.is_exchange_shared_secret()?;
        let api_req = ApiBackendRequest::new(req)?;
        let res = self.client.post(APP_ID_BIND).json(api_req).send::<ApiBackendResponse>().await?;
        res.process::<()>(APP_ID_BIND)?;
        Ok(())
    }

    // 钱包与 appId 解绑
    pub async fn wallet_unbind_appid(&self, req: &UnBindAppIdReq) -> Result<(), crate::Error> {
        GLOBAL_KEY.is_exchange_shared_secret()?;
        let api_req = ApiBackendRequest::new(req)?;
        let res =
            self.client.post(APP_ID_UNBIND).json(api_req).send::<ApiBackendResponse>().await?;

        res.process::<()>(APP_ID_UNBIND)?;
        Ok(())
    }

    /// 设置UID为API钱包
    pub async fn init_api_wallet(&self, req: InitApiWalletReq) -> Result<(), crate::Error> {
        GLOBAL_KEY.is_exchange_shared_secret()?;
        let api_req = ApiBackendRequest::new(req)?;
        let res =
            self.client.post(INIT_API_WALLET).json(api_req).send::<ApiBackendResponse>().await?;

        res.process::<()>(INIT_API_WALLET)?;
        Ok(())
    }

    /// 保存钱包激活配置
    pub async fn save_wallet_activation_config(
        &self,
        req: SaveWalletActivationConfigReq,
    ) -> Result<(), crate::Error> {
        GLOBAL_KEY.is_exchange_shared_secret()?;
        let api_req = ApiBackendRequest::new(req)?;
        let res = self
            .client
            .post(SAVE_WALLET_ACTIVATION_CONFIG)
            .json(api_req)
            .send::<ApiBackendResponse>()
            .await?;

        res.process::<()>(SAVE_WALLET_ACTIVATION_CONFIG)?;
        Ok(())
    }

    /// 查询钱包激活信息
    pub async fn query_wallet_activation_info(
        &self,
        uid: &str,
    ) -> Result<QueryWalletActivationInfoResp, crate::Error> {
        GLOBAL_KEY.is_exchange_shared_secret()?;
        let mut req = HashMap::new();
        req.insert("uid", uid);
        let api_req = ApiBackendRequest::new(req)?;
        let res = self
            .client
            .post(QUERY_WALLET_ACTIVATION_CONFIG)
            .json(api_req)
            .send::<ApiBackendResponse>()
            .await?;

        let opt = res.process(QUERY_WALLET_ACTIVATION_CONFIG)?;
        opt.ok_or(Backend(Some("no found list".to_string())))
    }

    /// 查询uid 绑定信息
    pub async fn query_uid_bind_info(
        &self,
        uid: &str,
    ) -> Result<QueryUidBindInfoRes, crate::Error> {
        GLOBAL_KEY.is_exchange_shared_secret()?;
        let mut req = HashMap::new();
        req.insert("uid", uid);
        let api_req = ApiBackendRequest::new(req)?;
        let res = self
            .client
            .post(QUERY_UID_BIND_INFO)
            .json(api_req)
            .send::<ApiBackendResponse>()
            .await?;
        let opt = res.process(QUERY_UID_BIND_INFO)?;
        opt.ok_or(Backend(Some("no found list".to_string())))
    }

    /// uid与appid的绑定
    pub async fn appid_withdrawal_wallet_change(
        &self,
        withdrawal_uid: &str,
        org_app_id: &str,
    ) -> Result<(), crate::Error> {
        GLOBAL_KEY.is_exchange_shared_secret()?;

        let res = self
            .client
            .post(APPID_WITHDRAWAL_WALLET_CHANGE)
            .json(serde_json::json!({
                "withdrawalUid": withdrawal_uid,
                "orgAppId": org_app_id
            }))
            .send::<ApiBackendResponse>()
            .await?;
        res.process::<()>(APPID_WITHDRAWAL_WALLET_CHANGE)?;
        Ok(())
    }

    pub async fn appid_import(&self, req: AppIdImportReq) -> Result<(), crate::Error> {
        GLOBAL_KEY.is_exchange_shared_secret()?;
        let api_req = ApiBackendRequest::new(req)?;
        let res = self
            .client
            .post(APPID_IMPORT_WALLET)
            .json(api_req)
            .send::<ApiBackendResponse>()
            .await?;
        res.process::<()>(APPID_IMPORT_WALLET)?;
        Ok(())
    }

    // pub async fn appid_sub_account_import(
    //     &self,
    //     sn: &str,
    //     recharge_uid: &str,
    // ) -> Result<(), crate::Error> {
    //     let res = self
    //         .client
    //         .post(APPID_IMPORT_SUB_ACCOUNT)
    //         .json(serde_json::json!({
    //             "sn": sn,
    //             "rechargeUid": recharge_uid
    //         }))
    //         .send::<BackendResponse>()
    //         .await?;

    //     res.process(&self.aes_cbc_cryptor)
    // }

    // // 绑定子账户钱包
    // pub async fn appid_sub_account_bind(
    //     &self,
    //     sn: &str,
    //     recharge_uid: &str,
    //     org_app_id: &str,
    // ) -> Result<(), crate::Error> {
    //     let res = self
    //         .client
    //         .post(APPID_SUB_ACCOUNT_BIND)
    //         .json(serde_json::json!({
    //             "sn": sn,
    //             "rechargeUid": recharge_uid,
    //             "orgAppId": org_app_id
    //         }))
    //         .send::<BackendResponse>()
    //         .await?;

    //     res.process(&self.aes_cbc_cryptor)
    // }
}
