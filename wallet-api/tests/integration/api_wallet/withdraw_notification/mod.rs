mod support;

use serial_test::serial;

use support::{
    WithdrawNotificationScenario, WithdrawOrderFixture, then_frontend_notification_failed,
    then_tx_ack_sent, then_worker_left_flow_retryable,
};

#[serial]
#[tokio::test]
async fn withdraw_notification_retry_on_existing_trade_no() {
    let scenario = WithdrawNotificationScenario::new().await;
    let order = WithdrawOrderFixture::new("withdraw_notify_retry");

    scenario.given_withdrawal_wallet(&order).await;
    scenario.given_frontend_notification_closed().await;

    let result = scenario.when_withdraw_order_submitted(&order).await;

    then_frontend_notification_failed(result);
    scenario.then_withdraw_order_is_retryable_after_notification_failure(&order).await;

    let mut notifications = scenario.given_frontend_notification_collector().await;

    scenario.when_withdraw_order_retried(&order).await;

    notifications.then_received_withdraw_order(&order).await;

    let ack_result = scenario.when_tx_ack_is_sent(&order).await;

    then_tx_ack_sent(ack_result);
    scenario.then_backend_tx_ack_attempted_once(&order).await;
}

#[serial]
#[tokio::test]
async fn withdraw_tx_ack_sends_once_and_persists_fact() {
    let scenario = WithdrawNotificationScenario::new().await;
    let order = WithdrawOrderFixture::new("withdraw_ack");

    scenario.given_withdraw_order(&order).await;

    let result = scenario.when_tx_ack_is_sent(&order).await;

    then_tx_ack_sent(result);
    scenario.then_backend_tx_ack_attempted_once(&order).await;
    scenario.then_tx_ack_fact_is_persisted(&order).await;
    scenario.then_scanner_will_not_retry_tx_ack(&order).await;

    let repeat_result = scenario.when_tx_ack_is_sent(&order).await;

    then_tx_ack_sent(repeat_result);
    scenario.then_backend_tx_ack_remains_once_after_quiet_period(&order).await;
}

#[serial]
#[tokio::test]
async fn withdraw_tx_ack_backend_failure_keeps_fact_unset_and_retryable() {
    let scenario = WithdrawNotificationScenario::new().await;
    let order = WithdrawOrderFixture::new("withdraw_ack_fail");

    scenario.given_withdraw_order(&order).await;
    scenario.given_backend_next_ack_fails(503, "ack unavailable");

    let result = scenario.when_tx_ack_is_sent(&order).await;

    then_worker_left_flow_retryable(result);
    scenario.then_backend_tx_ack_attempted_once(&order).await;
    scenario.then_tx_ack_fact_is_not_persisted(&order).await;
    scenario.then_scanner_can_retry_tx_ack(&order).await;
}
