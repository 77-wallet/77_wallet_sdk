use crate::messaging::mqtt::message::{BizType, Body};

#[derive(Debug, thiserror::Error)]
pub enum SystemError {
    #[error("Service error: {0}")]
    Service(String),
    #[error("Context not init")]
    ContextNotInit,
    #[error("Lock poison: {0}")]
    LockPoison(String),
    #[error("Channel send failed: {0}")]
    ChannelSendFailed(String),
    #[error("Frontend notify sender not set")]
    FrontendNotifySenderUnset,
    #[error("Message wrong [biz_type]: {0:?}, [body]: {1:?}")]
    MessageWrong(BizType, Box<Body>),
    #[error("Mqtt client not init")]
    MqttClientNotInit,
    #[error("device not init")]
    DeviceNotInit,
}
