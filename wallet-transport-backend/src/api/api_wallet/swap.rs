use crate::{
    api::BackendApi,
    consts::endpoint::api_wallet::INIT_SWAP,
    request::api_wallet::swap::{ApiInitSwapReq, ApiInitSwapResponse},
};

impl BackendApi {
    // 地址初始化
    pub async fn init_swap(
        &self,
        req: &ApiInitSwapReq,
    ) -> Result<ApiInitSwapResponse, crate::Error> {
        // 1. 加密
        let res = self.post_api_backend::<_, ApiInitSwapResponse>(INIT_SWAP, req).await?;
        tracing::info!("res: {res:#?}");
        res.ok_or(crate::Error::ApiBackend(999, Some("no init swap response".to_string())))
    }
}
