use crate::mqtt::payload::incoming::transaction::MultiSignTransAcceptCompleteMsgBody;
use serde::Serialize;
use wallet_transport_backend::ConfirmedAddress;

#[derive(Debug)]
pub struct FrontendNotifyEvent {
    pub event: String,
    pub data: NotifyEvent,
}

impl FrontendNotifyEvent {
    pub(crate) fn new(data: NotifyEvent) -> Self {
        crate::notify::FrontendNotifyEvent {
            event: data.name(),
            data,
        }
    }

    pub(crate) async fn send(
        self,
        // service: &crate::service::Service,
    ) -> Result<(), crate::ServiceError> {
        let sender = crate::manager::Context::get_global_frontend_notify_sender()?;
        // let sender = service.get_global_frontend_notify_sender()?;
        let sender = sender.read().await;
        if let Some(sender) = sender.as_ref() {
            sender.send(self).map_err(|e| {
                crate::ServiceError::System(crate::SystemError::ChannelSendFailed(e.to_string()))
            })?;
        } else {
            return Err(crate::ServiceError::System(
                crate::SystemError::FrontendNotifySenderUnset,
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum NotifyEvent {
    OrderMultiSignAccept(OrderMultiSignAcceptFrontend),
    OrderMultiSignAcceptCompleteMsg(OrderMultiSignAcceptCompleteMsgFrontend),
    OrderMultiSignServiceComplete(OrderMultiSignServiceCompleteFrontend),
    OrderMultiSignCreated(OrderMultiSignCreatedFrontend),
    OrderMultisignCanceled(OrderMultisignCanceledFrontend),
    MultiSignTransAccept(MultiSignTransAcceptFrontend),
    MultiSignTransAcceptCompleteMsg(MultiSignTransAcceptCompleteMsgFrontend),
    AcctChange(AcctChangeFrontend),
    TokenPriceChange(crate::response_vo::coin::TokenPriceChangeRes),
    Init(crate::mqtt::payload::incoming::init::Init),
    BulletinMsg(crate::mqtt::payload::incoming::announcement::BulletinMsg),

    MqttConnected,
    MqttDisconnected,
    KeepAlive,
    ConnectionError(ConnectionErrorFrontend),
    Debug(DebugFront),
    Err(ErrFront),
}

impl NotifyEvent {
    pub(crate) fn name(&self) -> String {
        match self {
            NotifyEvent::OrderMultiSignAccept(_) => "ORDER_MULTI_SIGN_ACCEPT".to_string(),
            NotifyEvent::OrderMultiSignAcceptCompleteMsg(_) => {
                "ORDER_MULTI_SIGN_ACCEPT_COMPLETE_MSG".to_string()
            }
            NotifyEvent::OrderMultiSignServiceComplete(_) => {
                "ORDER_MULTI_SIGN_SERVICE_COMPLETE".to_string()
            }
            NotifyEvent::OrderMultiSignCreated(_) => "ORDER_MULTI_SIGN_CREATED".to_string(),
            NotifyEvent::OrderMultisignCanceled(_) => "ORDER_MULTI_SIGN_CANCEL".to_string(),
            NotifyEvent::MultiSignTransAccept(_) => "MULTI_SIGN_TRANS_ACCEPT".to_string(),
            NotifyEvent::MultiSignTransAcceptCompleteMsg(_) => {
                "MULTI_SIGN_TRANS_ACCEPT_COMPLETE_MSG".to_string()
            }
            NotifyEvent::AcctChange(_) => "ACCT_CHANGE".to_string(),
            NotifyEvent::TokenPriceChange(_) => "TOKEN_PRICE_CHANGE".to_string(),
            NotifyEvent::Init(_) => "INIT".to_string(),
            NotifyEvent::BulletinMsg(_) => "BULLETIN_MSG".to_string(),
            NotifyEvent::MqttConnected => "MQTT_CONNECTED".to_string(),
            NotifyEvent::MqttDisconnected => "MQTT_DISCONNECTED".to_string(),
            NotifyEvent::KeepAlive => "KEEP_ALIVE".to_string(),
            NotifyEvent::ConnectionError(_) => "CONNECTION_ERROR".to_string(),
            NotifyEvent::Debug(_) => "DEBUG".to_string(),
            NotifyEvent::Err(_) => "ERR".to_string(),
        }
    }
}

// biz_type = ORDER_MULTI_SIGN_ACCEPT
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderMultiSignAcceptFrontend {
    /// 钱包名称
    pub(crate) name: String,
    /// 发起方地址
    pub(crate) initiator_addr: String,
    /// 多签钱包地址
    pub(crate) address: String,
    /// 链编码
    pub(crate) chain_code: String,
    /// 签名阀值
    pub(crate) threshold: i32,
    pub(crate) memeber: Vec<wallet_database::entities::multisig_member::MemberVo>,
}

// biz_type = ORDER_MULTI_SIGN_ACCEPT_COMPLETE_MSG
#[derive(Debug, serde::Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderMultiSignAcceptCompleteMsgFrontend {
    /// 参与状态(同意1,不同意0)
    pub status: i8,
    /// 多签钱包地址
    pub multisign_address: String,
    /// 参与方地址
    pub address_list: Vec<ConfirmedAddress>,
    pub accept_status: bool, // 参与人全部确认完
                             // pub confirm_list: Vec<crate::mqtt::payload::incoming::signature::Confirm>,
}

// biz_type = ORDER_MULTI_SIGN_SERVICE_COMPLETE
#[derive(Debug, serde::Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderMultiSignServiceCompleteFrontend {
    /// 多签钱包地址
    pub multisign_address: String,
    // 多签账号结果 true 多签账号或服务费执行完成  false 失败
    pub status: bool,
    // 1手续费 2服务费
    pub r#type: u8,
}

// biz_type = ORDER_MULTI_SIGN_CREATED
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderMultiSignCreatedFrontend {
    /// 多签账户id
    pub multisig_account_id: String,
    /// 多签账户地址
    pub multisig_account_address: String,
    /// 地址类型
    pub address_type: String,
}

// biz_type = ORDER_MULTI_SIGN_CANCEL
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderMultisignCanceledFrontend {
    /// 多签账户id
    pub multisig_account_id: String,
    /// 多签账户地址
    pub multisig_account_address: String,
    /// 地址类型
    pub address_type: String,
}

// biz_type = MULTI_SIGN_TRANS_ACCEPT
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiSignTransAcceptFrontend {
    /// 队列id
    pub id: String,
    pub from_addr: String,
    pub to_addr: String,
    pub value: String,
    pub expiration: i64,
    pub symbol: String,
    pub chain_code: String,
    pub token_addr: Option<String>,
    /// 签名哈希
    pub msg_hash: String,
    /// 交易哈希
    pub tx_hash: String,
    pub raw_data: String,
    /// 0待签名 1待执行 2已执行
    pub status: i8,
    pub notes: String,
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
}

// biz_type = MULTI_SIGN_TRANS_ACCEPT_COMPLETE_MSG
type MultiSignTransAcceptCompleteMsgFrontend = MultiSignTransAcceptCompleteMsgBody;

// biz_type = ACCT_CHANGE
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcctChangeFrontend {
    // 交易hash
    pub tx_hash: String,
    // 链码
    pub chain_code: String,
    // 币种符号
    pub symbol: String,
    // 交易方式 0转入 1转出 2初始化
    pub transfer_type: i8,
    // 交易类型 1:普通交易，2:部署多签账号 3:服务费
    pub tx_kind: i8,
    // 发起方
    pub from_addr: String,
    // 接收方
    pub to_addr: String,
    // 合约地址
    pub token: Option<String>,
    // 交易额
    pub value: f64,
    // 手续费
    pub transaction_fee: f64,
    // 交易时间
    pub transaction_time: String,
    // 交易状态 true-成功 false-失败
    pub status: bool,
    // 是否多签 1-是，0-否
    pub is_multisig: i32,
    // 队列id
    pub queue_id: String,
    // 块高
    pub block_height: i64,
    // 备注
    pub notes: String,
}

// biz_type = ORDER_MULTI_SIGN_ACCEPT_COMPLETE_MSG
#[derive(Debug, serde::Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionErrorFrontend {
    pub message: String,
}

// biz_type = ERR
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrFront {
    pub event: String,
    pub message: String,
}

// biz_type = DEBUG
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugFront {
    pub message: serde_json::Value,
}
