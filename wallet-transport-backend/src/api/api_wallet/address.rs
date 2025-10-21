use crate::{
    consts::endpoint::api_wallet::{
        ADDRESS_EXPAND_COMPLETE, ADDRESS_INIT, QUERY_ADDRESS_LIST, QUERY_ASSET_LIST,
    },
    request::api_wallet::address::*,
    response_vo::api_wallet::address::{AssetsListRes, UsedAddressListResp},
};
use std::collections::HashMap;
use wallet_ecdh::GLOBAL_KEY;

use crate::{
    Error::Backend, api::BackendApi, api_request::ApiBackendRequest,
    api_response::ApiBackendResponse,
};

impl BackendApi {
    // 地址初始化
    pub async fn expand_address(&self, req: &ApiAddressInitReq) -> Result<(), crate::Error> {
        GLOBAL_KEY.is_exchange_shared_secret()?;
        // 1. 加密
        let api_req = ApiBackendRequest::new(req)?;
        let res = self.client.post(ADDRESS_INIT).json(api_req).send::<ApiBackendResponse>().await?;
        tracing::info!("res: {res:#?}");
        res.process::<()>(ADDRESS_INIT)?;
        Ok(())
    }

    // 扩容完成上报
    pub async fn expand_address_complete(
        &self,
        uid: &str,
        serial_no: &str,
    ) -> Result<(), crate::Error> {
        GLOBAL_KEY.is_exchange_shared_secret()?;
        let mut req = HashMap::new();
        req.insert("uid", uid);
        req.insert("serialNo", serial_no);
        let api_req = ApiBackendRequest::new(req)?;
        let res = self
            .client
            .post(ADDRESS_EXPAND_COMPLETE)
            .json(api_req)
            .send::<ApiBackendResponse>()
            .await?;
        tracing::info!("[expand_address_complete] res: {res:#?}");
        res.process::<()>(ADDRESS_EXPAND_COMPLETE)?;
        Ok(())
    }

    // 查询已使用的地址列表
    pub async fn query_used_address_list(
        &self,
        req: &AddressListReq,
    ) -> Result<UsedAddressListResp, crate::Error> {
        GLOBAL_KEY.is_exchange_shared_secret()?;
        let api_req = ApiBackendRequest::new(req)?;
        let res =
            self.client.post(QUERY_ADDRESS_LIST).json(api_req).send::<ApiBackendResponse>().await?;
        let opt: Option<UsedAddressListResp> = res.process(QUERY_ADDRESS_LIST)?;
        opt.ok_or(Backend(Some("no address list".to_string())))
    }

    pub async fn query_asset_list(
        &self,
        req: &AssetListReq,
    ) -> Result<AssetsListRes, crate::Error> {
        GLOBAL_KEY.is_exchange_shared_secret()?;
        let api_req = ApiBackendRequest::new(req)?;
        let res =
            self.client.post(QUERY_ASSET_LIST).json(api_req).send::<ApiBackendResponse>().await?;
        let opt: Option<AssetsListRes> = res.process(QUERY_ASSET_LIST)?;
        opt.ok_or(Backend(Some("no asset list".to_string())))
    }
}
