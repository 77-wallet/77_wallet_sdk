use std::{
    path::Path,
    time::{Duration, Instant},
};

use crate::harness::{
    SMOKE_WALLET_PASSWORD, decrypt_captured_api_backend_body, ensure_worker_env, next_unique_id,
    open_api_wallet_pool,
};
use serial_test::serial;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use wallet_api::{
    error::service::ServiceError,
    messaging::notify::FrontendNotifyEvent,
    testkit::withdraw::{
        scan_withdraw_intent_labels_for_trade_once,
        send_tx_ack_via_worker as send_withdraw_tx_ack_via_worker,
    },
};
use wallet_database::{
    ApiTransactionDbPool, ApiWalletDbPool, SqliteContext,
    entities::{
        api_trade_type::ApiTradeType,
        api_wallet::ApiWalletType,
        api_withdraw::{ApiWithdrawEntity, ApiWithdrawStatus},
    },
    repositories::api_wallet::{wallet::ApiWalletRepo, withdraw::ApiWithdrawRepo},
};

use crate::harness::worker::{CapturedHttpRequest, WorkerTestEnv};

const WITHDRAW_NOTIFICATION_TEST_SN: &str = "withdraw-notification-test-sn";
const WITHDRAW_VALUE: &str = "56.78";
const WITHDRAW_VALIDATE: &str = "digest";
const WITHDRAW_CHAIN: &str = "sol";
const WITHDRAW_SYMBOL: &str = "USDC";

struct WithdrawOrderFixture {
    uid: String,
    trade_no: String,
    from_addr: String,
    to_addr: String,
}

impl WithdrawOrderFixture {
    fn new(prefix: &str) -> Self {
        let id = next_unique_id();
        Self {
            uid: format!("uid_{prefix}_{id}"),
            trade_no: format!("T_{prefix}_{id}"),
            from_addr: format!("from-{prefix}-{id}"),
            to_addr: format!("to-{prefix}-{id}"),
        }
    }
}

struct WithdrawNotificationScenario {
    env: &'static WorkerTestEnv,
    tx_pool: ApiTransactionDbPool,
    core_pool: ApiWalletDbPool,
}

impl WithdrawNotificationScenario {
    async fn new() -> Self {
        let env = ensure_worker_env().await;
        env.recorder.reset();

        let tx_pool = open_transaction_pool(&env.db_dir).await;
        let core_pool = open_api_wallet_pool(&env.db_dir).await;

        Self { env, tx_pool, core_pool }
    }

    async fn given_withdrawal_wallet(&self, order: &WithdrawOrderFixture) {
        seed_wallet(
            &self.env.db_dir,
            &order.uid,
            "withdraw-notify-wallet",
            ApiWalletType::Withdrawal,
        )
        .await;
    }

    async fn given_withdraw_order(&self, order: &WithdrawOrderFixture) {
        ApiWithdrawRepo::upsert_api_withdraw(
            &self.tx_pool,
            &order.uid,
            "withdraw",
            &order.from_addr,
            &order.to_addr,
            WITHDRAW_VALUE,
            WITHDRAW_VALIDATE,
            WITHDRAW_CHAIN,
            None,
            WITHDRAW_SYMBOL,
            &order.trade_no,
            None,
            None,
            None,
            ApiTradeType::Withdraw,
            1,
            None,
            ApiWithdrawStatus::AuditPass,
            ApiWithdrawStatus::InitOrder,
            "",
            "",
            None,
            None,
        )
        .await
        .expect("insert withdraw");
    }

    async fn given_frontend_notification_closed(&self) {
        let (tx, rx) = unbounded_channel::<FrontendNotifyEvent>();
        drop(rx);

        self.env
            ._manager
            .set_frontend_notify_sender(tx)
            .await
            .expect("install closed frontend sender");
    }

    async fn given_frontend_notification_collector(&self) -> WithdrawNotificationInbox {
        let (tx, rx) = unbounded_channel::<FrontendNotifyEvent>();

        self.env
            ._manager
            .set_frontend_notify_sender(tx)
            .await
            .expect("install working frontend sender");

        WithdrawNotificationInbox { rx }
    }

    fn given_backend_next_ack_fails(&self, status: i64, body: &str) {
        self.env.recorder.fail_next_api_backend_call(status, body);
    }

    async fn when_withdraw_order_submitted(
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

    async fn when_withdraw_order_retried(&self, order: &WithdrawOrderFixture) {
        self.when_withdraw_order_submitted(order)
            .await
            .expect("retrying the same withdraw order should resend frontend notify");
    }

    async fn when_tx_ack_is_sent(&self, order: &WithdrawOrderFixture) -> Result<(), ServiceError> {
        send_withdraw_tx_ack_via_worker(
            self.tx_pool.clone(),
            self.core_pool.clone(),
            &order.trade_no,
        )
        .await
    }

    async fn then_withdraw_order_is_retryable_after_notification_failure(
        &self,
        order: &WithdrawOrderFixture,
    ) {
        let persisted = self.load_withdraw(&order.trade_no).await;

        assert_eq!(persisted.init_status, ApiWithdrawStatus::AuditPass);
        assert_eq!(persisted.status, ApiWithdrawStatus::InitOrder);
    }

    async fn then_backend_tx_ack_attempted_once(&self, order: &WithdrawOrderFixture) {
        let tx_ack_request_count = self.wait_for_tx_ack_count(&order.trade_no).await;

        assert_eq!(tx_ack_request_count, 1, "withdraw order should emit 1 TX ack request");
    }

    async fn then_backend_tx_ack_remains_once_after_quiet_period(
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

    async fn then_tx_ack_fact_is_persisted(&self, order: &WithdrawOrderFixture) {
        let persisted = self.load_withdraw(&order.trade_no).await;
        assert!(
            persisted.tx_ack_sent_at.is_some(),
            "successful tx ack should persist tx_ack_sent_at"
        );
    }

    async fn then_tx_ack_fact_is_not_persisted(&self, order: &WithdrawOrderFixture) {
        let persisted = self.load_withdraw(&order.trade_no).await;
        assert!(
            persisted.tx_ack_sent_at.is_none(),
            "failed backend ACK must not persist tx_ack_sent_at"
        );
    }

    async fn then_scanner_will_not_retry_tx_ack(&self, order: &WithdrawOrderFixture) {
        let labels = self.scan_intent_labels(&order.trade_no).await;
        assert!(
            labels.iter().all(|label| label != "SendTxAck"),
            "withdraw with tx_ack_sent_at must not re-enter SendTxAck; labels: {labels:?}"
        );
    }

    async fn then_scanner_can_retry_tx_ack(&self, order: &WithdrawOrderFixture) {
        let labels = self.scan_intent_labels(&order.trade_no).await;
        assert!(
            labels.iter().any(|label| label == "SendTxAck"),
            "withdraw with failed TX ACK should stay retryable; labels: {labels:?}"
        );
    }

    async fn load_withdraw(&self, trade_no: &str) -> ApiWithdrawEntity {
        ApiWithdrawRepo::get_api_withdraw_by_trade_no(
            &self.tx_pool,
            trade_no,
            ApiTradeType::Withdraw,
        )
        .await
        .expect("load withdraw")
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

struct WithdrawNotificationInbox {
    rx: UnboundedReceiver<FrontendNotifyEvent>,
}

impl WithdrawNotificationInbox {
    async fn then_received_withdraw_order(&mut self, order: &WithdrawOrderFixture) {
        let notify = tokio::time::timeout(Duration::from_secs(1), self.rx.recv())
            .await
            .expect("timed out waiting for withdraw notify")
            .expect("missing withdraw notify event");

        let notify_json = serde_json::to_value(&notify).expect("serialize withdraw notify");

        assert_eq!(notify_json["event"], "WITHDRAW");
        assert_eq!(notify_json["data"]["uid"], order.uid);
        assert_eq!(notify_json["data"]["fromAddr"], order.from_addr);
        assert_eq!(notify_json["data"]["toAddr"], order.to_addr);
        assert_eq!(notify_json["data"]["value"], WITHDRAW_VALUE);
    }
}

async fn open_transaction_pool(db_dir: &Path) -> ApiTransactionDbPool {
    let tx_pool_ctx = SqliteContext::new(&db_dir.to_string_lossy(), Some("api_transaction.db"))
        .await
        .expect("open api transaction sqlite");
    tx_pool_ctx.into_transaction_db_pool().expect("transaction pool")
}

async fn seed_wallet(
    db_dir: &Path,
    uid: &str,
    wallet_name: &str,
    wallet_type: ApiWalletType,
) -> String {
    let pool = open_api_wallet_pool(db_dir).await;
    let address = format!("0xwallet{:016x}", next_unique_id());
    let seed_enc = wallet_api::testkit::seed::encrypt_seed(SMOKE_WALLET_PASSWORD, b"seed").await;
    ApiWalletRepo::upsert(
        &pool,
        uid,
        wallet_name,
        &address,
        b"phrase",
        &seed_enc,
        wallet_type,
        None,
        WITHDRAW_NOTIFICATION_TEST_SN,
        0,
    )
    .await
    .expect("seed wallet");
    address
}

fn count_withdraw_tx_ack_requests(requests: &[CapturedHttpRequest], trade_no: &str) -> usize {
    requests
        .iter()
        .filter(|req| {
            req.path
                .contains(wallet_transport_backend::consts::endpoint::api_wallet::TRANS_EVENT_ACK)
        })
        .filter(|req| {
            let payload = decrypt_captured_api_backend_body(&req.body);
            payload["tradeNo"].as_str() == Some(trade_no)
                && payload["ackType"].as_str() == Some("TX")
                && payload["type"].as_str() == Some("WD")
        })
        .count()
}

fn then_frontend_notification_failed(result: Result<(), ServiceError>) {
    assert!(result.is_err(), "frontend notify failure should bubble up");
}

fn then_tx_ack_sent(result: Result<(), ServiceError>) {
    result.expect("send withdraw tx ack");
}

fn then_worker_left_flow_retryable(result: Result<(), ServiceError>) {
    result.expect("backend ack failure should leave the worker retryable");
}

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
