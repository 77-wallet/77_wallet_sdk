use std::time::{Duration, Instant};

use tokio::sync::mpsc::unbounded_channel;
use wallet_api::{
    error::service::ServiceError,
    messaging::notify::FrontendNotifyEvent,
    testkit::withdraw::{
        scan_withdraw_intent_labels_for_trade_once,
        send_tx_ack_via_worker as send_withdraw_tx_ack_via_worker,
    },
};
use wallet_database::{
    ApiTransactionDbPool, ApiWalletDbPool,
    entities::{api_wallet::ApiWalletType, api_withdraw::ApiWithdrawStatus},
};

use crate::harness::{ensure_worker_env, open_api_wallet_pool, worker::WorkerTestEnv};

use super::{
    db::{insert_withdraw_order, load_withdraw, open_transaction_pool, seed_wallet},
    fixtures::{
        WITHDRAW_CHAIN, WITHDRAW_SYMBOL, WITHDRAW_VALIDATE, WITHDRAW_VALUE, WithdrawOrderFixture,
    },
    inbox::WithdrawNotificationInbox,
    recorder::count_withdraw_tx_ack_requests,
};

pub(crate) struct WithdrawNotificationScenario {
    env: &'static WorkerTestEnv,
    tx_pool: ApiTransactionDbPool,
    core_pool: ApiWalletDbPool,
}

impl WithdrawNotificationScenario {
    pub(crate) async fn new() -> Self {
        let env = ensure_worker_env().await;
        env.recorder.reset();

        let tx_pool = open_transaction_pool(&env.db_dir).await;
        let core_pool = open_api_wallet_pool(&env.db_dir).await;

        Self { env, tx_pool, core_pool }
    }

    pub(crate) async fn given_withdrawal_wallet(&self, order: &WithdrawOrderFixture) {
        seed_wallet(
            &self.env.db_dir,
            &order.uid,
            "withdraw-notify-wallet",
            ApiWalletType::Withdrawal,
        )
        .await;
    }

    pub(crate) async fn given_withdraw_order(&self, order: &WithdrawOrderFixture) {
        insert_withdraw_order(&self.tx_pool, order).await;
    }

    pub(crate) async fn given_frontend_notification_closed(&self) {
        let (tx, rx) = unbounded_channel::<FrontendNotifyEvent>();
        drop(rx);

        self.env
            ._manager
            .set_frontend_notify_sender(tx)
            .await
            .expect("install closed frontend sender");
    }

    pub(crate) async fn given_frontend_notification_collector(&self) -> WithdrawNotificationInbox {
        let (tx, rx) = unbounded_channel::<FrontendNotifyEvent>();

        self.env
            ._manager
            .set_frontend_notify_sender(tx)
            .await
            .expect("install working frontend sender");

        WithdrawNotificationInbox { rx }
    }

    pub(crate) fn given_backend_next_ack_fails(&self, status: i64, body: &str) {
        self.env.recorder.fail_next_api_backend_call(status, body);
    }

    pub(crate) async fn when_withdraw_order_submitted(
        &self,
        order: &WithdrawOrderFixture,
    ) -> Result<(), ServiceError> {
        self.env
            ._manager
            .api_withdrawal_order(
                &order.from_addr,
                &order.to_addr,
                WITHDRAW_VALUE,
                WITHDRAW_VALIDATE,
                WITHDRAW_CHAIN,
                None,
                WITHDRAW_SYMBOL,
                &order.trade_no,
                1,
                &order.uid,
            )
            .await
    }

    pub(crate) async fn when_withdraw_order_retried(&self, order: &WithdrawOrderFixture) {
        self.when_withdraw_order_submitted(order)
            .await
            .expect("retrying the same withdraw order should resend frontend notify");
    }

    pub(crate) async fn when_tx_ack_is_sent(
        &self,
        order: &WithdrawOrderFixture,
    ) -> Result<(), ServiceError> {
        send_withdraw_tx_ack_via_worker(
            self.tx_pool.clone(),
            self.core_pool.clone(),
            &order.trade_no,
        )
        .await
    }

    pub(crate) async fn then_withdraw_order_is_retryable_after_notification_failure(
        &self,
        order: &WithdrawOrderFixture,
    ) {
        let persisted = load_withdraw(&self.tx_pool, &order.trade_no).await;

        assert_eq!(persisted.init_status, ApiWithdrawStatus::AuditPass);
        assert_eq!(persisted.status, ApiWithdrawStatus::InitOrder);
    }

    pub(crate) async fn then_backend_tx_ack_attempted_once(&self, order: &WithdrawOrderFixture) {
        let tx_ack_request_count = self.wait_for_tx_ack_count(&order.trade_no).await;

        assert_eq!(tx_ack_request_count, 1, "withdraw order should emit 1 TX ack request");
    }

    pub(crate) async fn then_backend_tx_ack_remains_once_after_quiet_period(
        &self,
        order: &WithdrawOrderFixture,
    ) {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let tx_ack_request_count = self.tx_ack_request_count(&order.trade_no);

        assert_eq!(
            tx_ack_request_count, 1,
            "withdraw order should not emit a second TX ack request"
        );
    }

    pub(crate) async fn then_tx_ack_fact_is_persisted(&self, order: &WithdrawOrderFixture) {
        let persisted = load_withdraw(&self.tx_pool, &order.trade_no).await;
        assert!(
            persisted.tx_ack_sent_at.is_some(),
            "successful tx ack should persist tx_ack_sent_at"
        );
    }

    pub(crate) async fn then_tx_ack_fact_is_not_persisted(&self, order: &WithdrawOrderFixture) {
        let persisted = load_withdraw(&self.tx_pool, &order.trade_no).await;
        assert!(
            persisted.tx_ack_sent_at.is_none(),
            "failed backend ACK must not persist tx_ack_sent_at"
        );
    }

    pub(crate) async fn then_scanner_will_not_retry_tx_ack(&self, order: &WithdrawOrderFixture) {
        let labels = self.scan_intent_labels(&order.trade_no).await;
        assert!(
            labels.iter().all(|label| label != "SendTxAck"),
            "withdraw with tx_ack_sent_at must not re-enter SendTxAck; labels: {labels:?}"
        );
    }

    pub(crate) async fn then_scanner_can_retry_tx_ack(&self, order: &WithdrawOrderFixture) {
        let labels = self.scan_intent_labels(&order.trade_no).await;
        assert!(
            labels.iter().any(|label| label == "SendTxAck"),
            "withdraw with failed TX ACK should stay retryable; labels: {labels:?}"
        );
    }

    async fn scan_intent_labels(&self, trade_no: &str) -> Vec<String> {
        scan_withdraw_intent_labels_for_trade_once(self.tx_pool.clone(), trade_no)
            .await
            .expect("scan withdraw intents")
    }

    async fn wait_for_tx_ack_count(&self, trade_no: &str) -> usize {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let count = self.tx_ack_request_count(trade_no);
            if count > 0 || Instant::now() >= deadline {
                return count;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    fn tx_ack_request_count(&self, trade_no: &str) -> usize {
        let requests = self.env.recorder.snapshot();
        count_withdraw_tx_ack_requests(&requests, trade_no)
    }
}
