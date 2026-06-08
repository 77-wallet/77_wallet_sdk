//! Test-only MQTT message execution helpers.

use crate::context::Context;
use rumqttc::v5::mqttbytes::v5::Publish;
use wallet_database::{ApiTransactionDbPool, ApiWalletDbPool, DbPool, TaskDbPool};

pub async fn exec_wallet_order_payload(
    ctx: &'static Context,
    payload: &serde_json::Value,
) -> Result<(), anyhow::Error> {
    let publish = Publish {
        dup: false,
        qos: rumqttc::v5::mqttbytes::QoS::AtLeastOnce,
        retain: false,
        topic: "wallet/order".into(),
        pkid: 0,
        payload: serde_json::to_vec(payload)?.into(),
        properties: Default::default(),
    };
    crate::messaging::mqtt::handle::exec_incoming_publish(ctx, &publish).await
}

pub fn api_wallet_pool(
    ctx: &'static Context,
) -> Result<ApiWalletDbPool, crate::error::service::ServiceError> {
    ctx.api_wallet_pool()
}

pub fn api_transaction_pool(
    ctx: &'static Context,
) -> Result<ApiTransactionDbPool, crate::error::service::ServiceError> {
    ctx.api_transaction_pool()
}

pub fn task_pool(ctx: &'static Context) -> Result<TaskDbPool, crate::error::service::ServiceError> {
    ctx.task_pool()
}

pub fn core_pool(ctx: &'static Context) -> Result<DbPool, crate::error::service::ServiceError> {
    ctx.get_global_sqlite_pool()
}
