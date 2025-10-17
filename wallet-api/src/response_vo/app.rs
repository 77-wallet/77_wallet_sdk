use wallet_database::entities::{
    device::DeviceEntity, multisig_account::MultisigAccountEntity,
    multisig_queue::MultisigQueueEntity, wallet::WalletEntity,
};

use crate::response_vo::{
    account::BalanceInfo,
    api_wallet::wallet::{ApiWalletList, WalletInfo},
};

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetConfigRes {
    pub fiat: String,
    pub language: String,
    pub unread_count: UnreadCount,
    pub standard_wallet_list: Vec<WalletInfo>,
    pub api_wallet_list: ApiWalletList,
    pub device_info: Option<DeviceEntity>,
    pub url: crate::request::init::UrlParams,
}

impl From<WalletEntity> for WalletInfo {
    fn from(value: WalletEntity) -> Self {
        WalletInfo {
            address: value.address,
            uid: value.uid,
            name: value.name,
            app_id: None,
            balance: BalanceInfo::default(),
        }
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnreadCount {
    pub system_notification: i64,
    pub announcement: i64,
}

#[derive(Debug, serde::Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GlobalMsg {
    // 待处理的交易(通用了 account base 结构)
    pub pending_multisig_trans: Vec<MultisigAccountBase>,
    // 待部署的多签
    pub pending_deploy_multisig: Vec<MultisigAccountBase>,
    // 同意的多签数量
    pub pending_agree_multisig: Vec<MultisigAccountBase>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MultisigAccountBase {
    pub id: String,
    pub address: String,
    pub status: Option<i32>,
}

impl From<MultisigAccountEntity> for MultisigAccountBase {
    fn from(entity: MultisigAccountEntity) -> Self {
        MultisigAccountBase { id: entity.id, address: entity.address, status: None }
    }
}

impl From<&MultisigQueueEntity> for MultisigAccountBase {
    fn from(entity: &MultisigQueueEntity) -> Self {
        MultisigAccountBase {
            id: entity.id.clone(),
            address: entity.from_addr.clone(),
            status: Some(entity.status as i32),
        }
    }
}
