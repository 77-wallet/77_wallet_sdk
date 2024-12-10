use multisig::*;
use other::*;
use transaction::*;

pub(crate) mod multisig;
pub(crate) mod other;
pub(crate) mod transaction;
#[derive(Debug, serde::Serialize)]
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

    FetchBulletinMsg,
    MqttConnected,
    MqttDisconnected,
    KeepAlive,
    ConnectionError(ConnectionErrorFrontend),
    Debug(DebugFront),
    Err(ErrFront),
}
