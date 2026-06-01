use alloy::primitives::U256;
use wallet_api::{error::service::ServiceError, testkit::collect::shadow_collect_check_fee};
use wallet_database::{
    ApiTransactionDbPool, SqliteContext,
    entities::api_collect::{ApiCollectEntity, ApiCollectStatus},
    repositories::api_wallet::collect::ApiCollectRepo,
};

use crate::harness::{WorkerTestEnv, ensure_worker_env, next_unique_id};

use super::super::super::support::{
    build_eth_shadow_collect_worker, build_shadow_collect_worker, install_collect_eth_test_adapter,
    install_collect_test_adapter, install_collect_test_adapter_fee_shortage, seed_collect_order,
    seed_eth_collect_order,
};

pub(crate) struct CollectFeeCheckScenario {
    env: &'static WorkerTestEnv,
    collect_pool: ApiTransactionDbPool,
}

impl CollectFeeCheckScenario {
    pub(crate) async fn new() -> Self {
        let env = ensure_worker_env().await;
        let collect_pool = open_collect_pool(env).await;
        Self { env, collect_pool }
    }

    pub(crate) fn given_sol_recipient_adapter(
        &self,
        recipient_missing: bool,
        balance: u64,
    ) -> impl Drop {
        install_collect_test_adapter(recipient_missing, balance)
    }

    pub(crate) fn given_sol_fee_shortage_adapter(
        &self,
        recipient_missing: bool,
        balance: u64,
    ) -> impl Drop {
        install_collect_test_adapter_fee_shortage(recipient_missing, balance)
    }

    pub(crate) fn given_eth_fee_adapter(&self, balance_wei: u128, fee_amount: f64) -> impl Drop {
        install_collect_eth_test_adapter(U256::from(balance_wei), fee_amount)
    }

    pub(crate) async fn given_sol_collect_order(
        &self,
        trade_prefix: &str,
        to_addr: &str,
    ) -> ApiCollectEntity {
        let trade_no = format!("{trade_prefix}_{}", next_unique_id());
        seed_collect_order(&self.collect_pool, &trade_no, to_addr).await
    }

    pub(crate) async fn given_eth_collect_order(
        &self,
        trade_prefix: &str,
        from_addr: &str,
        to_addr: &str,
        value: &str,
    ) -> ApiCollectEntity {
        let trade_no = format!("{trade_prefix}_{}", next_unique_id());
        seed_eth_collect_order(&self.collect_pool, &trade_no, from_addr, to_addr, value).await
    }

    pub(crate) async fn when_sol_fee_check_runs(&self, collect: &ApiCollectEntity) -> bool {
        let worker = build_shadow_collect_worker(self.env).await;
        shadow_collect_check_fee(&worker, collect)
            .await
            .expect("collect fee check should return a boolean result")
    }

    pub(crate) async fn when_sol_fee_check_fails(
        &self,
        collect: &ApiCollectEntity,
    ) -> ServiceError {
        let worker = build_shadow_collect_worker(self.env).await;
        shadow_collect_check_fee(&worker, collect).await.expect_err("collect fee check should fail")
    }

    pub(crate) async fn when_eth_fee_check_runs(&self, collect: &ApiCollectEntity) -> bool {
        let worker = build_eth_shadow_collect_worker(self.env).await;
        shadow_collect_check_fee(&worker, collect)
            .await
            .expect("ETH collect fee check should return a boolean result")
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

    pub(crate) fn then_error_mentions_uninitialized_recipient(&self, err: ServiceError) {
        let msg = err.to_string();
        assert!(msg.contains("recipient account is not initialized"));
        assert!(msg.contains("rent-exempt minimum"));
    }

    pub(crate) fn then_fee_check_passed(&self, pass: bool) {
        assert!(pass);
    }

    pub(crate) fn then_fee_check_failed(&self, pass: bool, message: &str) {
        assert!(!pass, "{message}");
    }

    pub(crate) fn then_one_row_was_affected(&self, affected: u64) {
        assert_eq!(affected, 1);
    }

    pub(crate) async fn then_collect_status_is_init(&self, collect: &ApiCollectEntity) {
        let persisted = self.load_collect(&collect.trade_no).await;
        assert_eq!(persisted.status, ApiCollectStatus::Init);
    }

    pub(crate) async fn then_fee_cycle_is_reopened(&self, collect: &ApiCollectEntity) {
        let persisted = self.load_collect(&collect.trade_no).await;
        assert_eq!(persisted.need_service_fee, Some(true));
        assert!(persisted.service_fee_uploaded_at.is_none());
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
