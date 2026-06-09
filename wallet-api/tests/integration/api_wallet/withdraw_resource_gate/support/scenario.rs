use wallet_api::testkit::withdraw::{
    scan_withdraw_intent_labels_for_trade_once,
    send_resource_result_ack_via_worker as send_withdraw_resource_result_ack_via_worker,
    upload_resource_tx_exec_receipt_via_worker as upload_withdraw_resource_tx_exec_receipt_via_worker,
};
use wallet_database::{
    ApiTransactionDbPool,
    entities::{
        api_resource_gate::ApiResourceGateResult, api_trade_type::ApiTradeType,
        api_withdraw::ApiWithdrawEntity,
    },
    repositories::api_wallet::withdraw::ApiWithdrawRepo,
};

use crate::harness::{
    AssertRole, GivenRole, LoadRole, SeedRole, ThenRole, WhenRole, ensure_worker_env,
    worker::WorkerTestEnv,
};

use super::{
    assertions::assert_event_ack_payload_exists,
    db::{
        mark_withdraw_blocked, open_transaction_pool, seed_failed_original_order_resource_result,
        seed_failed_resource_delegation, seed_resource_delegation_ready_for_ack,
        seed_successful_resource_delegation, seed_withdraw,
    },
    fixtures::WithdrawResourceGateFixture,
};

pub(crate) struct WithdrawResourceGateScenario {
    env: &'static WorkerTestEnv,
    tx_pool: ApiTransactionDbPool,
}

impl WithdrawResourceGateScenario {
    pub(crate) async fn new() -> Self {
        let env = ensure_worker_env().await;
        env.recorder.reset();

        let tx_pool = open_transaction_pool(env).await;

        Self { env, tx_pool }
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
pub(crate) trait WithdrawResourceGateGiven {
    async fn resource_delegation_ready_for_ack(&self, fixture: &WithdrawResourceGateFixture);

    async fn blocked_withdraw(&self, fixture: &WithdrawResourceGateFixture);

    async fn successful_withdraw_resource_delegation(&self, fixture: &WithdrawResourceGateFixture);

    async fn failed_withdraw_resource_delegation(&self, fixture: &WithdrawResourceGateFixture);

    async fn failed_original_order_resource_result(&self, fixture: &WithdrawResourceGateFixture);

    async fn resource_delegation_without_origin_trade(&self, fixture: &WithdrawResourceGateFixture);

    async fn collect_origin_resource_delegation(&self, fixture: &WithdrawResourceGateFixture);
}

#[async_trait::async_trait(?Send)]
impl WithdrawResourceGateGiven for GivenRole<'_, WithdrawResourceGateScenario> {
    async fn resource_delegation_ready_for_ack(&self, fixture: &WithdrawResourceGateFixture) {
        self.scenario().seed().resource_delegation_ready_for_ack(fixture).await;
    }

    async fn blocked_withdraw(&self, fixture: &WithdrawResourceGateFixture) {
        self.scenario().seed().blocked_withdraw(fixture).await;
    }

    async fn successful_withdraw_resource_delegation(&self, fixture: &WithdrawResourceGateFixture) {
        self.scenario().seed().successful_withdraw_resource_delegation(fixture).await;
    }

    async fn failed_withdraw_resource_delegation(&self, fixture: &WithdrawResourceGateFixture) {
        self.scenario().seed().failed_withdraw_resource_delegation(fixture).await;
    }

    async fn failed_original_order_resource_result(&self, fixture: &WithdrawResourceGateFixture) {
        self.scenario().seed().failed_original_order_resource_result(fixture).await;
    }

    async fn resource_delegation_without_origin_trade(
        &self,
        fixture: &WithdrawResourceGateFixture,
    ) {
        self.scenario().seed().resource_delegation_without_origin_trade(fixture).await;
    }

    async fn collect_origin_resource_delegation(&self, fixture: &WithdrawResourceGateFixture) {
        self.scenario().seed().collect_origin_resource_delegation(fixture).await;
    }
}

#[async_trait::async_trait(?Send)]
trait WithdrawResourceGateSeed {
    async fn resource_delegation_ready_for_ack(&self, fixture: &WithdrawResourceGateFixture);

    async fn blocked_withdraw(&self, fixture: &WithdrawResourceGateFixture);

    async fn successful_withdraw_resource_delegation(&self, fixture: &WithdrawResourceGateFixture);

    async fn failed_withdraw_resource_delegation(&self, fixture: &WithdrawResourceGateFixture);

    async fn failed_original_order_resource_result(&self, fixture: &WithdrawResourceGateFixture);

    async fn resource_delegation_without_origin_trade(&self, fixture: &WithdrawResourceGateFixture);

    async fn collect_origin_resource_delegation(&self, fixture: &WithdrawResourceGateFixture);
}

#[async_trait::async_trait(?Send)]
impl WithdrawResourceGateSeed for SeedRole<'_, WithdrawResourceGateScenario> {
    async fn resource_delegation_ready_for_ack(&self, fixture: &WithdrawResourceGateFixture) {
        seed_resource_delegation_ready_for_ack(
            &self.scenario().tx_pool,
            &fixture.trade_no,
            &fixture.resource_trade_no,
        )
        .await;
    }

    async fn blocked_withdraw(&self, fixture: &WithdrawResourceGateFixture) {
        seed_withdraw(&self.scenario().tx_pool, &fixture.trade_no).await;
        mark_withdraw_blocked(
            &self.scenario().tx_pool,
            &fixture.trade_no,
            &fixture.resource_trade_no,
        )
        .await;
    }

    async fn successful_withdraw_resource_delegation(&self, fixture: &WithdrawResourceGateFixture) {
        seed_successful_resource_delegation(
            &self.scenario().tx_pool,
            Some((&fixture.trade_no, ApiTradeType::Withdraw)),
            &fixture.resource_trade_no,
            "tx_hash_withdraw_release",
        )
        .await;
    }

    async fn failed_withdraw_resource_delegation(&self, fixture: &WithdrawResourceGateFixture) {
        seed_failed_resource_delegation(
            &self.scenario().tx_pool,
            &fixture.trade_no,
            ApiTradeType::Withdraw,
            &fixture.resource_trade_no,
        )
        .await;
    }

    async fn failed_original_order_resource_result(&self, fixture: &WithdrawResourceGateFixture) {
        seed_failed_original_order_resource_result(&self.scenario().tx_pool, &fixture.trade_no)
            .await;
    }

    async fn resource_delegation_without_origin_trade(
        &self,
        fixture: &WithdrawResourceGateFixture,
    ) {
        seed_successful_resource_delegation(
            &self.scenario().tx_pool,
            None,
            &fixture.resource_trade_no,
            "tx_hash_withdraw_no_origin",
        )
        .await;
    }

    async fn collect_origin_resource_delegation(&self, fixture: &WithdrawResourceGateFixture) {
        seed_successful_resource_delegation(
            &self.scenario().tx_pool,
            Some((&fixture.trade_no, ApiTradeType::Collect)),
            &fixture.resource_trade_no,
            "tx_hash_withdraw_wrong_origin",
        )
        .await;
    }
}

#[async_trait::async_trait(?Send)]
pub(crate) trait WithdrawResourceGateWhen {
    async fn resource_result_ack_is_sent(&self, fixture: &WithdrawResourceGateFixture);

    async fn resource_receipt_upload_is_sent(&self, fixture: &WithdrawResourceGateFixture);
}

#[async_trait::async_trait(?Send)]
impl WithdrawResourceGateWhen for WhenRole<'_, WithdrawResourceGateScenario> {
    async fn resource_result_ack_is_sent(&self, fixture: &WithdrawResourceGateFixture) {
        send_withdraw_resource_result_ack_via_worker(
            self.scenario().env.ctx(),
            &fixture.resource_trade_no,
        )
        .await
        .expect("send withdraw resource result ack");
    }

    async fn resource_receipt_upload_is_sent(&self, fixture: &WithdrawResourceGateFixture) {
        upload_withdraw_resource_tx_exec_receipt_via_worker(
            self.scenario().env.ctx(),
            &fixture.resource_trade_no,
        )
        .await
        .expect("upload withdraw resource tx exec receipt");
    }
}

#[async_trait::async_trait(?Send)]
pub(crate) trait WithdrawResourceGateThen {
    async fn resource_result_ack_uses_withdraw_resource_type(
        &self,
        fixture: &WithdrawResourceGateFixture,
    );

    async fn origin_withdraw_gate_is_released_by_successful_delegation(
        &self,
        fixture: &WithdrawResourceGateFixture,
    );

    async fn origin_withdraw_gate_is_released_by_failed_bypass(
        &self,
        fixture: &WithdrawResourceGateFixture,
    );

    async fn origin_withdraw_gate_is_not_released(&self, fixture: &WithdrawResourceGateFixture);

    async fn withdraw_can_build(&self, fixture: &WithdrawResourceGateFixture);

    async fn withdraw_cannot_build(&self, fixture: &WithdrawResourceGateFixture);
}

#[async_trait::async_trait(?Send)]
impl WithdrawResourceGateThen for ThenRole<'_, WithdrawResourceGateScenario> {
    async fn resource_result_ack_uses_withdraw_resource_type(
        &self,
        fixture: &WithdrawResourceGateFixture,
    ) {
        assert_event_ack_payload_exists(
            &self.scenario().env.recorder,
            &fixture.resource_trade_no,
            "TX_RES",
            "WD_RSC_DL",
        )
        .await;
    }

    async fn origin_withdraw_gate_is_released_by_successful_delegation(
        &self,
        fixture: &WithdrawResourceGateFixture,
    ) {
        self.origin_withdraw_gate_is_released(
            fixture,
            ApiResourceGateResult::ResourceDelegationSuccess,
        )
        .await;
    }

    async fn origin_withdraw_gate_is_released_by_failed_bypass(
        &self,
        fixture: &WithdrawResourceGateFixture,
    ) {
        self.origin_withdraw_gate_is_released(
            fixture,
            ApiResourceGateResult::ResourceDelegationFailedBypass,
        )
        .await;
    }

    async fn origin_withdraw_gate_is_not_released(&self, fixture: &WithdrawResourceGateFixture) {
        let withdraw = self.scenario().load().withdraw(&fixture.trade_no).await;
        self.scenario().assert().origin_withdraw_gate_is_not_released(&withdraw);
    }

    async fn withdraw_can_build(&self, fixture: &WithdrawResourceGateFixture) {
        let labels = self.scenario().load().withdraw_intent_labels(&fixture.trade_no).await;
        self.scenario().assert().withdraw_can_build(&labels);
    }

    async fn withdraw_cannot_build(&self, fixture: &WithdrawResourceGateFixture) {
        let labels = self.scenario().load().withdraw_intent_labels(&fixture.trade_no).await;
        self.scenario().assert().withdraw_cannot_build(&labels);
    }
}

impl ThenRole<'_, WithdrawResourceGateScenario> {
    async fn origin_withdraw_gate_is_released(
        &self,
        fixture: &WithdrawResourceGateFixture,
        expected_result: ApiResourceGateResult,
    ) {
        let withdraw = self.scenario().load().withdraw(&fixture.trade_no).await;
        self.scenario().assert().origin_withdraw_gate_is_released(&withdraw, expected_result);
    }
}

#[async_trait::async_trait(?Send)]
trait WithdrawResourceGateLoad {
    async fn withdraw(&self, trade_no: &str) -> ApiWithdrawEntity;

    async fn withdraw_intent_labels(&self, trade_no: &str) -> Vec<String>;
}

#[async_trait::async_trait(?Send)]
impl WithdrawResourceGateLoad for LoadRole<'_, WithdrawResourceGateScenario> {
    async fn withdraw(&self, trade_no: &str) -> ApiWithdrawEntity {
        ApiWithdrawRepo::get_api_withdraw_by_trade_no(
            &self.scenario().tx_pool,
            trade_no,
            ApiTradeType::Withdraw,
        )
        .await
        .expect("load withdraw")
    }

    async fn withdraw_intent_labels(&self, trade_no: &str) -> Vec<String> {
        scan_withdraw_intent_labels_for_trade_once(self.scenario().env.ctx(), trade_no)
            .await
            .expect("scan withdraw labels")
    }
}

trait WithdrawResourceGateAssert {
    fn origin_withdraw_gate_is_released(
        &self,
        withdraw: &ApiWithdrawEntity,
        expected_result: ApiResourceGateResult,
    );

    fn origin_withdraw_gate_is_not_released(&self, withdraw: &ApiWithdrawEntity);

    fn withdraw_can_build(&self, labels: &[String]);

    fn withdraw_cannot_build(&self, labels: &[String]);
}

impl WithdrawResourceGateAssert for AssertRole<'_, WithdrawResourceGateScenario> {
    fn origin_withdraw_gate_is_released(
        &self,
        withdraw: &ApiWithdrawEntity,
        expected_result: ApiResourceGateResult,
    ) {
        assert!(withdraw.resource_gate_released_at.is_some());
        assert_eq!(withdraw.resource_gate_result, Some(expected_result));
    }

    fn origin_withdraw_gate_is_not_released(&self, withdraw: &ApiWithdrawEntity) {
        assert!(withdraw.resource_gate_released_at.is_none());
        assert!(withdraw.resource_gate_result.is_none());
    }

    fn withdraw_can_build(&self, labels: &[String]) {
        assert!(
            labels.iter().any(|label| label == "BuildTx"),
            "released withdraw should re-enter BuildTx"
        );
    }

    fn withdraw_cannot_build(&self, labels: &[String]) {
        assert!(
            labels.iter().all(|label| label != "BuildTx"),
            "blocked withdraw should not be eligible for BuildTx before failed delegation bypass"
        );
    }
}
