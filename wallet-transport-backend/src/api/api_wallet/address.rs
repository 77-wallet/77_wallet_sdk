use std::collections::HashMap;
use wallet_ecdh::GLOBAL_KEY;
use crate::{
    consts::endpoint::api_wallet::{ADDRESS_EXPAND_COMPLETE, ADDRESS_INIT, QUERY_ADDRESS_LIST},
    request::api_wallet::address::*,
    response_vo::api_wallet::address::UsedAddressListResp,
};

use crate::api::BackendApi;
use crate::api_request::{ApiBackendRequest};
use crate::api_response::ApiBackendResponse;

impl BackendApi {
    // 地址初始化
    pub async fn expand_address(&self, req: &ApiAddressInitReq) -> Result<(), crate::Error> {
        GLOBAL_KEY.is_exchange_shared_secret() ?;
        // 1. 加密
        let api_req = ApiBackendRequest::new(req)?;
        let res = self.client
            .post(ADDRESS_INIT)
            .json(api_req)
            .send::<ApiBackendResponse>().await?;
        tracing::info!("res: {res:#?}");
        res.process()
    }

    // 扩容完成上报
    pub async fn expand_address_complete(
        &self,
        uid: &str,
        serial_no: &str,
    ) -> Result<(), crate::Error> {
        GLOBAL_KEY.is_exchange_shared_secret() ?;
        let mut req = HashMap::new();
        req.insert("uid", uid);
        req.insert("serial_no", serial_no);
        let api_req = ApiBackendRequest::new( req)?;
        let res =
            self.client.post(ADDRESS_EXPAND_COMPLETE).json(api_req).send::<ApiBackendResponse>().await?;
        tracing::info!("[expand_address_complete] res: {res:#?}");
        res.process()
    }

    // 查询已使用的地址列表
    pub async fn query_used_address_list(
        &self,
        req: &AddressListReq,
    ) -> Result<UsedAddressListResp, crate::Error> {
        GLOBAL_KEY.is_exchange_shared_secret() ?;
        let api_req = ApiBackendRequest::new( req)?;
        let res = self
            .client
            .post(QUERY_ADDRESS_LIST)
            .json(api_req)
            .send::<ApiBackendResponse>()
            .await?;
        res.process()
    }
}
