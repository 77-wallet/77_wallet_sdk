pub mod address;
pub mod audit;
pub mod strategy;
pub mod transaction;

use crate::{
    api::BackendApi,
    request::api_wallet::{WalletBindAppIdReq, WalletUnbindAppIdReq},
};

impl BackendApi {
    // 钱包与 appId 绑定
    pub async fn wallet_bind_appid(
        &self,
        req: &WalletBindAppIdReq,
    ) -> Result<Option<()>, crate::Error> {
        todo!()
    }

    // 钱包与 appId 解绑
    pub async fn wallet_unbind_appid(
        &self,
        req: &WalletUnbindAppIdReq,
    ) -> Result<Option<()>, crate::Error> {
        todo!()
    }
}
