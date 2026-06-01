use wallet_api::testkit::collect::shadow_collect_check_fee;
use wallet_database::{
    ApiTransactionDbPool, SqliteContext, entities::api_collect::ApiCollectEntity,
    repositories::api_wallet::collect::ApiCollectRepo,
};

use crate::harness::{WorkerTestEnv, ensure_worker_env, next_unique_id};

use super::super::super::support::{
    build_shadow_collect_worker, install_collect_test_adapter, seed_collect_order,
};

pub(crate) struct CollectBuildFeeScenario {
    env: &'static WorkerTestEnv,
    collect_pool: ApiTransactionDbPool,
}

impl CollectBuildFeeScenario {
    pub(crate) async fn new() -> Self {
        let env = ensure_worker_env().await;
        let collect_pool = open_collect_pool(env).await;
        Self { env, collect_pool }
    }

    pub(crate) fn given_low_balance_sol_adapter(&self) -> impl Drop {
        install_collect_test_adapter(false, 0)
    }

    pub(crate) async fn given_sol_collect_order(
        &self,
        trade_prefix: &str,
        to_addr: &str,
    ) -> ApiCollectEntity {
        let trade_no = format!("{trade_prefix}_{}", next_unique_id());
        seed_collect_order(&self.collect_pool, &trade_no, to_addr).await
    }

    pub(crate) async fn given_completed_fee_cycle_facts(&self, collect: &ApiCollectEntity) {
        sqlx::query(
            r#"
            UPDATE api_collect
            SET service_fee_uploaded_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                tx_fee_res_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                need_service_fee = false,
                ever_needed_service_fee = true,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = ?
            "#,
        )
        .bind(&collect.trade_no)
        .execute(self.collect_pool.as_ref())
        .await
        .expect("seed fee cycle facts");
    }

    pub(crate) async fn given_collect_reloaded(
        &self,
        collect: &ApiCollectEntity,
    ) -> ApiCollectEntity {
        self.load_collect(&collect.trade_no).await
    }

    pub(crate) async fn when_fee_check_runs(&self, collect: &ApiCollectEntity) -> bool {
        let worker = build_shadow_collect_worker(self.env).await;
        shadow_collect_check_fee(&worker, collect)
            .await
            .expect("fee check should return a boolean result")
    }

    pub(crate) async fn when_build_attempt_is_invalidated(
        &self,
        collect: &ApiCollectEntity,
    ) -> u64 {
        let worker = build_shadow_collect_worker(self.env).await;
        worker
            .invalidate_build_attempt_after_fee_check_failure(collect)
            .await
            .expect("fee check failure should invalidate build attempt")
    }

    pub(crate) fn then_fee_check_failed(&self, pass: bool, message: &str) {
        assert!(!pass, "{message}");
    }

    pub(crate) fn then_one_row_was_affected(&self, affected: u64) {
        assert_eq!(affected, 1);
    }

    pub(crate) async fn then_fee_cycle_is_reopened(&self, collect: &ApiCollectEntity) {
        let persisted = self.load_collect(&collect.trade_no).await;
        assert_eq!(persisted.need_service_fee, Some(true));
        assert!(persisted.service_fee_uploaded_at.is_none());
        assert!(persisted.raw_tx.is_none());
        assert!(persisted.tx_hash.is_none());
    }

    pub(crate) async fn then_completed_fee_cycle_facts_are_preserved(
        &self,
        collect: &ApiCollectEntity,
    ) {
        let persisted = self.load_collect(&collect.trade_no).await;
        assert_eq!(persisted.need_service_fee, Some(false));
        assert!(persisted.service_fee_uploaded_at.is_some());
        assert!(persisted.tx_fee_res_ack_sent_at.is_some());
        assert!(persisted.raw_tx.is_none());
        assert!(persisted.tx_hash.is_none());
    }

    async fn load_collect(&self, trade_no: &str) -> ApiCollectEntity {
        ApiCollectRepo::get_api_collect_by_trade_no(&self.collect_pool, trade_no)
            .await
            .expect("load collect")
    }
}

async fn open_collect_pool(env: &WorkerTestEnv) -> ApiTransactionDbPool {
    let collect_pool_ctx =
        SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
            .await
            .expect("open api transaction sqlite");
    collect_pool_ctx.into_transaction_db_pool().expect("transaction pool")
}
