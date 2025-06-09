use super::BackendApi;
use crate::{request::LanguageInitReq, response::BackendResponse};

impl BackendApi {
    pub async fn app_install_save(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: crate::request::AppInstallSaveReq,
    ) -> Result<serde_json::Value, crate::Error> {
        let res = self
            .client
            .post("/app/install/save")
            .json(serde_json::json!(req))
            .send::<serde_json::Value>()
            .await?;
        let res: BackendResponse = wallet_utils::serde_func::serde_from_value(res)?;
        res.process(aes_cbc_cryptor)
    }

    pub async fn app_install_download(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
    ) -> Result<String, crate::Error> {
        let res = self
            .client
            .post("/app/install/download")
            .send::<serde_json::Value>()
            .await?;
        let res: BackendResponse = wallet_utils::serde_func::serde_from_value(res)?;
        res.process(aes_cbc_cryptor)
    }

    pub async fn mqtt_init(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
    ) -> Result<String, crate::Error> {
        let res = self
            .client
            .post("mqtt/init")
            .send::<crate::response::BackendResponse>()
            .await?;
        res.process(aes_cbc_cryptor)
    }

    pub async fn rpc_token(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        client_id: &str,
    ) -> Result<String, crate::Error> {
        self.client
            .post("app/rpc/token")
            .json(serde_json::json!({"clientId":client_id}))
            .send::<BackendResponse>()
            .await?
            .process(aes_cbc_cryptor)
    }

    pub async fn version_view(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: crate::request::VersionViewReq,
    ) -> Result<crate::response_vo::app::AppVersionRes, crate::Error> {
        let res = self
            .client
            .post("version/view")
            .json(serde_json::json!(req))
            .send::<serde_json::Value>()
            .await?;
        let res: BackendResponse = wallet_utils::serde_func::serde_from_value(res)?;
        res.process(aes_cbc_cryptor)
    }

    pub async fn version_download_url(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        url: &str,
    ) -> Result<crate::response_vo::app::AppVersionRes, crate::Error> {
        let res = self
            .client
            .get(&format!("version/download/{url}"))
            .send::<serde_json::Value>()
            .await?;
        let res: BackendResponse = wallet_utils::serde_func::serde_from_value(res)?;
        res.process(aes_cbc_cryptor)
    }

    pub async fn language_init(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: LanguageInitReq,
    ) -> Result<(), crate::Error> {
        let res = self
            .client
            .post("/language/init")
            .json(req)
            .send::<serde_json::Value>()
            .await?;
        let res: BackendResponse = wallet_utils::serde_func::serde_from_value(res)?;
        res.process(aes_cbc_cryptor)
    }

    pub async fn set_invite_code(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: crate::request::SetInviteeStatusReq,
    ) -> Result<(), crate::Error> {
        let res = self
            .client
            .post("/device/editDeviceInviteeStatus")
            .json(req)
            .send::<serde_json::Value>()
            .await?;
        let res: BackendResponse = wallet_utils::serde_func::serde_from_value(res)?;
        res.process(aes_cbc_cryptor)
    }
}
