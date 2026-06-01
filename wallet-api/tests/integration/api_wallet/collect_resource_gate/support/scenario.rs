use wallet_api::testkit::collect::{
    send_resource_result_ack_via_worker, upload_resource_tx_exec_receipt_via_worker,
};
use wallet_database::{
    ApiTransactionDbPool, ApiWalletDbPool,
    entities::{
        api_collect::ApiCollectEntity,
        api_resource_gate::{
            ApiResourceBlockReason, ApiResourceDependencyType, ApiResourceGateResult,
        },
        api_trade_type::ApiTradeType,
    },
    repositories::api_wallet::collect::ApiCollectRepo,
};

use crate::harness::{ensure_worker_env, open_api_wallet_pool};

use super::{
    db::{
        open_collect_pool, seed_blocked_collect, seed_failed_resource_receipt_row,
        seed_resource_result,
    },
    fixtures::CollectResourceGateFixture,
};

pub(crate) struct CollectResourceGateScenario {
    collect_pool: ApiTransactionDbPool,
    core_pool: ApiWalletDbPool,
}

impl CollectResourceGateScenario {
    pub(crate) async fn new() -> Self {
        let env = ensure_worker_env().await;
        env.recorder.reset();

        let collect_pool = open_collect_pool(env).await;
        let core_pool = open_api_wallet_pool(&env.db_dir).await;

        Self { collect_pool, core_pool }
    }

    pub(crate) async fn given_blocked_collect(&self, fixture: &CollectResourceGateFixture) {
        seed_blocked_collect(&self.collect_pool, &fixture.trade_no).await;
    }

    pub(crate) async fn given_successful_collect_resource_result(
        &self,
        fixture: &CollectResourceGateFixture,
    ) {
        seed_resource_result(
            &self.collect_pool,
            &fixture.trade_no,
            &fixture.resource_trade_no,
            ApiTradeType::Collect,
            true,
        )
        .await;
    }

    pub(crate) async fn given_failed_collect_resource_result(
        &self,
        fixture: &CollectResourceGateFixture,
    ) {
        seed_resource_result(
            &self.collect_pool,
            &fixture.trade_no,
            &fixture.resource_trade_no,
            ApiTradeType::Collect,
            false,
        )
        .await;
    }

    pub(crate) async fn given_successful_withdraw_origin_resource_result(
        &self,
        fixture: &CollectResourceGateFixture,
    ) {
        seed_resource_result(
            &self.collect_pool,
            "W_ORIGIN_SKIP",
            &fixture.resource_trade_no,
            ApiTradeType::Withdraw,
            true,
        )
        .await;
    }

    pub(crate) async fn given_failed_collect_resource_receipt(
        &self,
        fixture: &CollectResourceGateFixture,
    ) {
        seed_failed_resource_receipt_row(
            &self.collect_pool,
            Some(&fixture.trade_no),
            &fixture.resource_trade_no,
        )
        .await;
    }

    pub(crate) async fn given_failed_resource_receipt_without_origin_trade(
        &self,
        fixture: &CollectResourceGateFixture,
    ) {
        seed_failed_resource_receipt_row(&self.collect_pool, None, &fixture.resource_trade_no)
            .await;
    }

    pub(crate) async fn when_resource_result_ack_is_sent(
        &self,
        fixture: &CollectResourceGateFixture,
    ) {
        send_resource_result_ack_via_worker(
            self.collect_pool.clone(),
            self.core_pool.clone(),
            &fixture.resource_trade_no,
        )
        .await
        .expect("send resource result ack");
    }

    pub(crate) async fn when_resource_receipt_upload_is_sent(
        &self,
        fixture: &CollectResourceGateFixture,
    ) {
        upload_resource_tx_exec_receipt_via_worker(
            self.collect_pool.clone(),
            self.core_pool.clone(),
            &fixture.resource_trade_no,
        )
        .await
        .expect("upload resource tx exec receipt");
    }

    pub(crate) async fn then_origin_collect_gate_is_released(
        &self,
        fixture: &CollectResourceGateFixture,
        expected_result: ApiResourceGateResult,
    ) {
        let collect = self.load_collect(&fixture.trade_no).await;
        assert!(collect.resource_gate_released_at.is_some());
        assert_eq!(collect.resource_gate_result, Some(expected_result));
    }

    pub(crate) async fn then_origin_collect_gate_is_not_released(
        &self,
        fixture: &CollectResourceGateFixture,
    ) {
        let collect = self.load_collect(&fixture.trade_no).await;
        assert!(collect.resource_gate_released_at.is_none());
        assert!(collect.resource_gate_result.is_none());
    }

    pub(crate) async fn then_collect_can_build(&self, fixture: &CollectResourceGateFixture) {
        assert!(
            self.is_collect_build_candidate(&fixture.trade_no).await,
            "released collect should be eligible for BuildTx"
        );
    }

    pub(crate) async fn then_collect_cannot_build(&self, fixture: &CollectResourceGateFixture) {
        assert!(
            !self.is_collect_build_candidate(&fixture.trade_no).await,
            "blocked collect should not be eligible for BuildTx before local delegation fallback"
        );
    }

    pub(crate) async fn then_collect_still_waits_for_platform_delegate(
        &self,
        fixture: &CollectResourceGateFixture,
    ) {
        let collect = self.load_collect(&fixture.trade_no).await;
        assert!(collect.resource_gate_released_at.is_none());
        assert_eq!(
            collect.resource_dependency_trade_no.as_deref(),
            Some(fixture.resource_trade_no.as_str())
        );
        assert_eq!(
            collect.resource_dependency_type,
            Some(ApiResourceDependencyType::PlatformDelegate)
        );
        assert_eq!(
            collect.resource_block_reason,
            Some(ApiResourceBlockReason::NeedPlatformDelegate)
        );
    }

    async fn load_collect(&self, trade_no: &str) -> ApiCollectEntity {
        ApiCollectRepo::get_api_collect_by_trade_no(&self.collect_pool, trade_no)
            .await
            .expect("load collect")
    }

    async fn is_collect_build_candidate(&self, trade_no: &str) -> bool {
        ApiCollectRepo::scan_can_build(&self.collect_pool, 10_000)
            .await
            .expect("scan collect build candidates")
            .iter()
            .any(|collect| collect.trade_no == trade_no)
    }
}
