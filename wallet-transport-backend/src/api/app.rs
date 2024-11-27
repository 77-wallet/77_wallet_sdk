use super::BackendApi;
use crate::{request::LanguageInitReq, response::BackendResponse};

impl BackendApi {
    pub async fn app_install_save(
        &self,
        req: crate::request::AppInstallSaveReq,
    ) -> Result<std::collections::HashMap<String, serde_json::Value>, crate::Error> {
        let res = self
            .client
            .post("/app/install/save")
            .json(serde_json::json!(req))
            .send::<serde_json::Value>()
            .await?;
        let res: BackendResponse = wallet_utils::serde_func::serde_from_value(res)?;
        res.process()
    }

    pub async fn rpc_token(&self, client_id: &str) -> Result<String, crate::Error> {
        self.client
            .post("app/rpc/token")
            .json(serde_json::json!({"clientId":client_id}))
            .send::<BackendResponse>()
            .await?
            .process()
    }

    pub async fn version_view(
        &self,
        req: crate::request::VersionViewReq,
    ) -> Result<crate::response_vo::app::AppVersionRes, crate::Error> {
        let res = self
            .client
            .post("/version/view")
            .json(serde_json::json!(req))
            .send::<serde_json::Value>()
            .await?;
        let res: BackendResponse = wallet_utils::serde_func::serde_from_value(res)?;
        res.process()
    }

    pub async fn version_his_version(
        &self,
    ) -> Result<std::collections::HashMap<String, serde_json::Value>, crate::Error> {
        let res = self
            .client
            .post("/version/hisVersion")
            // .json(serde_json::json!(req))
            .send::<serde_json::Value>()
            .await?;
        let res: BackendResponse = wallet_utils::serde_func::serde_from_value(res)?;
        res.process()
    }

    pub async fn language_init(&self, req: LanguageInitReq) -> Result<(), crate::Error> {
        let res = self
            .client
            .post("/language/init")
            .json(req)
            .send::<serde_json::Value>()
            .await?;
        let res: BackendResponse = wallet_utils::serde_func::serde_from_value(res)?;
        res.process()
    }
}

#[cfg(test)]
mod test {

    use wallet_utils::init_test_log;

    use crate::{
        api::BackendApi,
        request::{AppInstallSaveReq, LanguageInitReq, VersionViewReq},
    };

    #[tokio::test]
    async fn test_app_install_save() {
        // let method = "POST";
        let base_url = "http://api.wallet.net";

        let req = AppInstallSaveReq {
            sn: "1".to_string(),
            channel: None,
            device_type: Some("ANDROID".to_string()),
        };
        let res = BackendApi::new(Some(base_url.to_string()), None)
            .unwrap()
            .app_install_save(req)
            .await
            .unwrap();

        println!("[test_chain_default_list] res: {res:?}");
    }

    #[tokio::test]
    async fn test_version_his_version() {
        // let method = "POST";
        init_test_log();
        let base_url = crate::consts::BASE_URL;

        let res = BackendApi::new(Some(base_url.to_string()), None)
            .unwrap()
            .version_his_version()
            .await
            .unwrap();

        println!("[test_chain_default_list] res: {res:?}");
    }

    #[tokio::test]
    async fn test_token() {
        // let method = "POST";
        init_test_log();
        let base_url = crate::consts::BASE_URL;

        let res = BackendApi::new(Some(base_url.to_string()), None)
            .unwrap()
            .rpc_token("52f2a22938c434f33c9904c05f6b1d83")
            .await
            .unwrap();

        println!("[test_chain_default_list] res: {res:?}");
    }

    #[tokio::test]
    async fn test_version_view() {
        init_test_log();

        // let method = "POST";
        let base_url = crate::consts::BASE_URL;

        let req = VersionViewReq {
            device_type: "ANDROID".to_string(),
        };
        let res = BackendApi::new(Some(base_url.to_string()), None)
            .unwrap()
            .version_view(req)
            .await
            .unwrap();

        println!("[test_chain_default_list] res: {res:?}");
    }

    #[tokio::test]
    async fn test_language_init() {
        init_test_log();

        // let method = "POST";
        let base_url = crate::consts::BASE_URL;

        let req = LanguageInitReq {
            client_id: "104.2.0.125C00".to_string(),
            lan: "CHINESE_SIMPLIFIED".to_string(),
        };
        let res = BackendApi::new(Some(base_url.to_string()), None)
            .unwrap()
            .language_init(req)
            .await
            .unwrap();

        println!("[test_language_init] res: {res:?}");
    }
}
