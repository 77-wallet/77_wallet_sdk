use super::property::UserProperty;
use crate::{
    error::{service::ServiceError, system::SystemError},
    infrastructure::mqtt::client::MqttClientBuilder,
    messaging::notify::{FrontendNotifyEvent, event::NotifyEvent, other::ConnectionErrorFrontend},
};
use rumqttc::v5::{Event, EventLoop, mqttbytes::QoS};
use std::sync::Arc;
use tokio::{
    sync::{Mutex, broadcast, mpsc::UnboundedSender},
    task::JoinHandle,
};
use tokio_stream::StreamExt as _;

#[derive(Debug)]
pub(crate) struct ProcessMqttHandle {
    shutdown_tx: broadcast::Sender<()>,
    client: Arc<rumqttc::v5::AsyncClient>,
    ev_handle: Mutex<Option<JoinHandle<()>>>,
    e_handle: Mutex<Option<JoinHandle<Result<(), ServiceError>>>>,
}

impl ProcessMqttHandle {
    pub async fn new(user_property: UserProperty, url: String) -> Result<Self, ServiceError> {
        let (shutdown_tx, _) = broadcast::channel(1);
        let shutdown_rx1 = shutdown_tx.subscribe();
        let shutdown_rx2 = shutdown_tx.subscribe();

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let rx = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);

        tracing::debug!("[init_mqtt_processor] url: {url}");
        let (client, eventloop) = MqttClientBuilder::new(&url, user_property).build()?;
        let client = Arc::new(client);

        let mut ev = ProcessMqttEventLoop::new(shutdown_rx1, tx, eventloop);
        let ev_handle = tokio::spawn(async move { ev.handle_eventloop().await });
        let mut e = ProcessMqttEvent::new(shutdown_rx2, rx, client.clone());
        let e_handle = tokio::spawn(async move { e.exec_event().await });

        Ok(Self {
            shutdown_tx,
            client,
            ev_handle: Mutex::new(Some(ev_handle)),
            e_handle: Mutex::new(Some(e_handle)),
        })
    }

    pub fn try_subscribe<S: Into<String>>(&self, topic: S, qos: QoS) -> Result<(), ServiceError> {
        self.client
            .try_subscribe(topic, qos)
            .map_err(|e| ServiceError::System(SystemError::MqttClientNotInit))
    }

    pub fn try_unsubscribe<S: Into<String>>(&self, topic: S) -> Result<(), ServiceError> {
        self.client
            .try_unsubscribe(topic)
            .map_err(|e| ServiceError::System(SystemError::MqttClientNotInit))
    }

    pub(crate) async fn close(&self) -> Result<(), ServiceError> {
        let _ = self.shutdown_tx.send(());
        if let Some(handle) = self.ev_handle.lock().await.take() {
            handle.await;
        }
        if let Some(handle) = self.e_handle.lock().await.take() {
            handle
                .await
                .map_err(|_| ServiceError::System(SystemError::BackendEndpointNotFound))??;
        }
        Ok(())
    }
}

struct ProcessMqttEventLoop {
    shutdown_rx: broadcast::Receiver<()>,
    tx: UnboundedSender<rumqttc::v5::Event>,
    eventloop: EventLoop,
}

impl ProcessMqttEventLoop {
    fn new(
        shutdown_rx: broadcast::Receiver<()>,
        tx: UnboundedSender<rumqttc::v5::Event>,
        eventloop: EventLoop,
    ) -> Self {
        Self { shutdown_rx, tx, eventloop }
    }

    async fn handle_eventloop(&mut self) {
        loop {
            tokio::select! {
                _ = self.shutdown_rx.recv() => {
                    tracing::info!("closing mqtt event loop -------------------------------");
                    break;
                }
                ev = self.eventloop.poll() => {
                    match ev {
                        Ok(event) => {
                            if let Err(e) = self.tx.send(event) {
                                tracing::error!("[handle event loop] send channel error: {e}");
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
        }
    }
}

struct ProcessMqttEvent {
    shutdown_rx: broadcast::Receiver<()>,
    rx: tokio_stream::wrappers::UnboundedReceiverStream<Event>,
    client: Arc<rumqttc::v5::AsyncClient>,
}

impl ProcessMqttEvent {
    fn new(
        shutdown_rx: broadcast::Receiver<()>,
        rx: tokio_stream::wrappers::UnboundedReceiverStream<Event>,
        client: Arc<rumqttc::v5::AsyncClient>,
    ) -> Self {
        Self { shutdown_rx, rx, client }
    }

    pub async fn exec_event(&mut self) -> Result<(), crate::error::service::ServiceError> {
        loop {
            tokio::select! {
                _ = self.shutdown_rx.recv() => {
                    self.client.disconnect().await;
                    tracing::info!("closing mqtt event -------------------------------");
                    break;
                }
                Some(event) = self.rx.next() => {
                  tracing::info!("[mqtt] receive event: {event:?}");
                    let res = match event {
                        rumqttc::v5::Event::Incoming(packet) => {
                            crate::messaging::mqtt::handle::exec_incoming(&self.client, packet).await
                        }
                        rumqttc::v5::Event::Outgoing(_) => Ok(()),
                    };

                    if let Err(e) = res {
                        tracing::error!("[exec_event] error: {e}");
                    }
                }
            }
        }
        Ok(())
    }
}

// 过滤ping 和 pong 的日志
// fn _filter_log_event(event: &Event) -> bool {
//     match event {
//         Event::Incoming(packet) => match packet {
//             rumqttc::v5::Incoming::PingReq(_) => false,
//             rumqttc::v5::Incoming::PingResp(_) => false,
//             _ => true,
//         },
//         Event::Outgoing(outgoing) => match outgoing {
//             rumqttc::Outgoing::PingReq => false,
//             rumqttc::Outgoing::PingResp => false,
//             _ => true,
//         },
//     }
// }
