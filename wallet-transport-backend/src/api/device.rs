use serde_json::json;

use super::BackendApi;
use crate::{response::BackendResponse, response_vo::app::MinValueConfigResp};

impl BackendApi {
    pub async fn device_init(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: &crate::request::DeviceInitReq,
    ) -> Result<Option<()>, crate::Error> {
        let res = self
            .client
            .post("device/init")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;

        res.process(aes_cbc_cryptor)
    }

    pub async fn device_delete(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: &crate::request::DeviceDeleteReq,
    ) -> Result<Option<()>, crate::Error> {
        let res = self
            .client
            .post("device/delete")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;

        res.process(aes_cbc_cryptor)
    }

    pub async fn device_bind_address(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: &crate::request::DeviceBindAddressReq,
    ) -> Result<Option<()>, crate::Error> {
        let res = self
            .client
            .post("device/bindAddress")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;

        res.process(aes_cbc_cryptor)
    }

    pub async fn device_unbind_address(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: &crate::request::DeviceBindAddressReq,
    ) -> Result<Option<()>, crate::Error> {
        let res = self
            .client
            .post("device/unBindAddress")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;

        res.process(aes_cbc_cryptor)
    }

    pub async fn keys_init(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: &crate::request::KeysInitReq,
    ) -> Result<Option<()>, crate::Error> {
        let res = self
            .client
            .post("keys/init")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;
        res.process(aes_cbc_cryptor)
    }

    // report filter min value config
    pub async fn save_send_msg_account(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
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
            .process(aes_cbc_cryptor)
    }

    // fetch min config
    pub async fn fetch_min_config(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        sn: String,
    ) -> Result<MinValueConfigResp, crate::Error> {
        let req = json!({
            "sn":sn
        });

        self.client
            .post("device/querySendMsgAmount")
            .json(req)
            .send::<BackendResponse>()
            .await?
            .process(aes_cbc_cryptor)
    }

    pub async fn update_app_id(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: &crate::request::UpdateAppIdReq,
    ) -> Result<(), crate::Error> {
        let res = self
            .client
            .post("device/updateAppId")
            .json(req)
            .send::<BackendResponse>()
            .await?;
        res.process(aes_cbc_cryptor)
    }
}
