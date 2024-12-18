pub(crate) mod event;
pub(crate) use event::NotifyEvent;

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

    pub(crate) async fn send_debug<T: serde::Serialize>(
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

    pub(crate) async fn send_error<T: serde::Serialize>(
        event: &str,
        message: T,
    ) -> Result<(), crate::ServiceError> {
        let message = wallet_utils::serde_func::serde_to_string(&message)?;
        let data = crate::notify::NotifyEvent::Err(event::other::ErrFront {
            event: event.to_string(),
            message,
        });
        match crate::notify::FrontendNotifyEvent::new(data).send().await {
            Ok(_) => tracing::debug!("[mqtt] send err message ok"),
            Err(e) => tracing::error!("[mqtt] send err message error: {e}"),
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
