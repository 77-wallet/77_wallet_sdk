use super::account::BalanceInfo;
use wallet_database::entities::chain::ChainEntity;

pub mod assets_struct {

    #[derive(Debug, serde::Serialize)]
    pub struct AllChainAssets {
        pub name: String,
        pub symbol: String,
        pub address: String,
        pub token_address: String,
        pub status: u8,
        pub balance: String,
        pub usdt_balance: String,
        // pub unit_price: String,
        // pub unit
        pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
        pub updated_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
    }
}

#[derive(Debug, serde::Serialize)]
pub struct ChainInfo {
    pub address: String,
    pub total_assets: wallet_types::Decimal,
    pub token_info: std::collections::HashMap<String, TokenInfo>,
}

type ChainCode = String;
type ChainName = String;
#[derive(Debug, serde::Serialize)]
pub struct ChainCodeAndName(std::collections::HashMap<ChainCode, ChainName>);

impl std::ops::Deref for ChainCodeAndName {
    type Target = std::collections::HashMap<ChainCode, ChainName>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for ChainCodeAndName {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<Vec<ChainEntity>> for ChainCodeAndName {
    fn from(chain: Vec<ChainEntity>) -> Self {
        let mut res = std::collections::HashMap::new();
        for chain in chain {
            res.insert(chain.chain_code, chain.name);
        }
        Self(res)
    }
}

#[derive(Debug, serde::Serialize)]
pub struct TokenInfo {
    pub token_name: String,
    pub total_assets: wallet_types::Decimal,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainAssets {
    pub chain_code: String,
    pub name: String,
    pub symbol: String,
    pub address: String,
    pub token_address: String,
    // pub address_catogary: BtcAddressCategoryOpt,
    pub balance: BalanceInfo,
    /// 0/普通资产 1/多签资产 2/待部署多签账户的普通资产
    pub is_multisig: i8,
    // pub is_multichain: bool,
}

// #[derive(Debug, serde::Serialize, Default)]
// #[serde(rename_all = "camelCase")]
// pub struct ChainAssetsList(pub Vec<ChainAssets>);

// impl Deref for ChainAssetsList {
//     type Target = Vec<ChainAssets>;

//     fn deref(&self) -> &Self::Target {
//         &self.0
//     }
// }

// impl DerefMut for ChainAssetsList {
//     fn deref_mut(&mut self) -> &mut Self::Target {
//         &mut self.0
//     }
// }

// impl ChainAssetsList {
//     // 使用 HashSet 来存储每个 symbol 对应的不同 chain_code，以避免重复
//     pub(crate) fn mark_multichain_assets(&mut self) {
//         // 使用 HashSet 来存储每个 symbol 对应的不同 chain_code，以避免重复
//         let mut symbol_chain_map: HashMap<String, HashSet<String>> = HashMap::new();

//         // 先填充 symbol_chain_map，每个 symbol 对应的 HashSet 包含不同的 chain_code
//         for asset in self.iter() {
//             symbol_chain_map
//                 .entry(asset.symbol.clone())
//                 .or_default()
//                 .insert(asset.chain_code.clone());
//         }

//         // 再次遍历 self，设置 is_multichain 标记
//         for asset in self.iter_mut() {
//             if let Some(chain_codes) = symbol_chain_map.get(&asset.symbol) {
//                 asset.is_multichain = chain_codes.len() > 1;
//             }
//         }
//     }
// }

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeDynData {
    pub chain_code: String,
    pub node_id: String,
    pub name: String,
    pub delay: u64,
    pub block_height: i64,
}

#[derive(Debug, Default, serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct NodeListRes {
    pub node_id: String,
    pub name: String,
    pub chain_code: String,
    pub rpc_url: String,
    pub ws_url: String,
    pub status: u8,
}
