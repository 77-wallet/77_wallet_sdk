use crate::{
    infrastructure::mqtt::client::MqttClientBuilder,
    messaging::notify::{event::NotifyEvent, other::ConnectionErrorFrontend, FrontendNotifyEvent},
};

use super::{client::MqttAsyncClient, property::UserProperty};
use rumqttc::v5::{Event, EventLoop};
use tokio::sync::mpsc::UnboundedSender;
use tokio_stream::StreamExt as _;

pub(crate) static MQTT_PROCESSOR: once_cell::sync::Lazy<tokio::sync::OnceCell<MqttAsyncClient>> =
    once_cell::sync::Lazy::new(tokio::sync::OnceCell::new);

pub async fn init_mqtt_processor<'a>(
    user_property: UserProperty,
    url: String,
) -> Result<&'a MqttAsyncClient, crate::ServiceError> {
    MQTT_PROCESSOR
        .get_or_try_init(|| async {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            let rx = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);

            tracing::debug!("[init_mqtt_processor] url: {url}");
            let (client, eventloop) = MqttClientBuilder::new(&url, user_property).build()?;

            tokio::spawn(async move { handle_eventloop(tx, eventloop).await });
            let cli = client.client();
            tokio::spawn(async move { exec_event(rx, cli).await });

            Ok(client)
        })
        .await
}

async fn handle_eventloop(tx: UnboundedSender<rumqttc::v5::Event>, mut eventloop: EventLoop) {
    loop {
        match eventloop.poll().await {
            Ok(event) => {
                if let Err(e) = tx.send(event) {
                    tracing::error!("[handle eventloop] send channel error: {e}");
                };
            }
            Err(err) => {
                tracing::error!("[mqtt] connection error = {err:?}");
                // if connect error  ,sleep 5s and reconnect
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                let data = NotifyEvent::ConnectionError(ConnectionErrorFrontend {
                    message: err.to_string(),
                });
                match FrontendNotifyEvent::new(data).send().await {
                    Ok(_) => tracing::debug!("[mqtt] sender send ok"),
                    Err(e) => tracing::error!("[mqtt] sender send error: {e}"),
                };
            }
        }
    }
}

pub async fn exec_event(
    mut rx: tokio_stream::wrappers::UnboundedReceiverStream<rumqttc::v5::Event>,
    client: rumqttc::v5::AsyncClient,
) -> Result<(), crate::ServiceError> {
    while let Some(event) = rx.next().await {
        // #[cfg(not(feature = "prod"))]
        // if filter_log_event(&event) {
        //     tracing::info!("[mqtt] receive event: {event:?}");
        // }

        let res = match event {
            rumqttc::v5::Event::Incoming(packet) => {
                crate::messaging::mqtt::handle::exec_incoming(&client, packet).await
            }
            rumqttc::v5::Event::Outgoing(_) => Ok(()),
        };

        if let Err(e) = res {
            tracing::error!("[exec_event] error: {e}");
        }
    }
    Ok(())
}

// 过滤ping 和 pong 的日志
fn filter_log_event(event: &Event) -> bool {
    match event {
        Event::Incoming(packet) => match packet {
            rumqttc::v5::Incoming::PingReq(_) => false,
            rumqttc::v5::Incoming::PingResp(_) => false,
            _ => true,
        },
        Event::Outgoing(outgoing) => match outgoing {
            rumqttc::Outgoing::PingReq => false,
            rumqttc::Outgoing::PingResp => false,
            _ => true,
        },
    }
}
