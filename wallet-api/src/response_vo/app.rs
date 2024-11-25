// {
//     "msg": "string",
//     "code": "string",
//     "data": {
//       "deviceType": "ANDROID",
//       "version": "string",
//       "minVersion": "string",
//       "updateType": "{force:强制更新}",
//       "frequency": 0,
//       "remark": "string",
//       "startTime": "2024-09-03T13:16:34.974Z",
//       "status": "{not_start:未开始}",
//       "operator": "string",
//       "newVersion": "string"
//     },
//     "success": true
//   }

// #[derive(Debug, serde::Deserialize, serde::Serialize)]
// #[serde(rename_all = "camelCase")]
// pub struct ChainInfo {
//     pub chain_code: Option<String>,
//     pub token_address: Option<String>,
//     pub protocol: Option<String>,
// }

use wallet_database::entities::{device::DeviceEntity, wallet::WalletEntity};

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetConfigRes {
    pub fiat: String,
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
