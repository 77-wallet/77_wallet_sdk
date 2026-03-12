use rumqttc::v5::mqttbytes::v5::Publish;
use wallet_database::{ApiWalletDbPool, TaskDbPool};

pub async fn exec_wallet_order_payload(payload: &serde_json::Value) -> Result<(), anyhow::Error> {
    let publish = Publish {
        dup: false,
        qos: rumqttc::v5::mqttbytes::QoS::AtLeastOnce,
        retain: false,
        topic: "wallet/order".into(),
        pkid: 0,
        payload: serde_json::to_vec(payload)?.into(),
        properties: Default::default(),
    };
    crate::messaging::mqtt::handle::exec_incoming_publish(&publish).await
}

pub fn api_wallet_pool() -> Result<ApiWalletDbPool, crate::error::service::ServiceError> {
    crate::context::CONTEXT.get().unwrap().api_wallet_pool()
}

pub fn task_pool() -> Result<TaskDbPool, crate::error::service::ServiceError> {
    crate::context::CONTEXT.get().unwrap().task_pool()
}
