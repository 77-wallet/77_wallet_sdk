pub(crate) mod event;

pub(crate) use event::NotifyEvent;
use serde::Serialize;

#[derive(Debug)]
pub struct FrontendNotifyEvent {
    pub event: String,
    pub data: NotifyEvent,
}

impl FrontendNotifyEvent {
    pub(crate) fn new(data: NotifyEvent) -> Self {
        crate::notify::FrontendNotifyEvent {
            event: data.event_name(),
            data,
        }
    }

    pub(crate) async fn send_error<T: serde::Serialize>(
        message: T,
    ) -> Result<(), crate::ServiceError> {
        let message = wallet_utils::serde_func::serde_to_value(message)?;
        let data = crate::notify::NotifyEvent::Debug(event::other::DebugFront { message });
        match crate::notify::FrontendNotifyEvent::new(data).send().await {
            Ok(_) => tracing::debug!("[mqtt] send debug message ok"),
            Err(e) => tracing::error!("[mqtt] send debug message error: {e}"),
        };
        Ok(())
    }

    pub(crate) async fn send(self) -> Result<(), crate::ServiceError> {
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
            NotifyEvent::MultiSignTransAccept(_) => "MULTI_SIGN_TRANS_ACCEPT".to_string(),
            NotifyEvent::MultiSignTransAcceptCompleteMsg(_) => {
                "MULTI_SIGN_TRANS_ACCEPT_COMPLETE_MSG".to_string()
            }
            NotifyEvent::AcctChange(_) => "ACCT_CHANGE".to_string(),
            NotifyEvent::TokenPriceChange(_) => "TOKEN_PRICE_CHANGE".to_string(),
            NotifyEvent::Init(_) => "INIT".to_string(),
            NotifyEvent::BulletinMsg(_) => "BULLETIN_MSG".to_string(),

            NotifyEvent::FetchBulletinMsg => "FETCH_BULLETIN_MSG".to_string(),
            NotifyEvent::MqttConnected => "MQTT_CONNECTED".to_string(),
            NotifyEvent::MqttDisconnected => "MQTT_DISCONNECTED".to_string(),
            NotifyEvent::KeepAlive => "KEEP_ALIVE".to_string(),
            NotifyEvent::ConnectionError(_) => "CONNECTION_ERROR".to_string(),
            NotifyEvent::Debug(_) => "DEBUG".to_string(),
            NotifyEvent::Err(_) => "ERR".to_string(),
        }
    }
}
