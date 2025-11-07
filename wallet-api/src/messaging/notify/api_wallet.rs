use std::ops::{Deref, DerefMut};

use dashmap::DashMap;
use wallet_transport_backend::response_vo::api_wallet::wallet::ActiveStatus;

use crate::{
    messaging::mqtt::topics::api_wallet::{
        cmd::wallet_activation::AwmCmdActiveMsg, trans::AwmOrderTransMsg,
    },
    response_vo::account::BalanceInfo,
};

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawFront {
    pub uid: String,
    pub from_addr: String,
    pub to_addr: String,
    pub value: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawNoPassFront {
    pub uid: String,
    pub from_addr: String,
    pub to_addr: String,
    pub value: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectFront {
    pub uid: String,
    pub from_addr: String,
    pub to_addr: String,
    pub value: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectFeeNotEnoughFront {
    pub uid: String,
    pub from_addr: String,
    pub to_addr: String,
    pub value: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeFront {
    pub uid: String,
    pub from_addr: String,
    pub to_addr: String,
    pub value: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AwmCmdActiveMsgFront {
    pub chain_code: String,
    pub uid: String,
    /// 激活状态: 0 未激活 /1已激活
    pub active: ActiveStatus,
}

impl From<&AwmCmdActiveMsg> for AwmCmdActiveMsgFront {
    fn from(msg: &AwmCmdActiveMsg) -> Self {
        Self {
            chain_code: msg.chain_code.clone(),
            uid: msg.uid.clone(),
            active: msg.active.clone(),
        }
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AwmCmdAddrExpandMsgFront {
    pub uid: String,
    pub done_number: u32,
    /// 扩容数量（可空，CHA_BATCH 类型时有效）
    #[serde(deserialize_with = "wallet_utils::serde_func::string_to_u32")]
    pub number: u32,
}

// impl From<&AwmCmdAddrExpandMsg> for AwmCmdAddrExpandMsgFront {
//     fn from(msg: &AwmCmdAddrExpandMsg) -> Self {
//         Self { uid: msg.uid.clone(), number: msg.number.clone() }
//     }
// }

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AwmOrderTransMsgFront {
    from: String,
    to: String,
    value: String,
    chain_code: String,
    #[serde(rename = "tokenAddr")]
    token_address: String,
    symbol: String,
    /// 平台交易单号
    trade_no: String,
    /// 交易类型： 1 提币 / 2 归集 / 3 归集手续费交易
    #[serde(deserialize_with = "wallet_utils::serde_func::string_to_u32")]
    trade_type: u32,
    /// 是否需要审核（可空）： 1 不需要审核 / 2 需要审核
    #[serde(deserialize_with = "wallet_utils::serde_func::string_to_u32")]
    audit: u32,
    uid: String,
}

impl From<&AwmOrderTransMsg> for AwmOrderTransMsgFront {
    fn from(msg: &AwmOrderTransMsg) -> Self {
        Self {
            from: msg.from.clone(),
            to: msg.to.clone(),
            value: msg.value.clone(),
            chain_code: msg.chain_code.clone(),
            token_address: msg.token_address.clone(),
            symbol: msg.symbol.clone(),
            trade_no: msg.trade_no.clone(),
            trade_type: msg.trade_type,
            audit: msg.audit,
            uid: msg.uid.clone(),
        }
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
// key： 钱包地址， value：账户资产信息
pub struct ApiWalletSyncAssetsMsgFront(pub DashMap<String, Vec<ApiWalletSyncAssetsMsgFrontItem>>);

impl Deref for ApiWalletSyncAssetsMsgFront {
    type Target = DashMap<String, Vec<ApiWalletSyncAssetsMsgFrontItem>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ApiWalletSyncAssetsMsgFront {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl ApiWalletSyncAssetsMsgFront {
    pub fn new() -> Self {
        Self(DashMap::new())
    }

    pub fn add_item(&self, wallet_address: &str, item: ApiWalletSyncAssetsMsgFrontItem) {
        let mut vec_ref = self.entry(wallet_address.to_string()).or_insert(Vec::new());
        vec_ref.push(item);
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ApiWalletSyncAssetsMsgFrontItem {
    // 账户地址
    pub address: String,
    // 链码
    pub chain_code: String,
    // 合约地址
    pub token_address: String,
    // 余额
    pub balance: BalanceInfo,
}

impl ApiWalletSyncAssetsMsgFrontItem {
    pub fn new(address: &str, chain_code: &str, token_address: &str, balance: BalanceInfo) -> Self {
        Self {
            address: address.to_string(),
            chain_code: chain_code.to_string(),
            token_address: token_address.to_string(),
            balance,
        }
    }
}
