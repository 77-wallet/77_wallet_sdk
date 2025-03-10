pub mod constant;
mod eventloop;
pub(crate) mod handle;
pub mod payload;
pub(crate) mod topic;
pub mod user_property;
use eventloop::{EventLoopHandler, MqttEventLoop};
use rumqttc::v5::{AsyncClient, EventLoop};
use std::{future::Future, pin::Pin};
use tokio_stream::StreamExt as _;
use wallet_transport::client::mqtt_client::{MqttAsyncClient, MqttClientBuilder};

pub(crate) static MQTT_PROCESSOR: once_cell::sync::Lazy<tokio::sync::OnceCell<MqttAsyncClient>> =
    once_cell::sync::Lazy::new(tokio::sync::OnceCell::new);

pub async fn init_mqtt_processor<'a>(
    username: &str,
    password: &str,
    user_property: user_property::UserProperty,
    handle_eventloop: EventLoopHandler,
) -> Result<&'a MqttAsyncClient, crate::ServiceError> {
    MQTT_PROCESSOR
        .get_or_try_init(async || {
            let url = crate::manager::Context::get_global_mqtt_url().await?;
            let Some(url) = url.as_ref() else {
                return Err(crate::ServiceError::System(
                    crate::SystemError::MqttClientNotInit,
                ));
            };

            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            let rx = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);

            let up = user_property.to_vec();

            tracing::debug!("[init_mqtt_processor] url: {url}");
            let (client, eventloop) =
                MqttClientBuilder::new(&user_property.client_id, url, username, password, up)
                    .build()?;

            let cli = client.client();
            let eventloop = MqttEventLoop::new(eventloop);
            tokio::spawn(async move { eventloop.process(tx, cli, handle_eventloop).await });

            let cli = client.client();
            tokio::spawn(async move { exec_event(rx, cli).await });

            Ok(client)
        })
        .await
}

pub fn wrap_handle_eventloop(
    tx: tokio::sync::mpsc::UnboundedSender<rumqttc::v5::Event>,
    client: AsyncClient,
    eventloop: EventLoop,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    Box::pin(handle_eventloop(tx, client, eventloop))
}

async fn handle_eventloop(
    tx: tokio::sync::mpsc::UnboundedSender<rumqttc::v5::Event>,
    _client: AsyncClient,
    mut eventloop: EventLoop,
) {
    loop {
        let event = eventloop.poll().await;
        match event {
            Ok(notif) => {
                if let Err(e) = tx.send(notif) {
                    tracing::error!("[handle eventloop] send channel error: {e}");
                };
            }
            Err(err) => {
                tracing::error!("[mqtt] connection error = {err:?}");
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                let data = crate::notify::NotifyEvent::ConnectionError(
                    crate::notify::event::other::ConnectionErrorFrontend {
                        message: err.to_string(),
                    },
                );
                match crate::notify::FrontendNotifyEvent::new(data).send().await {
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
    while let Some(notif) = rx.next().await {
        #[cfg(not(feature = "prod"))]
        tracing::info!("[mqtt] event: {notif:?}");
        let res = match notif {
            rumqttc::v5::Event::Incoming(packet) => {
                crate::mqtt::handle::exec_incoming(&client, packet).await
            }
            rumqttc::v5::Event::Outgoing(_) => Ok(()),
        };

        if let Err(e) = res {
            tracing::error!("[handle eventloop] error: {e}");
        }
    }
    Ok(())
}
