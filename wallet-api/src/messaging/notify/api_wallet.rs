use wallet_transport_backend::response_vo::api_wallet::wallet::ActiveStatus;

use crate::messaging::mqtt::topics::api_wallet::{
    cmd::{
        address_allock::{AddressAllockType, AwmCmdAddrExpandMsg},
        wallet_activation::AwmCmdActiveMsg,
    },
    trans::AwmOrderTransMsg,
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
    /// 扩容类型： CHA_ALL / CHA_INDEX
    #[serde(rename = "type")]
    pub typ: AddressAllockType,
    pub chain_code: String,
    pub index: Option<i32>,
    pub uid: String,
    /// 扩容编号  
    pub serial_no: String,
    /// 扩容数量（可空，CHA_BATCH 类型时有效）
    #[serde(deserialize_with = "wallet_utils::serde_func::string_to_u32")]
    pub number: u32,
}

impl From<&AwmCmdAddrExpandMsg> for AwmCmdAddrExpandMsgFront {
    fn from(msg: &AwmCmdAddrExpandMsg) -> Self {
        Self {
            typ: msg.typ.clone(),
            chain_code: msg.chain_code.clone(),
            index: msg.index.clone(),
            uid: msg.uid.clone(),
            serial_no: msg.serial_no.clone(),
            number: msg.number.clone(),
        }
    }
}

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
