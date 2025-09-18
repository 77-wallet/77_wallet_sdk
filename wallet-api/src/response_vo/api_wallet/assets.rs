use std::ops::{Deref, DerefMut};

use crate::response_vo::{account::BalanceInfo, chain::ChainList};

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiAccountChainAsset {
    pub chain_code: String,
    pub symbol: String,
    pub name: String,
    /// key: chainCode, value: tokenAddress
    pub chain_list: ChainList,
    pub balance: BalanceInfo,
    // pub is_multichain: bool,
    pub is_multisig: i8,
    pub is_default: bool,
}

#[derive(Debug, serde::Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiAccountChainAssetList(pub Vec<ApiAccountChainAsset>);

impl Deref for ApiAccountChainAssetList {
    type Target = Vec<ApiAccountChainAsset>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ApiAccountChainAssetList {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
