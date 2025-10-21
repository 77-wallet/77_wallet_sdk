use crate::{
    consts::endpoint::CLIENT_TASK_LOG_UPLOAD, request::LanguageInitReq, response::BackendResponse,
};

use crate::api::BackendApi;

impl BackendApi {
    pub async fn app_install_save(
        &self,
        req: crate::request::AppInstallSaveReq,
    ) -> Result<serde_json::Value, crate::Error> {
        let res = self
            .client
            .post("/app/install/save")
            .json(serde_json::json!(req))
            .send::<serde_json::Value>()
            .await?;
        let res: BackendResponse = wallet_utils::serde_func::serde_from_value(res)?;
        res.process(&self.aes_cbc_cryptor)
    }

    pub async fn app_install_download(&self) -> Result<String, crate::Error> {
        let res = self.client.post("/app/install/download").send::<serde_json::Value>().await?;
        let res: BackendResponse = wallet_utils::serde_func::serde_from_value(res)?;
        res.process(&self.aes_cbc_cryptor)
    }

    pub async fn mqtt_init(&self) -> Result<String, crate::Error> {
        let res = self.client.post("mqtt/init").send::<crate::response::BackendResponse>().await?;
        res.process(&self.aes_cbc_cryptor)
    }

    pub async fn rpc_token(&self, client_id: &str) -> Result<String, crate::Error> {
        self.client
            .post("app/rpc/token")
            .json(serde_json::json!({"clientId":client_id}))
            .send::<BackendResponse>()
            .await?
            .process(&self.aes_cbc_cryptor)
    }

    pub async fn version_view(
        &self,

        req: crate::request::VersionViewReq,
    ) -> Result<crate::response_vo::app::AppVersionRes, crate::Error> {
        let res = self
            .client
            .post("version/view")
            .json(serde_json::json!(req))
            .send::<serde_json::Value>()
            .await?;
        let res: BackendResponse = wallet_utils::serde_func::serde_from_value(res)?;
        res.process(&self.aes_cbc_cryptor)
    }

    pub async fn version_download_url(
        &self,

        url: &str,
    ) -> Result<crate::response_vo::app::AppVersionRes, crate::Error> {
        let res =
            self.client.get(&format!("version/download/{url}")).send::<serde_json::Value>().await?;
        let res: BackendResponse = wallet_utils::serde_func::serde_from_value(res)?;
        res.process(&self.aes_cbc_cryptor)
    }

    pub async fn language_init(&self, req: LanguageInitReq) -> Result<(), crate::Error> {
        let res = self.client.post("/language/init").json(req).send::<serde_json::Value>().await?;
        let res: BackendResponse = wallet_utils::serde_func::serde_from_value(res)?;
        res.process(&self.aes_cbc_cryptor)
    }

    pub async fn set_invite_code(
        &self,

        req: crate::request::SetInviteeStatusReq,
    ) -> Result<(), crate::Error> {
        let res = self
            .client
            .post("/device/editDeviceInviteeStatus")
            .json(req)
            .send::<serde_json::Value>()
            .await?;
        let res: BackendResponse = wallet_utils::serde_func::serde_from_value(res)?;
        res.process(&self.aes_cbc_cryptor)
    }

    pub async fn client_task_log_upload(
        &self,

        req: crate::request::ClientTaskLogUploadReq,
    ) -> Result<(), crate::Error> {
        let res =
            self.client.post(CLIENT_TASK_LOG_UPLOAD).json(req).send::<serde_json::Value>().await?;
        let res: BackendResponse = wallet_utils::serde_func::serde_from_value(res)?;
        res.process(&self.aes_cbc_cryptor)
    }
}
