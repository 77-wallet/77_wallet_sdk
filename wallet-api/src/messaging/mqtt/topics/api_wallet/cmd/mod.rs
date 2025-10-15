use crate::messaging::mqtt::topics::api_wallet::cmd::{
    address_allock::AwmCmdAddrExpandMsg, unbind_uid::AwmCmdUidUnbindMsg,
    wallet_activation::AwmCmdActiveMsg,
};

pub(crate) mod address_allock;
pub(crate) mod address_use;
pub(crate) mod dev_change;
pub(crate) mod fee_res;
pub(crate) mod unbind_uid;
pub(crate) mod wallet_activation;

// biz_type = MERCHAIN_CMD
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
// #[serde(rename_all = "camelCase")]
// #[serde(untagged)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", tag = "bizType")]
pub enum AwmCmdMsg {
    /// 地址扩容
    AwmCmdAddrExpand(AwmCmdAddrExpandMsg),
    // AwmCmdFeeRes
    /// 激活钱包
    AwmCmdActive(AwmCmdActiveMsg),
    /// 平台解绑
    AwmCmdUidUnbind(AwmCmdUidUnbindMsg),
}

// impl AwmCmdMsg {
//     pub(crate) async fn exec(
//         &self,
//         _msg_id: &str,
//     ) -> Result<(), crate::error::service::ServiceError> {
//         tracing::info!("[AwmCmdMsg] exec: {:?}", self);
//         match self {
//             AwmCmdMsg::AwmCmdActive(msg) => msg.exec(_msg_id).await?,
//             AwmCmdMsg::AwmCmdUidUnbind(msg) => msg.exec(_msg_id).await?,
//             AwmCmdMsg::AwmCmdAddrExpand(msg) => msg.exec(_msg_id).await?,
//         }
//         Ok(())
//     }
// }
