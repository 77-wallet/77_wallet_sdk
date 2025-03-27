use crate::messaging::mqtt::topics::{BulletinMsg, ChainChange, Init};

use super::{
    multisig::{
        OrderMultiSignAcceptCompleteMsgFrontend, OrderMultiSignAcceptFrontend,
        OrderMultiSignCreatedFrontend, OrderMultiSignServiceCompleteFrontend,
        OrderMultisignCanceledFrontend,
    },
    other::{ConnectionErrorFrontend, DebugFront, ErrFront, TransactionProcessFrontend},
    permission::PermissionChangeFrontend,
    resource::ResourceChangeFrontend,
    transaction::{
        AcctChangeFrontend, ConfirmationFrontend, MultiSignTransAcceptCompleteMsgFrontend,
    },
};

#[derive(Debug, serde::Serialize)]
#[serde(untagged)]
pub enum NotifyEvent {
    OrderMultiSignAccept(OrderMultiSignAcceptFrontend),
    OrderMultiSignAcceptCompleteMsg(OrderMultiSignAcceptCompleteMsgFrontend),
    OrderMultiSignServiceComplete(OrderMultiSignServiceCompleteFrontend),
    OrderMultiSignCreated(OrderMultiSignCreatedFrontend),
    OrderMultisignCanceled(OrderMultisignCanceledFrontend),
    Confirmation(ConfirmationFrontend),
    MultiSignTransAcceptCompleteMsg(MultiSignTransAcceptCompleteMsgFrontend),
    AcctChange(AcctChangeFrontend),
    TokenPriceChange(crate::response_vo::coin::TokenPriceChangeRes),
    Init(Init),
    BulletinMsg(BulletinMsg),

    FetchBulletinMsg,
    MqttConnected,
    MqttDisconnected,
    KeepAlive,
    ConnectionError(ConnectionErrorFrontend),
    Debug(DebugFront),
    Err(ErrFront),

    // 执行交易的过程
    TransactionProcess(TransactionProcessFrontend),
    ChainChange(ChainChange),

    // 资源
    ResourceChange(ResourceChangeFrontend),
    // 权限变更事件
    PermissionChanger(PermissionChangeFrontend),
}

impl NotifyEvent {
    pub(crate) fn event_name(&self) -> String {
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
            NotifyEvent::Confirmation(_) => "CONFIRMATION".to_string(),
            NotifyEvent::MultiSignTransAcceptCompleteMsg(_) => {
                "MULTI_SIGN_TRANS_ACCEPT_COMPLETE_MSG".to_string()
            }
            NotifyEvent::AcctChange(_) => "ACCT_CHANGE".to_string(),
            NotifyEvent::TokenPriceChange(_) => "TOKEN_PRICE_CHANGE".to_string(),
            NotifyEvent::Init(_) => "INIT".to_string(),
            NotifyEvent::BulletinMsg(_) => "BULLETIN_MSG".to_string(),

            NotifyEvent::ChainChange(_) => "CHAIN_CHANGE".to_string(),

            NotifyEvent::FetchBulletinMsg => "FETCH_BULLETIN_MSG".to_string(),
            NotifyEvent::MqttConnected => "MQTT_CONNECTED".to_string(),
            NotifyEvent::MqttDisconnected => "MQTT_DISCONNECTED".to_string(),
            NotifyEvent::KeepAlive => "KEEP_ALIVE".to_string(),
            NotifyEvent::ConnectionError(_) => "CONNECTION_ERROR".to_string(),
            NotifyEvent::Debug(_) => "DEBUG".to_string(),
            NotifyEvent::Err(_) => "ERR".to_string(),
            NotifyEvent::TransactionProcess(_) => "TRANSACTION_PROCESS".to_string(),

            // 资源
            NotifyEvent::ResourceChange(_) => "RESOURCE_CHANGE".to_string(),

            // 权限变更事件
            NotifyEvent::PermissionChanger(_) => "PERMISSION_CHANGE".to_string(),
        }
    }
}
