use alloy::primitives::U256;
use serde_json::Value;

use crate::harness::{WorkerTestEnv, ensure_worker_env};

use super::super::super::support::{
    ServiceFeeUploadScenario, given_eth_service_fee_upload_waiting,
    given_sol_service_fee_upload_waiting, install_collect_eth_test_adapter,
    install_collect_test_adapter_fee_shortage, then_service_fee_upload_payload,
    when_upload_collect_service_fee,
};

pub(crate) struct CollectServiceFeeUploadScenario {
    env: &'static WorkerTestEnv,
}

impl CollectServiceFeeUploadScenario {
    pub(crate) async fn new() -> Self {
        let env = ensure_worker_env().await;
        env.recorder.reset();
        Self { env }
    }

    pub(crate) fn given_sol_fee_shortage_adapter(&self, recipient_missing: bool) -> impl Drop {
        install_collect_test_adapter_fee_shortage(recipient_missing, 0)
    }

    pub(crate) fn given_eth_fee_adapter(&self, balance_wei: u128, fee_amount: f64) -> impl Drop {
        install_collect_eth_test_adapter(U256::from(balance_wei), fee_amount)
    }

    pub(crate) async fn given_sol_service_fee_upload_waiting(
        &self,
        trade_prefix: &str,
    ) -> ServiceFeeUploadScenario {
        given_sol_service_fee_upload_waiting(self.env, trade_prefix).await
    }

    pub(crate) async fn given_eth_service_fee_upload_waiting(&self) -> ServiceFeeUploadScenario {
        given_eth_service_fee_upload_waiting(self.env).await
    }

    pub(crate) async fn when_service_fee_is_uploaded(
        &self,
        upload: &ServiceFeeUploadScenario,
        expect_msg: &str,
    ) {
        when_upload_collect_service_fee(upload, expect_msg).await;
    }

    pub(crate) fn then_payload_routes_reverse_transfer(&self, upload: &ServiceFeeUploadScenario) {
        let payload = self.payload(upload);
        assert_eq!(payload["tradeNo"].as_str(), Some(upload.trade_no.as_str()));
        assert_eq!(payload["from"].as_str(), Some(upload.to_addr.as_str()));
        assert_eq!(payload["to"].as_str(), Some(upload.from_addr.as_str()));
        assert_eq!(payload["tokenCode"].as_str(), Some(upload.token_code));
        assert_eq!(payload["contractAddress"].as_str(), Some(upload.contract_address));
    }

    pub(crate) fn then_payload_trade_no_matches(&self, upload: &ServiceFeeUploadScenario) {
        let payload = self.payload(upload);
        assert_eq!(payload["tradeNo"].as_str(), Some(upload.trade_no.as_str()));
    }

    pub(crate) fn then_payload_amount_is(
        &self,
        upload: &ServiceFeeUploadScenario,
        expected: f64,
        reason: &str,
    ) {
        let payload = self.payload(upload);
        let amount = payload["amount"].as_f64().unwrap_or_default();
        assert!(
            (amount - expected).abs() < 1e-12,
            "service fee upload must use {reason}, got {amount}"
        );
    }

    fn payload(&self, upload: &ServiceFeeUploadScenario) -> Value {
        then_service_fee_upload_payload(self.env, &upload.trade_no)
    }
}
