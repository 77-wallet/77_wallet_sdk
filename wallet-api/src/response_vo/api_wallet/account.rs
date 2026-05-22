use crate::{domain::account::AccountDomain, response_vo::standard_wallet::wallet::ChainInfo};
use wallet_database::entities::{api_account::ApiAccountEntity, api_wallet::ApiWalletType};
use wallet_types::chain::address::category::AddressCategory;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiAccountInfo {
    pub account_id: u32,
    pub account_index_map: wallet_utils::address::AccountIndexMap,
    pub name: String,
    pub balance: crate::response_vo::standard_wallet::account::BalanceInfo,
    pub chain: Vec<ChainInfo>,
    pub api_wallet_type: ApiWalletType,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiAccountInfos(pub Vec<ApiAccountInfo>);

impl std::ops::Deref for ApiAccountInfos {
    type Target = Vec<ApiAccountInfo>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for ApiAccountInfos {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryApiAccountDerivationPath {
    pub address: String,
    pub derivation_path: String,
    pub chain_code: String,
    pub address_type: AddressCategory,
}

impl QueryApiAccountDerivationPath {
    pub fn new(
        address: &str,
        derivation_path: &str,
        chain_code: &str,
        address_type: AddressCategory,
    ) -> Self {
        Self {
            address: address.to_string(),
            derivation_path: derivation_path.to_string(),
            chain_code: chain_code.to_string(),
            address_type,
        }
    }
}

impl TryFrom<ApiAccountEntity> for QueryApiAccountDerivationPath {
    type Error = crate::error::service::ServiceError;

    fn try_from(value: ApiAccountEntity) -> Result<Self, Self::Error> {
        let address_type =
            AccountDomain::get_show_address_type(&value.chain_code, value.address_type())?;

        Ok(QueryApiAccountDerivationPath {
            address: value.address,
            derivation_path: value.derivation_path,
            chain_code: value.chain_code,
            address_type,
        })
    }
}

/// 地址搜索结果项
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiWalletAddressSearchItem {
    pub account_id: u32,
    pub account_name: Option<String>,
    pub address: String,
    pub chain_code: String,
}

impl From<ApiAccountEntity> for ApiWalletAddressSearchItem {
    fn from(entity: ApiAccountEntity) -> Self {
        Self {
            account_id: entity.account_id,
            account_name: Some(entity.name),
            address: entity.address,
            chain_code: entity.chain_code,
        }
    }
}

/// 地址搜索响应
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiWalletAddressSearchResp {
    pub items: Vec<ApiWalletAddressSearchItem>,
}
