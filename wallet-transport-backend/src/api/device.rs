use serde_json::json;

use super::BackendApi;
use crate::{response::BackendResponse, response_vo::app::MinValueConfigResp};

impl BackendApi {
    pub async fn device_init(
        &self,

        req: &crate::request::DeviceInitReq,
    ) -> Result<Option<()>, crate::Error> {
        let res = self
            .client
            .post("device/init")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;

        res.process(&self.aes_cbc_cryptor)
    }

    pub async fn device_delete(
        &self,

        req: &crate::request::DeviceDeleteReq,
    ) -> Result<Option<()>, crate::Error> {
        let res = self
            .client
            .post("device/delete")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;

        res.process(&self.aes_cbc_cryptor)
    }

    pub async fn device_bind_address(
        &self,

        req: &crate::request::DeviceBindAddressReq,
    ) -> Result<Option<()>, crate::Error> {
        let res = self
            .client
            .post("device/bindAddress")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;

        res.process(&self.aes_cbc_cryptor)
    }

    pub async fn device_unbind_address(
        &self,

        req: &crate::request::DeviceBindAddressReq,
    ) -> Result<Option<()>, crate::Error> {
        let res = self
            .client
            .post("device/unBindAddress")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;

        res.process(&self.aes_cbc_cryptor)
    }

    pub async fn keys_init(
        &self,

        req: &crate::request::KeysInitReq,
    ) -> Result<Option<()>, crate::Error> {
        let res = self
            .client
            .post("keys/init")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;
        res.process(&self.aes_cbc_cryptor)
    }

    pub async fn keys_v2_init(
        &self,

        req: &crate::request::KeysInitReq,
    ) -> Result<Option<()>, crate::Error> {
        let res = self
            .client
            .post("keys/v2/init")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;
        res.process(&self.aes_cbc_cryptor)
    }

    pub async fn keys_update_wallet_name(
        &self,

        req: &crate::request::KeysUpdateWalletNameReq,
    ) -> Result<Option<()>, crate::Error> {
        let res = self
            .client
            .post("keys/updateWalletName")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;
        res.process(&self.aes_cbc_cryptor)
    }

    pub async fn keys_reset(&self, sn: &str) -> Result<Option<()>, crate::Error> {
        let res = self
            .client
            .post("keys/reset")
            .json(serde_json::json!({
                "sn": sn,
            }))
            .send::<BackendResponse>()
            .await?;
        res.process(&self.aes_cbc_cryptor)
    }

    // report filter min value config
    pub async fn save_send_msg_account(
        &self,

        req: Vec<crate::response_vo::app::SaveSendMsgAccount>,
    ) -> Result<(), crate::Error> {
        let req = json!({
            "tokenAmountInfo":req,
        });

        self.client
            .post("device/saveSendMsgAmount")
            .json(req)
            .send::<BackendResponse>()
            .await?
            .process(&self.aes_cbc_cryptor)
    }

    // fetch min config
    pub async fn fetch_min_config(&self, sn: String) -> Result<MinValueConfigResp, crate::Error> {
        let req = json!({
            "sn":sn
        });

        self.client
            .post("device/querySendMsgAmount")
            .json(req)
            .send::<BackendResponse>()
            .await?
            .process(&self.aes_cbc_cryptor)
    }

    pub async fn update_app_id(
        &self,

        req: &crate::request::UpdateAppIdReq,
    ) -> Result<(), crate::Error> {
        let res =
            self.client.post("device/updateAppId").json(req).send::<BackendResponse>().await?;
        res.process(&self.aes_cbc_cryptor)
    }
}
