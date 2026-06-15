use wallet_api::testkit::collect::{
    send_resource_result_ack_via_worker, upload_resource_tx_exec_receipt_via_worker,
};
use wallet_database::{
    ApiTransactionDbPool,
    entities::{
        api_collect::ApiCollectEntity,
        api_resource_gate::{
            ApiResourceBlockReason, ApiResourceDependencyType, ApiResourceGateResult,
        },
        api_trade_type::ApiTradeType,
    },
    repositories::api_wallet::collect::ApiCollectRepo,
};

use crate::harness::{
    AssertRole, GivenRole, LoadRole, SeedRole, ThenRole, WhenRole, ensure_worker_env,
    worker::WorkerTestEnv,
};

use super::{
    db::{
        open_collect_pool, seed_blocked_collect, seed_failed_resource_receipt_row,
        seed_resource_result,
    },
    fixtures::CollectResourceGateFixture,
};

pub(crate) struct CollectResourceGateScenario {
    env: &'static WorkerTestEnv,
    collect_pool: ApiTransactionDbPool,
}

impl CollectResourceGateScenario {
    pub(crate) async fn new() -> Self {
        let env = ensure_worker_env().await;
        env.recorder.reset();

        let collect_pool = open_collect_pool(env).await;

        Self { env, collect_pool }
    }

    fn seed(&self) -> SeedRole<'_, Self> {
        SeedRole::new(self)
    }

    fn load(&self) -> LoadRole<'_, Self> {
        LoadRole::new(self)
    }

    fn assert(&self) -> AssertRole<'_, Self> {
        AssertRole::new(self)
    }
}

#[async_trait::async_trait(?Send)]
pub(crate) trait CollectResourceGateGiven {
    async fn blocked_collect(&self, fixture: &CollectResourceGateFixture);

    async fn successful_collect_resource_result(&self, fixture: &CollectResourceGateFixture);

    async fn failed_collect_resource_result(&self, fixture: &CollectResourceGateFixture);

    async fn successful_withdraw_origin_resource_result(
        &self,
        fixture: &CollectResourceGateFixture,
    );

    async fn failed_collect_resource_receipt(&self, fixture: &CollectResourceGateFixture);

    async fn failed_resource_receipt_without_origin_trade(
        &self,
        fixture: &CollectResourceGateFixture,
    );
}

#[async_trait::async_trait(?Send)]
impl CollectResourceGateGiven for GivenRole<'_, CollectResourceGateScenario> {
    async fn blocked_collect(&self, fixture: &CollectResourceGateFixture) {
        self.scenario().seed().blocked_collect(fixture).await;
    }

    async fn successful_collect_resource_result(&self, fixture: &CollectResourceGateFixture) {
        self.scenario().seed().collect_resource_result(fixture, true).await;
    }

    async fn failed_collect_resource_result(&self, fixture: &CollectResourceGateFixture) {
        self.scenario().seed().collect_resource_result(fixture, false).await;
    }

    async fn successful_withdraw_origin_resource_result(
        &self,
        fixture: &CollectResourceGateFixture,
    ) {
        self.scenario().seed().withdraw_origin_resource_result(fixture).await;
    }

    async fn failed_collect_resource_receipt(&self, fixture: &CollectResourceGateFixture) {
        self.scenario().seed().failed_collect_resource_receipt(fixture).await;
    }

    async fn failed_resource_receipt_without_origin_trade(
        &self,
        fixture: &CollectResourceGateFixture,
    ) {
        self.scenario().seed().failed_resource_receipt_without_origin_trade(fixture).await;
    }
}

#[async_trait::async_trait(?Send)]
trait CollectResourceGateSeed {
    async fn blocked_collect(&self, fixture: &CollectResourceGateFixture);

    async fn collect_resource_result(&self, fixture: &CollectResourceGateFixture, success: bool);

    async fn withdraw_origin_resource_result(&self, fixture: &CollectResourceGateFixture);

    async fn failed_collect_resource_receipt(&self, fixture: &CollectResourceGateFixture);

    async fn failed_resource_receipt_without_origin_trade(
        &self,
        fixture: &CollectResourceGateFixture,
    );
}

#[async_trait::async_trait(?Send)]
impl CollectResourceGateSeed for SeedRole<'_, CollectResourceGateScenario> {
    async fn blocked_collect(&self, fixture: &CollectResourceGateFixture) {
        seed_blocked_collect(&self.scenario().collect_pool, &fixture.trade_no).await;
    }

    async fn collect_resource_result(&self, fixture: &CollectResourceGateFixture, success: bool) {
        seed_resource_result(
            &self.scenario().collect_pool,
            &fixture.trade_no,
            &fixture.resource_trade_no,
            ApiTradeType::Collect,
            success,
        )
        .await;
    }

    async fn withdraw_origin_resource_result(&self, fixture: &CollectResourceGateFixture) {
        seed_resource_result(
            &self.scenario().collect_pool,
            "W_ORIGIN_SKIP",
            &fixture.resource_trade_no,
            ApiTradeType::Withdraw,
            true,
        )
        .await;
    }

    async fn failed_collect_resource_receipt(&self, fixture: &CollectResourceGateFixture) {
        seed_failed_resource_receipt_row(
            &self.scenario().collect_pool,
            Some(&fixture.trade_no),
            &fixture.resource_trade_no,
        )
        .await;
    }

    async fn failed_resource_receipt_without_origin_trade(
        &self,
        fixture: &CollectResourceGateFixture,
    ) {
        seed_failed_resource_receipt_row(
            &self.scenario().collect_pool,
            None,
            &fixture.resource_trade_no,
        )
        .await;
    }
}

#[async_trait::async_trait(?Send)]
pub(crate) trait CollectResourceGateWhen {
    async fn resource_result_ack_is_sent(&self, fixture: &CollectResourceGateFixture);

    async fn resource_receipt_upload_is_sent(&self, fixture: &CollectResourceGateFixture);
}

#[async_trait::async_trait(?Send)]
impl CollectResourceGateWhen for WhenRole<'_, CollectResourceGateScenario> {
    async fn resource_result_ack_is_sent(&self, fixture: &CollectResourceGateFixture) {
        send_resource_result_ack_via_worker(self.scenario().env.ctx(), &fixture.resource_trade_no)
            .await
            .expect("send resource result ack");
    }

    async fn resource_receipt_upload_is_sent(&self, fixture: &CollectResourceGateFixture) {
        upload_resource_tx_exec_receipt_via_worker(
            self.scenario().env.ctx(),
            &fixture.resource_trade_no,
        )
        .await
        .expect("upload resource tx exec receipt");
    }
}

#[async_trait::async_trait(?Send)]
pub(crate) trait CollectResourceGateThen {
    async fn origin_collect_gate_is_released_by_successful_delegation(
        &self,
        fixture: &CollectResourceGateFixture,
    );

    async fn origin_collect_gate_is_released_by_failed_bypass(
        &self,
        fixture: &CollectResourceGateFixture,
    );

    async fn origin_collect_gate_is_not_released(&self, fixture: &CollectResourceGateFixture);

    async fn collect_can_build(&self, fixture: &CollectResourceGateFixture);

    async fn collect_cannot_build(&self, fixture: &CollectResourceGateFixture);

    async fn collect_still_waits_for_platform_delegate(&self, fixture: &CollectResourceGateFixture);
}

#[async_trait::async_trait(?Send)]
impl CollectResourceGateThen for ThenRole<'_, CollectResourceGateScenario> {
    async fn origin_collect_gate_is_released_by_successful_delegation(
        &self,
        fixture: &CollectResourceGateFixture,
    ) {
        let collect = self.scenario().load().collect(&fixture.trade_no).await;
        self.scenario().assert().origin_collect_gate_is_released(
            &collect,
            ApiResourceGateResult::ResourceDelegationSuccess,
        );
    }

    async fn origin_collect_gate_is_released_by_failed_bypass(
        &self,
        fixture: &CollectResourceGateFixture,
    ) {
        let collect = self.scenario().load().collect(&fixture.trade_no).await;
        self.scenario().assert().origin_collect_gate_is_released(
            &collect,
            ApiResourceGateResult::ResourceDelegationFailedBypass,
        );
    }

    async fn origin_collect_gate_is_not_released(&self, fixture: &CollectResourceGateFixture) {
        let collect = self.scenario().load().collect(&fixture.trade_no).await;
        self.scenario().assert().origin_collect_gate_is_not_released(&collect);
    }

    async fn collect_can_build(&self, fixture: &CollectResourceGateFixture) {
        let is_candidate =
            self.scenario().load().is_collect_build_candidate(&fixture.trade_no).await;
        self.scenario().assert().collect_can_build(is_candidate);
    }

    async fn collect_cannot_build(&self, fixture: &CollectResourceGateFixture) {
        let is_candidate =
            self.scenario().load().is_collect_build_candidate(&fixture.trade_no).await;
        self.scenario().assert().collect_cannot_build(is_candidate);
    }

    async fn collect_still_waits_for_platform_delegate(
        &self,
        fixture: &CollectResourceGateFixture,
    ) {
        let collect = self.scenario().load().collect(&fixture.trade_no).await;
        self.scenario().assert().collect_still_waits_for_platform_delegate(&collect, fixture);
    }
}

#[async_trait::async_trait(?Send)]
trait CollectResourceGateLoad {
    async fn collect(&self, trade_no: &str) -> ApiCollectEntity;

    async fn is_collect_build_candidate(&self, trade_no: &str) -> bool;
}

#[async_trait::async_trait(?Send)]
impl CollectResourceGateLoad for LoadRole<'_, CollectResourceGateScenario> {
    async fn collect(&self, trade_no: &str) -> ApiCollectEntity {
        ApiCollectRepo::get_api_collect_by_trade_no(&self.scenario().collect_pool, trade_no)
            .await
            .expect("load collect")
    }

    async fn is_collect_build_candidate(&self, trade_no: &str) -> bool {
        ApiCollectRepo::scan_can_build(&self.scenario().collect_pool, 10_000)
            .await
            .expect("scan collect build candidates")
            .iter()
            .any(|collect| collect.trade_no == trade_no)
    }
}

trait CollectResourceGateAssert {
    fn origin_collect_gate_is_released(
        &self,
        collect: &ApiCollectEntity,
        expected_result: ApiResourceGateResult,
    );

    fn origin_collect_gate_is_not_released(&self, collect: &ApiCollectEntity);

    fn collect_can_build(&self, is_candidate: bool);

    fn collect_cannot_build(&self, is_candidate: bool);

    fn collect_still_waits_for_platform_delegate(
        &self,
        collect: &ApiCollectEntity,
        fixture: &CollectResourceGateFixture,
    );
}

impl CollectResourceGateAssert for AssertRole<'_, CollectResourceGateScenario> {
    fn origin_collect_gate_is_released(
        &self,
        collect: &ApiCollectEntity,
        expected_result: ApiResourceGateResult,
    ) {
        assert!(collect.resource_gate_released_at.is_some());
        assert_eq!(collect.resource_gate_result, Some(expected_result));
    }

    fn origin_collect_gate_is_not_released(&self, collect: &ApiCollectEntity) {
        assert!(collect.resource_gate_released_at.is_none());
        assert!(collect.resource_gate_result.is_none());
    }

    fn collect_can_build(&self, is_candidate: bool) {
        assert!(is_candidate, "released collect should be eligible for BuildTx");
    }

    fn collect_cannot_build(&self, is_candidate: bool) {
        assert!(
            !is_candidate,
            "blocked collect should not be eligible for BuildTx before local delegation fallback"
        );
    }

    fn collect_still_waits_for_platform_delegate(
        &self,
        collect: &ApiCollectEntity,
        fixture: &CollectResourceGateFixture,
    ) {
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
}
