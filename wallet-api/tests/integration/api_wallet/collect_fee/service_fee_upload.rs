use crate::harness::ensure_worker_env;
use alloy::primitives::U256;
use serial_test::serial;

use super::support::{
    given_eth_service_fee_upload_waiting, given_sol_service_fee_upload_waiting,
    install_collect_eth_test_adapter, install_collect_test_adapter_fee_shortage,
    then_service_fee_upload_payload, when_upload_collect_service_fee,
};

#[serial]
#[tokio::test]
async fn collect_service_fee_upload_bypasses_local_sol_fee_gate() {
    let env = ensure_worker_env().await;
    env.recorder.reset();
    let _guard = install_collect_test_adapter_fee_shortage(false, 0);
    let scenario = given_sol_service_fee_upload_waiting(env, "T_collect_service_fee_upload").await;

    when_upload_collect_service_fee(
        &scenario,
        "service fee upload should bypass local balance gate",
    )
    .await;

    let payload = then_service_fee_upload_payload(env, &scenario.trade_no);
    assert_eq!(payload["tradeNo"].as_str(), Some(scenario.trade_no.as_str()));
    assert_eq!(payload["from"].as_str(), Some(scenario.to_addr.as_str()));
    assert_eq!(payload["to"].as_str(), Some(scenario.from_addr.as_str()));
    assert_eq!(payload["tokenCode"].as_str(), Some(scenario.token_code));
    assert_eq!(payload["contractAddress"].as_str(), Some(scenario.contract_address));
    assert!(
        (payload["amount"].as_f64().unwrap_or_default() - 0.00100588).abs() < 1e-12,
        "service fee upload must only carry the base Solana shortfall when the recipient ATA exists"
    );
}

#[serial]
#[tokio::test]
async fn collect_service_fee_upload_includes_solana_recipient_ata_rent_when_missing() {
    let env = ensure_worker_env().await;
    env.recorder.reset();
    let _guard = install_collect_test_adapter_fee_shortage(true, 0);
    let scenario =
        given_sol_service_fee_upload_waiting(env, "T_collect_service_fee_upload_ata").await;

    when_upload_collect_service_fee(
        &scenario,
        "service fee upload should include recipient ATA rent",
    )
    .await;

    let payload = then_service_fee_upload_payload(env, &scenario.trade_no);
    assert_eq!(payload["tradeNo"].as_str(), Some(scenario.trade_no.as_str()));
    let amount = payload["amount"].as_f64().unwrap_or_default();
    assert!(
        (amount - 0.00350588).abs() < 1e-12,
        "service fee upload must include the recipient ATA rent when the ATA is missing, got {amount}"
    );
}

#[serial]
#[tokio::test]
async fn collect_eth_service_fee_upload_uses_estimated_fee_without_multiplier() {
    let env = ensure_worker_env().await;
    let _guard =
        install_collect_eth_test_adapter(U256::from(100_000_000_000_000_000u128), 0.000015);
    let scenario = given_eth_service_fee_upload_waiting(env).await;

    when_upload_collect_service_fee(
        &scenario,
        "eth service fee upload should use the estimated fee",
    )
    .await;

    let payload = then_service_fee_upload_payload(env, &scenario.trade_no);
    assert_eq!(payload["tradeNo"].as_str(), Some(scenario.trade_no.as_str()));
    assert_eq!(payload["from"].as_str(), Some(scenario.to_addr.as_str()));
    assert_eq!(payload["to"].as_str(), Some(scenario.from_addr.as_str()));
    assert_eq!(payload["tokenCode"].as_str(), Some(scenario.token_code));
    assert_eq!(payload["contractAddress"].as_str(), Some(scenario.contract_address));
    let amount = payload["amount"].as_f64().unwrap_or_default();
    assert!(
        (amount - 0.000015).abs() < 1e-12,
        "service fee upload must use the estimated fee without multiplier, got {amount}"
    );
}
