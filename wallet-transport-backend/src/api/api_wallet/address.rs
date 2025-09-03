use crate::{
    request::api_wallet::address::*, response_vo::api_wallet::address::UsedAddressListResp,
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
        req: &UsedAddressListReq,
    ) -> Result<UsedAddressListResp, crate::Error> {
        todo!()
    }
}
