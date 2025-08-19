use super::{
    multisig::{
        OrderMultiSignAcceptCompleteMsgFrontend, OrderMultiSignAcceptFrontend,
        OrderMultiSignCreatedFrontend, OrderMultiSignServiceCompleteFrontend,
        OrderMultisignCanceledFrontend,
    },
    other::{
        ChainChangeFrontend, ConnectionErrorFrontend, DebugFront, ErrFront,
        TransactionProcessFrontend,
    },
    permission::PermissionChangeFrontend,
    transaction::{
        AcctChangeFrontend, ConfirmationFrontend, MultiSignTransAcceptCompleteMsgFrontend,
    },
};
use crate::messaging::{
    mqtt::topics::{
        api_wallet::{AddressUseMsg, UnbindUidMsg, WithdrawMsg},
        BulletinMsg,
    },
    notify::api_wallet::WithdrawFront,
};
use crate::messaging::notify::api_wallet::WithdrawNoPassFront;

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
    // Init(Init),
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
    ChainChange(ChainChangeFrontend),

    // 资源
    // ResourceChange(ResourceChangeFrontend),
    // 权限变更事件
    PermissionChanger(PermissionChangeFrontend),
    // 恢复多签数据完成
    RecoverComplete,
    // 多签交易取消
    MultisigTransCancel,
    // 多签交易执行
    MultiSignTransExecute,

    // 其他
    // 同步资产
    SyncAssets,

    // API wallet
    UnbindUid(UnbindUidMsg),
    AddressUse(AddressUseMsg),
    Withdraw(WithdrawFront),
    WithdrawNoPass(WithdrawNoPassFront),
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

            // 权限变更事件
            NotifyEvent::PermissionChanger(_) => "PERMISSION_CHANGE".to_string(),
            // 恢复多签数据完成
            NotifyEvent::RecoverComplete => "RECOVER_COMPLETE".to_string(),
            NotifyEvent::MultisigTransCancel => "MULTISIG_TRANS_CANCE".to_string(),
            // 多签交易执行
            NotifyEvent::MultiSignTransExecute => "MULTI_SIGN_TRANS_EXECUTE".to_string(),

            // 其他
            // 同步资产
            NotifyEvent::SyncAssets => "SYNC_ASSETS".to_string(),
            NotifyEvent::UnbindUid(_) => "UNBIND_UID".to_string(),
            NotifyEvent::AddressUse(_) => "ADDRESS_USE".to_string(),
            NotifyEvent::Withdraw(_) => "WITHDRAW".to_string(),
            NotifyEvent::WithdrawNoPass(_) => "WITHDRAW_NO_PASS".to_string(),
        }
    }
}
