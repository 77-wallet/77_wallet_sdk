use std::time::{Duration, Instant};

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

use crate::harness::{
    decrypt_captured_api_backend_body, ensure_worker_env, open_api_wallet_pool,
    worker::WorkerTestEnv,
};

use super::{
    db::{
        insert_failed_resource_delegation, insert_resource_delegation_ready_for_ack,
        insert_successful_resource_delegation, insert_withdraw, mark_withdraw_blocked,
        open_transaction_pool,
    },
    fixtures::WithdrawResourceGateFixture,
};

pub(crate) struct WithdrawResourceGateScenario {
    env: &'static WorkerTestEnv,
    tx_pool: ApiTransactionDbPool,
    core_pool: ApiWalletDbPool,
}

impl WithdrawResourceGateScenario {
    pub(crate) async fn new() -> Self {
        let env = ensure_worker_env().await;
        env.recorder.reset();

        let tx_pool = open_transaction_pool(env).await;
        let core_pool = open_api_wallet_pool(&env.db_dir).await;

        Self { env, tx_pool, core_pool }
    }

    pub(crate) async fn given_resource_delegation_ready_for_ack(
        &self,
        fixture: &WithdrawResourceGateFixture,
    ) {
        insert_resource_delegation_ready_for_ack(&self.tx_pool, &fixture.resource_trade_no).await;
    }

    pub(crate) async fn given_blocked_withdraw(&self, fixture: &WithdrawResourceGateFixture) {
        insert_withdraw(&self.tx_pool, &fixture.trade_no).await;
        mark_withdraw_blocked(&self.tx_pool, &fixture.trade_no, &fixture.resource_trade_no).await;
    }

    pub(crate) async fn given_successful_withdraw_resource_delegation(
        &self,
        fixture: &WithdrawResourceGateFixture,
    ) {
        insert_successful_resource_delegation(
            &self.tx_pool,
            Some((&fixture.trade_no, ApiTradeType::Withdraw)),
            &fixture.resource_trade_no,
            "tx_hash_withdraw_release",
        )
        .await;
    }

    pub(crate) async fn given_failed_withdraw_resource_delegation(
        &self,
        fixture: &WithdrawResourceGateFixture,
    ) {
        insert_failed_resource_delegation(
            &self.tx_pool,
            &fixture.trade_no,
            ApiTradeType::Withdraw,
            &fixture.resource_trade_no,
        )
        .await;
    }

    pub(crate) async fn given_resource_delegation_without_origin_trade(
        &self,
        fixture: &WithdrawResourceGateFixture,
    ) {
        insert_successful_resource_delegation(
            &self.tx_pool,
            None,
            &fixture.resource_trade_no,
            "tx_hash_withdraw_no_origin",
        )
        .await;
    }

    pub(crate) async fn given_collect_origin_resource_delegation(
        &self,
        fixture: &WithdrawResourceGateFixture,
    ) {
        insert_successful_resource_delegation(
            &self.tx_pool,
            Some((&fixture.trade_no, ApiTradeType::Collect)),
            &fixture.resource_trade_no,
            "tx_hash_withdraw_wrong_origin",
        )
        .await;
    }

    pub(crate) async fn when_resource_result_ack_is_sent(
        &self,
        fixture: &WithdrawResourceGateFixture,
    ) {
        send_withdraw_resource_result_ack_via_worker(
            self.tx_pool.clone(),
            self.core_pool.clone(),
            &fixture.resource_trade_no,
        )
        .await
        .expect("send withdraw resource result ack");
    }

    pub(crate) async fn when_resource_receipt_upload_is_sent(
        &self,
        fixture: &WithdrawResourceGateFixture,
    ) {
        upload_withdraw_resource_tx_exec_receipt_via_worker(
            self.tx_pool.clone(),
            self.core_pool.clone(),
            &fixture.resource_trade_no,
        )
        .await
        .expect("upload withdraw resource tx exec receipt");
    }

    pub(crate) async fn then_resource_result_ack_uses_withdraw_resource_type(
        &self,
        fixture: &WithdrawResourceGateFixture,
    ) {
        let matched = self
            .wait_for_resource_ack_payload(&fixture.resource_trade_no, "TX_RES", "WD_RSC_DL")
            .await;

        let captured_requests = self.env.recorder.snapshot();
        let decoded_event_acks: Vec<_> = captured_requests
            .iter()
            .filter(|req| {
                req.path.contains(
                    wallet_transport_backend::consts::endpoint::api_wallet::TRANS_EVENT_ACK,
                )
            })
            .map(|req| decrypt_captured_api_backend_body(&req.body))
            .collect();

        assert!(
            matched,
            "withdraw resource result ack must use WD_RSC_DL; decoded event ack payloads: {:?}; captured requests: {:?}",
            decoded_event_acks, captured_requests
        );
    }

    pub(crate) async fn then_origin_withdraw_gate_is_released(
        &self,
        fixture: &WithdrawResourceGateFixture,
        expected_result: ApiResourceGateResult,
    ) {
        let withdraw = self.load_withdraw(&fixture.trade_no).await;
        assert!(withdraw.resource_gate_released_at.is_some());
        assert_eq!(withdraw.resource_gate_result, Some(expected_result));
    }

    pub(crate) async fn then_origin_withdraw_gate_is_not_released(
        &self,
        fixture: &WithdrawResourceGateFixture,
    ) {
        let withdraw = self.load_withdraw(&fixture.trade_no).await;
        assert!(withdraw.resource_gate_released_at.is_none());
        assert!(withdraw.resource_gate_result.is_none());
    }

    pub(crate) async fn then_withdraw_can_build(&self, fixture: &WithdrawResourceGateFixture) {
        let labels = self.scan_withdraw_intent_labels(&fixture.trade_no).await;
        assert!(
            labels.iter().any(|label| label == "BuildTx"),
            "released withdraw should re-enter BuildTx"
        );
    }

    pub(crate) async fn then_withdraw_cannot_build(&self, fixture: &WithdrawResourceGateFixture) {
        let labels = self.scan_withdraw_intent_labels(&fixture.trade_no).await;
        assert!(
            labels.iter().all(|label| label != "BuildTx"),
            "blocked withdraw should not be eligible for BuildTx before failed delegation bypass"
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

    async fn scan_withdraw_intent_labels(&self, trade_no: &str) -> Vec<String> {
        scan_withdraw_intent_labels_for_trade_once(self.tx_pool.clone(), trade_no)
            .await
            .expect("scan withdraw labels")
    }

    async fn wait_for_resource_ack_payload(
        &self,
        resource_trade_no: &str,
        ack_type: &str,
        event_type: &str,
    ) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let found = self.env.recorder.snapshot().iter().any(|req| {
                req.path.contains(
                    wallet_transport_backend::consts::endpoint::api_wallet::TRANS_EVENT_ACK,
                ) && {
                    let payload = decrypt_captured_api_backend_body(&req.body);
                    payload["tradeNo"].as_str() == Some(resource_trade_no)
                        && payload["ackType"].as_str() == Some(ack_type)
                        && payload["type"].as_str() == Some(event_type)
                }
            });
            if found || Instant::now() >= deadline {
                return found;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}
