use crate::{
    consts::endpoint::api_wallet::{ADDRESS_EXPAND_COMPLETE, ADDRESS_INIT, QUERY_ADDRESS_LIST},
    request::api_wallet::address::*,
    response::BackendResponse,
    response_vo::api_wallet::address::UsedAddressListResp,
};

use crate::api::BackendApi;

impl BackendApi {
    // 地址初始化
    pub async fn expand_address(&self, req: &ApiAddressInitReq) -> Result<(), crate::Error> {
        let req = serde_json::json!(req);
        tracing::info!("req: {}", req.to_string());

        let res = self.client.post(ADDRESS_INIT).json(req).send::<BackendResponse>().await?;
        tracing::info!("res: {res:#?}");
        res.process(&self.aes_cbc_cryptor)
    }

    // 扩容完成上报
    pub async fn expand_address_complete(
        &self,
        uid: &str,
        serial_no: &str,
    ) -> Result<(), crate::Error> {
        let req = serde_json::json!({
            "uid": uid,
            "serialNo": serial_no,
        });
        tracing::info!("[expand_address_complete] req: {}", req.to_string());

        let res =
            self.client.post(ADDRESS_EXPAND_COMPLETE).json(req).send::<BackendResponse>().await?;
        tracing::info!("[expand_address_complete] res: {res:#?}");
        res.process(&self.aes_cbc_cryptor)
    }

    // 查询已使用的地址列表
    pub async fn query_used_address_list(
        &self,
        req: &AddressListReq,
    ) -> Result<UsedAddressListResp, crate::Error> {
        let res = self
            .client
            .post(QUERY_ADDRESS_LIST)
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;
        res.process(&self.aes_cbc_cryptor)
    }
}
