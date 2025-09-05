use crate::{
    consts::endpoint::api_wallet::QUERY_ADDRESS_LIST, request::api_wallet::address::*,
    response::BackendResponse, response_vo::api_wallet::address::UsedAddressListResp,
};

use super::BackendApi;

impl BackendApi {
    // 分配好的地址上传
    pub async fn upload_allocated_addresses(
        &self,
        req: &UploadAllocatedAddressesReq,
    ) -> Result<Option<()>, crate::Error> {
        todo!()
    }

    // 地址恢复
    pub async fn restore_addresses(
        &self,
        req: &RestoreAddressesReq,
    ) -> Result<Option<()>, crate::Error> {
        todo!()
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
