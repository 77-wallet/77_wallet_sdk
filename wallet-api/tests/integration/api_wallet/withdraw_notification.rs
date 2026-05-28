use std::{
    path::Path,
    time::{Duration, Instant},
};

use crate::harness::{
    SMOKE_WALLET_PASSWORD, decrypt_captured_api_backend_body, ensure_worker_env, next_unique_id,
    open_api_wallet_pool,
};
use serial_test::serial;
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

const TEST_SN: &str = "collect-worker-test-sn";
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

struct WithdrawNotificationTest {
    env: &'static WorkerTestEnv,
    tx_pool: ApiTransactionDbPool,
    core_pool: ApiWalletDbPool,
}

impl WithdrawNotificationTest {
    async fn new() -> Self {
        let env = ensure_worker_env().await;
        env.recorder.reset();

        let tx_pool = open_transaction_pool(&env.db_dir).await;
        let core_pool = open_api_wallet_pool(&env.db_dir).await;

        Self { env, tx_pool, core_pool }
    }

    async fn seed_withdrawal_wallet(&self, uid: &str) -> String {
        seed_wallet(&self.env.db_dir, uid, "withdraw-notify-wallet", ApiWalletType::Withdrawal)
            .await
    }

    async fn seed_withdraw_order(&self, order: &WithdrawOrderFixture) {
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

    async fn submit_withdraw_order(
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

    async fn send_tx_ack(&self, trade_no: &str) -> Result<(), ServiceError> {
        send_withdraw_tx_ack_via_worker(self.tx_pool.clone(), self.core_pool.clone(), trade_no)
            .await
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
        TEST_SN,
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

#[serial]
#[tokio::test]
async fn withdraw_notification_retry_on_existing_trade_no() {
    // Arrange: environment
    let t = WithdrawNotificationTest::new().await;

    // Arrange: data and failing frontend notification channel
    let order = WithdrawOrderFixture::new("withdraw_notify_retry");
    let _wallet_addr = t.seed_withdrawal_wallet(&order.uid).await;

    let (fail_tx, fail_rx) = tokio::sync::mpsc::unbounded_channel::<FrontendNotifyEvent>();
    drop(fail_rx);
    t.env
        ._manager
        .set_frontend_notify_sender(fail_tx)
        .await
        .expect("install failing frontend sender");

    // Act: first submit fails after the withdraw row is persisted.
    let first = t.submit_withdraw_order(&order).await;

    // Assert: failed notification bubbles up and leaves retryable DB facts.
    assert!(first.is_err(), "frontend notify failure should bubble up");

    let persisted = t.load_withdraw(&order.trade_no).await;
    assert_eq!(persisted.init_status, ApiWithdrawStatus::AuditPass);
    assert_eq!(persisted.status, ApiWithdrawStatus::InitOrder);

    // Arrange: restore a working notification channel.
    let (ok_tx, mut ok_rx) = tokio::sync::mpsc::unbounded_channel::<FrontendNotifyEvent>();
    t.env
        ._manager
        .set_frontend_notify_sender(ok_tx)
        .await
        .expect("install working frontend sender");

    // Act: retry the same order.
    t.submit_withdraw_order(&order)
        .await
        .expect("retrying the same withdraw order should resend frontend notify");

    // Assert: retry emits the expected frontend notification.
    let notify = tokio::time::timeout(Duration::from_secs(1), ok_rx.recv())
        .await
        .expect("timed out waiting for withdraw notify")
        .expect("missing withdraw notify event");
    let notify_json = serde_json::to_value(&notify).expect("serialize withdraw notify");
    assert_eq!(notify_json["event"], "WITHDRAW");
    assert_eq!(notify_json["data"]["uid"], order.uid);
    assert_eq!(notify_json["data"]["fromAddr"], order.from_addr);
    assert_eq!(notify_json["data"]["toAddr"], order.to_addr);
    assert_eq!(notify_json["data"]["value"], WITHDRAW_VALUE);

    // Act: send TX ACK after the retry path.
    t.send_tx_ack(&order.trade_no).await.expect("send withdraw tx ack after retry");

    // Assert: retrying the order still produces only one backend TX ACK.
    let tx_ack_request_count = t.wait_for_tx_ack_count(&order.trade_no).await;
    assert_eq!(
        tx_ack_request_count, 1,
        "retrying the same withdraw order should still emit only one TX ack request"
    );
}

#[serial]
#[tokio::test]
async fn withdraw_tx_ack_sends_once_and_persists_fact() {
    // Arrange: environment
    let t = WithdrawNotificationTest::new().await;

    // Arrange: data
    let order = WithdrawOrderFixture::new("withdraw_ack");
    t.seed_withdraw_order(&order).await;

    // Act
    t.send_tx_ack(&order.trade_no).await.expect("send withdraw tx ack");

    // Assert: backend side effect
    let tx_ack_request_count = t.wait_for_tx_ack_count(&order.trade_no).await;
    assert_eq!(tx_ack_request_count, 1, "withdraw order should emit exactly one TX ack request");

    // Assert: DB fact and scanner state
    let persisted = t.load_withdraw(&order.trade_no).await;
    assert!(persisted.tx_ack_sent_at.is_some(), "successful tx ack should persist tx_ack_sent_at");

    let labels = t.scan_intent_labels(&order.trade_no).await;
    assert!(
        labels.iter().all(|label| label != "SendTxAck"),
        "withdraw with tx_ack_sent_at must not re-enter SendTxAck; labels: {labels:?}"
    );

    // Assert: repeated act stays idempotent
    t.send_tx_ack(&order.trade_no).await.expect("repeat withdraw tx ack should be idempotent");

    tokio::time::sleep(Duration::from_millis(500)).await;
    let tx_ack_request_count = t.tx_ack_request_count(&order.trade_no);
    assert_eq!(tx_ack_request_count, 1, "withdraw order should not emit a second TX ack request");
}

#[serial]
#[tokio::test]
async fn withdraw_tx_ack_backend_failure_keeps_fact_unset_and_retryable() {
    // Arrange: environment
    let t = WithdrawNotificationTest::new().await;

    // Arrange: data and fake backend failure
    let order = WithdrawOrderFixture::new("withdraw_ack_fail");
    t.seed_withdraw_order(&order).await;
    t.env.recorder.fail_next_api_backend_call(503, "ack unavailable");

    // Act
    t.send_tx_ack(&order.trade_no)
        .await
        .expect("backend ack failure should leave the worker retryable");

    // Assert: backend was called, but the durable ACK fact was not written.
    let tx_ack_request_count = t.wait_for_tx_ack_count(&order.trade_no).await;
    assert_eq!(tx_ack_request_count, 1, "withdraw order should attempt one TX ACK request");

    // Assert: DB fact remains unset
    let persisted = t.load_withdraw(&order.trade_no).await;
    assert!(
        persisted.tx_ack_sent_at.is_none(),
        "failed backend ACK must not persist tx_ack_sent_at"
    );

    // Assert: scanner can retry the ACK side effect
    let labels = t.scan_intent_labels(&order.trade_no).await;
    assert!(
        labels.iter().any(|label| label == "SendTxAck"),
        "withdraw with failed TX ACK should stay retryable; labels: {labels:?}"
    );
}
