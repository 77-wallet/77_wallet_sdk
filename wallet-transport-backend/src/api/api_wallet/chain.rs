use std::collections::HashMap;
use wallet_ecdh::GLOBAL_KEY;
use crate::{
    consts::endpoint::api_wallet::API_WALLET_CHAIN_LIST,
    response_vo::api_wallet::chain::ApiChainListResp,
};

use crate::api::BackendApi;
use crate::api_request::ApiBackendRequest;
use crate::api_response::ApiBackendResponse;

impl BackendApi {
    // api钱包查询链列表
    pub async fn api_wallet_chain_list(
        &self,
        app_version_code: &str,
    ) -> Result<ApiChainListResp, crate::Error> {
        tracing::info!("api_wallet_chain_list ------------------------");
        GLOBAL_KEY.is_exchange_shared_secret() ?;
        let mut req = HashMap::new();
        req.insert("appVersionCode", app_version_code);
        let api_req = ApiBackendRequest::new( req)?;

        let res =
            self.client.post(API_WALLET_CHAIN_LIST).json(api_req).send::<ApiBackendResponse>().await?;
        tracing::info!("res: {res:#?}");
        res.process()
    }
}
