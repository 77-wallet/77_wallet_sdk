use wallet_database::entities::{device::DeviceEntity, wallet::WalletEntity};

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetConfigRes {
    pub fiat: String,
    pub language: String,
    pub unread_count: UnreadCount,
    pub wallet_list: Vec<WalletEntity>,
    pub device_info: Option<DeviceEntity>,
    pub url: crate::request::init::UrlParams,
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
    // 待处理的交易
    pub pending_multisig_trans: i32,
    // 待部署的多签
    pub pending_deploy_multisig: i32,
    // 同意的多签数量
    pub pending_agree_multisig: i32,
}
