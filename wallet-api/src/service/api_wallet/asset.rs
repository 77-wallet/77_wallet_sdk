use std::collections::HashMap;

use crate::{
    domain::{
        account::AccountDomain, api_wallet::assets::ApiAssetsDomain, assets::AssetsDomain,
        coin::CoinDomain, multisig::MultisigDomain,
    },
    infrastructure::task_queue::{BackendApiTask, BackendApiTaskData, CommonTask, task::Tasks},
    response_vo::{
        assets::{AccountChainAsset, AccountChainAssetList, CoinAssets, GetAccountAssetsRes},
        chain::ChainList,
    },
};
use wallet_database::{
    dao::assets::CreateAssetsVo,
    entities::{
        assets::{AssetsEntity, AssetsId},
        coin::SymbolId,
    },
    repositories::{
        ResourcesRepo,
        account::AccountRepoTrait,
        assets::AssetsRepoTrait,
        chain::{ChainRepo, ChainRepoTrait},
        coin::CoinRepoTrait,
        device::DeviceRepo,
    },
};
use wallet_transport_backend::request::{
    TokenBalanceRefresh, TokenBalanceRefreshReq, TokenQueryPriceReq,
};

#[derive(Debug, Clone)]
pub struct AddressChainCode {
    pub address: String,
    pub chain_code: String,
}

pub struct ApiAssetsService {
    pub repo: ResourcesRepo,
    account_domain: AccountDomain,
    assets_domain: AssetsDomain,
    coin_domain: CoinDomain, // keystore: wallet_crypto::Keystore
                             // keystore: wallet_crypto::Keystore
}

impl ApiAssetsService {
    pub fn new(repo: ResourcesRepo) -> Self {
        Self {
            repo,
            account_domain: AccountDomain::new(),
            assets_domain: AssetsDomain::new(),
            coin_domain: CoinDomain::new(),
        }
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
