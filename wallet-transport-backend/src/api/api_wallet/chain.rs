use crate::{
    consts::endpoint::api_wallet::API_WALLET_CHAIN_LIST, response::BackendResponse,
    response_vo::api_wallet::chain::ApiChainListResp,
};

use crate::api::BackendApi;

impl BackendApi {
    // api钱包查询链列表
    pub async fn api_wallet_chain_list(
        &self,
        app_version_code: &str,
    ) -> Result<ApiChainListResp, crate::Error> {
        // ) -> Result<serde_json::Value, crate::Error> {
        let req = serde_json::json!({
            "appVersionCode": app_version_code,
        });
        tracing::info!("req: {}", req.to_string());

        let res =
            self.client.post(API_WALLET_CHAIN_LIST).json(req).send::<BackendResponse>().await?;
        tracing::info!("res: {res:#?}");
        // res.process(&self.aes_cbc_cryptor)
        res.process(&self.aes_cbc_cryptor)
    }
}
