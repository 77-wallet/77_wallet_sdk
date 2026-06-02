use wallet_api::testkit::withdraw::{
    scan_withdraw_intent_labels_for_trade_once,
    send_resource_result_ack_via_worker as send_withdraw_resource_result_ack_via_worker,
    upload_resource_tx_exec_receipt_via_worker as upload_withdraw_resource_tx_exec_receipt_via_worker,
};
use wallet_database::{
    ApiTransactionDbPool, ApiWalletDbPool,
    entities::{
        api_resource_gate::ApiResourceGateResult, api_trade_type::ApiTradeType,
        api_withdraw::ApiWithdrawEntity,
    },
    repositories::api_wallet::withdraw::ApiWithdrawRepo,
};

use crate::harness::{ensure_worker_env, open_api_wallet_pool, worker::WorkerTestEnv};

use super::{
    assertions::assert_event_ack_payload_exists,
    db::{
        mark_withdraw_blocked, open_transaction_pool, seed_failed_resource_delegation,
        seed_resource_delegation_ready_for_ack, seed_successful_resource_delegation, seed_withdraw,
    },
    fixtures::WithdrawResourceGateFixture,
};

pub(crate) struct WithdrawResourceGateScenario {
    env: &'static WorkerTestEnv,
    tx_pool: ApiTransactionDbPool,
    core_pool: ApiWalletDbPool,
}

pub(crate) struct GivenRole<'a> {
    scenario: &'a WithdrawResourceGateScenario,
}

pub(crate) struct WhenRole<'a> {
    scenario: &'a WithdrawResourceGateScenario,
}

pub(crate) struct ThenRole<'a> {
    scenario: &'a WithdrawResourceGateScenario,
}

impl WithdrawResourceGateScenario {
    pub(crate) async fn new() -> Self {
        let env = ensure_worker_env().await;
        env.recorder.reset();

        let tx_pool = open_transaction_pool(env).await;
        let core_pool = open_api_wallet_pool(&env.db_dir).await;

        Self { env, tx_pool, core_pool }
    }

    pub(crate) fn given(&self) -> GivenRole<'_> {
        GivenRole { scenario: self }
    }

    pub(crate) fn when(&self) -> WhenRole<'_> {
        WhenRole { scenario: self }
    }

    pub(crate) fn then(&self) -> ThenRole<'_> {
        ThenRole { scenario: self }
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

    async fn scan_withdraw_intent_labels(&self, trade_no: &str) -> Vec<String> {
        scan_withdraw_intent_labels_for_trade_once(self.tx_pool.clone(), trade_no)
            .await
            .expect("scan withdraw labels")
    }
}

#[async_trait::async_trait(?Send)]
pub(crate) trait WithdrawResourceGateGiven {
    async fn resource_delegation_ready_for_ack(&self, fixture: &WithdrawResourceGateFixture);

    async fn blocked_withdraw(&self, fixture: &WithdrawResourceGateFixture);

    async fn successful_withdraw_resource_delegation(&self, fixture: &WithdrawResourceGateFixture);

    async fn failed_withdraw_resource_delegation(&self, fixture: &WithdrawResourceGateFixture);

    async fn resource_delegation_without_origin_trade(&self, fixture: &WithdrawResourceGateFixture);

    async fn collect_origin_resource_delegation(&self, fixture: &WithdrawResourceGateFixture);
}

#[async_trait::async_trait(?Send)]
impl WithdrawResourceGateGiven for GivenRole<'_> {
    async fn resource_delegation_ready_for_ack(&self, fixture: &WithdrawResourceGateFixture) {
        seed_resource_delegation_ready_for_ack(
            &self.scenario.tx_pool,
            &fixture.trade_no,
            &fixture.resource_trade_no,
        )
        .await;
    }

    async fn blocked_withdraw(&self, fixture: &WithdrawResourceGateFixture) {
        seed_withdraw(&self.scenario.tx_pool, &fixture.trade_no).await;
        mark_withdraw_blocked(
            &self.scenario.tx_pool,
            &fixture.trade_no,
            &fixture.resource_trade_no,
        )
        .await;
    }

    async fn successful_withdraw_resource_delegation(&self, fixture: &WithdrawResourceGateFixture) {
        seed_successful_resource_delegation(
            &self.scenario.tx_pool,
            Some((&fixture.trade_no, ApiTradeType::Withdraw)),
            &fixture.resource_trade_no,
            "tx_hash_withdraw_release",
        )
        .await;
    }

    async fn failed_withdraw_resource_delegation(&self, fixture: &WithdrawResourceGateFixture) {
        seed_failed_resource_delegation(
            &self.scenario.tx_pool,
            &fixture.trade_no,
            ApiTradeType::Withdraw,
            &fixture.resource_trade_no,
        )
        .await;
    }

    async fn resource_delegation_without_origin_trade(
        &self,
        fixture: &WithdrawResourceGateFixture,
    ) {
        seed_successful_resource_delegation(
            &self.scenario.tx_pool,
            None,
            &fixture.resource_trade_no,
            "tx_hash_withdraw_no_origin",
        )
        .await;
    }

    async fn collect_origin_resource_delegation(&self, fixture: &WithdrawResourceGateFixture) {
        seed_successful_resource_delegation(
            &self.scenario.tx_pool,
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
impl WithdrawResourceGateWhen for WhenRole<'_> {
    async fn resource_result_ack_is_sent(&self, fixture: &WithdrawResourceGateFixture) {
        send_withdraw_resource_result_ack_via_worker(
            self.scenario.tx_pool.clone(),
            self.scenario.core_pool.clone(),
            &fixture.resource_trade_no,
        )
        .await
        .expect("send withdraw resource result ack");
    }

    async fn resource_receipt_upload_is_sent(&self, fixture: &WithdrawResourceGateFixture) {
        upload_withdraw_resource_tx_exec_receipt_via_worker(
            self.scenario.tx_pool.clone(),
            self.scenario.core_pool.clone(),
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
impl WithdrawResourceGateThen for ThenRole<'_> {
    async fn resource_result_ack_uses_withdraw_resource_type(
        &self,
        fixture: &WithdrawResourceGateFixture,
    ) {
        assert_event_ack_payload_exists(
            &self.scenario.env.recorder,
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
        let withdraw = self.scenario.load_withdraw(&fixture.trade_no).await;
        assert!(withdraw.resource_gate_released_at.is_none());
        assert!(withdraw.resource_gate_result.is_none());
    }

    async fn withdraw_can_build(&self, fixture: &WithdrawResourceGateFixture) {
        let labels = self.scenario.scan_withdraw_intent_labels(&fixture.trade_no).await;
        assert!(
            labels.iter().any(|label| label == "BuildTx"),
            "released withdraw should re-enter BuildTx"
        );
    }

    async fn withdraw_cannot_build(&self, fixture: &WithdrawResourceGateFixture) {
        let labels = self.scenario.scan_withdraw_intent_labels(&fixture.trade_no).await;
        assert!(
            labels.iter().all(|label| label != "BuildTx"),
            "blocked withdraw should not be eligible for BuildTx before failed delegation bypass"
        );
    }
}

impl ThenRole<'_> {
    async fn origin_withdraw_gate_is_released(
        &self,
        fixture: &WithdrawResourceGateFixture,
        expected_result: ApiResourceGateResult,
    ) {
        let withdraw = self.scenario.load_withdraw(&fixture.trade_no).await;
        assert!(withdraw.resource_gate_released_at.is_some());
        assert_eq!(withdraw.resource_gate_result, Some(expected_result));
    }
}
