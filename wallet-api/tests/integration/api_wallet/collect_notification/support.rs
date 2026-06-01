use std::{path::Path, time::Duration};

use crate::harness::{
    SMOKE_WALLET_PASSWORD, ensure_worker_env, next_unique_id, open_api_wallet_pool,
    worker::WorkerTestEnv,
};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use wallet_api::{error::service::ServiceError, messaging::notify::FrontendNotifyEvent};
use wallet_database::{
    ApiTransactionDbPool, SqliteContext,
    entities::{
        api_collect::{ApiCollectEntity, ApiCollectStatus},
        api_wallet::ApiWalletType,
    },
    repositories::api_wallet::{collect::ApiCollectRepo, wallet::ApiWalletRepo},
};

const COLLECT_NOTIFICATION_TEST_SN: &str = "collect-notification-test-sn";
const COLLECT_VALUE: &str = "12.34";
const COLLECT_VALIDATE: &str = "digest";
const COLLECT_CHAIN: &str = "sol";
const COLLECT_SYMBOL: &str = "USDC";

pub(super) struct CollectOrderFixture {
    pub(super) uid: String,
    trade_no: String,
    from_addr: String,
    to_addr: String,
}

impl CollectOrderFixture {
    pub(super) fn new(prefix: &str) -> Self {
        let id = next_unique_id();
        Self {
            uid: format!("uid_{prefix}_{id}"),
            trade_no: format!("T_{prefix}_{id}"),
            from_addr: format!("from-{prefix}-{id}"),
            to_addr: format!("to-{prefix}-{id}"),
        }
    }
}

pub(super) struct CollectNotificationScenario {
    env: &'static WorkerTestEnv,
    tx_pool: ApiTransactionDbPool,
}

impl CollectNotificationScenario {
    pub(super) async fn new() -> Self {
        let env = ensure_worker_env().await;
        env.recorder.reset();

        let tx_pool = open_transaction_pool(&env.db_dir).await;

        Self { env, tx_pool }
    }

    pub(super) async fn given_sub_account_wallet(&self, order: &CollectOrderFixture) {
        seed_wallet(
            &self.env.db_dir,
            &order.uid,
            "collect-notify-wallet",
            ApiWalletType::SubAccount,
        )
        .await;
    }

    pub(super) async fn given_frontend_notification_closed(&self) {
        let (tx, rx) = unbounded_channel::<FrontendNotifyEvent>();
        drop(rx);

        self.env
            ._manager
            .set_frontend_notify_sender(tx)
            .await
            .expect("install closed frontend sender");
    }

    pub(super) async fn given_frontend_notification_collector(&self) -> CollectNotificationInbox {
        let (tx, rx) = unbounded_channel::<FrontendNotifyEvent>();

        self.env
            ._manager
            .set_frontend_notify_sender(tx)
            .await
            .expect("install working frontend sender");

        CollectNotificationInbox { rx }
    }

    pub(super) async fn when_collect_order_submitted(
        &self,
        order: &CollectOrderFixture,
    ) -> Result<(), ServiceError> {
        self.env
            ._manager
            .api_collect_order(
                &order.from_addr,
                &order.to_addr,
                COLLECT_VALUE,
                COLLECT_VALIDATE,
                COLLECT_CHAIN,
                None,
                COLLECT_SYMBOL,
                &order.trade_no,
                2,
                &order.uid,
            )
            .await
    }

    pub(super) async fn when_collect_order_retried(&self, order: &CollectOrderFixture) {
        self.when_collect_order_submitted(order)
            .await
            .expect("retrying the same collect order should resend frontend notify");
    }

    pub(super) async fn then_collect_order_is_retryable(&self, order: &CollectOrderFixture) {
        let persisted = self.load_collect(&order.trade_no).await;

        assert_eq!(persisted.status, ApiCollectStatus::Init);
    }

    async fn load_collect(&self, trade_no: &str) -> ApiCollectEntity {
        ApiCollectRepo::get_api_collect_by_trade_no(&self.tx_pool, trade_no)
            .await
            .expect("load collect")
    }
}

pub(super) struct CollectNotificationInbox {
    rx: UnboundedReceiver<FrontendNotifyEvent>,
}

impl CollectNotificationInbox {
    pub(super) async fn then_received_collect_order(&mut self, order: &CollectOrderFixture) {
        let notify = tokio::time::timeout(Duration::from_secs(1), self.rx.recv())
            .await
            .expect("timed out waiting for collect notify")
            .expect("missing collect notify event");

        let notify_json = serde_json::to_value(&notify).expect("serialize collect notify");

        assert_eq!(notify_json["event"], "COLLECT");
        assert_eq!(notify_json["data"]["uid"], order.uid);
        assert_eq!(notify_json["data"]["fromAddr"], order.from_addr);
        assert_eq!(notify_json["data"]["toAddr"], order.to_addr);
        assert_eq!(notify_json["data"]["value"], COLLECT_VALUE);
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
        COLLECT_NOTIFICATION_TEST_SN,
        0,
    )
    .await
    .expect("seed wallet");
    address
}

pub(super) fn then_frontend_notification_failed(result: Result<(), ServiceError>) {
    assert!(result.is_err(), "frontend notify failure should bubble up");
}
