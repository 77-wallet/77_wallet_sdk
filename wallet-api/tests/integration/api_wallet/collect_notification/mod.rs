mod support;

use serial_test::serial;

use support::{
    CollectNotificationScenario, CollectOrderFixture, then_frontend_notification_failed,
};

#[serial]
#[tokio::test]
async fn collect_notification_retry_on_existing_trade_no() {
    let scenario = CollectNotificationScenario::new().await;
    let order = CollectOrderFixture::new("collect_notify_retry");

    scenario.given_sub_account_wallet(&order).await;
    scenario.given_frontend_notification_closed().await;

    let result = scenario.when_collect_order_submitted(&order).await;

    then_frontend_notification_failed(result);
    scenario.then_collect_order_is_retryable(&order).await;

    let mut notifications = scenario.given_frontend_notification_collector().await;

    scenario.when_collect_order_retried(&order).await;

    notifications.then_received_collect_order(&order).await;
}
