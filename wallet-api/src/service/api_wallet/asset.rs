use crate::domain::{api_wallet::assets::ApiAssetsDomain, assets::AssetsDomain};

#[derive(Debug, Clone)]
pub struct AddressChainCode {
    pub address: String,
    pub chain_code: String,
}

pub struct ApiAssetsService {}

impl ApiAssetsService {
    pub fn new() -> Self {
        Self {}
    }

    // 根据地址来同步余额(链)
    pub async fn sync_assets_by_addr(
        self,
        addr: Vec<String>,
        chain_code: Option<String>,
        symbol: Vec<String>,
    ) -> Result<(), crate::ServiceError> {
        AssetsDomain::sync_assets_by_addr_chain(addr, chain_code, symbol).await
    }

    // 从后端同步余额(后端)
    pub async fn sync_assets_from_backend(
        self,
        addr: String,
        chain_code: Option<String>,
        _symbol: Vec<String>,
    ) -> Result<(), crate::ServiceError> {
        AssetsDomain::async_balance_from_backend_addr(addr, chain_code).await
    }

    // 根据钱包地址来同步资产余额
    pub async fn sync_assets_by_wallet_chain(
        self,
        wallet_address: &str,
        account_id: Option<u32>,
        _symbol: Vec<String>,
    ) -> Result<(), crate::ServiceError> {
        ApiAssetsDomain::sync_assets_by_wallet(wallet_address, account_id, _symbol).await
    }

    pub async fn sync_assets_by_wallet_backend(
        self,
        wallet_address: String,
        account_id: Option<u32>,
        _symbol: Vec<String>,
    ) -> Result<(), crate::ServiceError> {
        AssetsDomain::async_balance_from_backend_wallet(wallet_address, account_id).await
    }
}
