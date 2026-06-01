use tokio::sync::mpsc::unbounded_channel;
use wallet_api::{error::service::ServiceError, messaging::notify::FrontendNotifyEvent};
use wallet_database::{
    ApiTransactionDbPool,
    entities::{api_collect::ApiCollectStatus, api_wallet::ApiWalletType},
};

use crate::harness::{ensure_worker_env, worker::WorkerTestEnv};

use super::{
    db::{load_collect, open_transaction_pool, seed_wallet},
    fixtures::{
        COLLECT_CHAIN, COLLECT_SYMBOL, COLLECT_VALIDATE, COLLECT_VALUE, CollectOrderFixture,
    },
    inbox::CollectNotificationInbox,
};

pub(crate) struct CollectNotificationScenario {
    env: &'static WorkerTestEnv,
    tx_pool: ApiTransactionDbPool,
}

impl CollectNotificationScenario {
    pub(crate) async fn new() -> Self {
        let env = ensure_worker_env().await;
        env.recorder.reset();

        let tx_pool = open_transaction_pool(&env.db_dir).await;

        Self { env, tx_pool }
    }

    pub(crate) async fn given_sub_account_wallet(&self, order: &CollectOrderFixture) {
        seed_wallet(
            &self.env.db_dir,
            &order.uid,
            "collect-notify-wallet",
            ApiWalletType::SubAccount,
        )
        .await;
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

    pub(crate) async fn given_frontend_notification_collector(&self) -> CollectNotificationInbox {
        let (tx, rx) = unbounded_channel::<FrontendNotifyEvent>();

        self.env
            ._manager
            .set_frontend_notify_sender(tx)
            .await
            .expect("install working frontend sender");

        CollectNotificationInbox { rx }
    }

    pub(crate) async fn when_collect_order_submitted(
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

    pub(crate) async fn when_collect_order_retried(&self, order: &CollectOrderFixture) {
        self.when_collect_order_submitted(order)
            .await
            .expect("retrying the same collect order should resend frontend notify");
    }

    pub(crate) async fn then_collect_order_is_retryable(&self, order: &CollectOrderFixture) {
        let persisted = load_collect(&self.tx_pool, &order.trade_no).await;

        assert_eq!(persisted.status, ApiCollectStatus::Init);
    }
}
