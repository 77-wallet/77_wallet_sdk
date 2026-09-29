mod support;

use serial_test::serial;
use support::{EvmDeployScenario, FakeEvmRpc};

#[tokio::test]
#[serial]
async fn wrong_polygon_rpc_stops_before_service_fee_and_keeps_account_unchanged() {
    // Given an unpaid Polygon multisig account and an RPC for Ethereum.
    let rpc = FakeEvmRpc::new(&[("eth_chainId", "0x1")]);
    let scenario = EvmDeployScenario::given("polygon", "Polygon", &rpc.url).await;

    // When deployment starts, the Safe preflight rejects the RPC.
    let error = scenario.when_deploy_without_payer().await.unwrap_err();

    // Then no service fee is attempted and the account's durable state is unchanged.
    assert!(error.contains("expected 137"), "{error}");
    scenario.then_account_unchanged().await;
    assert_eq!(rpc.finish(), ["eth_chainId"]);
}

#[tokio::test]
#[serial]
async fn valid_polygon_rpc_reaches_service_fee_payer_gate_without_mutating_account() {
    // Given an unpaid Polygon multisig account with valid Safe contracts.
    let rpc = FakeEvmRpc::new(&[
        ("eth_chainId", "0x89"),
        ("eth_getCode", "0x6001"),
        ("eth_getCode", "0x6002"),
    ]);
    let scenario = EvmDeployScenario::given("polygon", "Polygon", &rpc.url).await;

    // When deployment starts without a service fee payer.
    let error = scenario.when_deploy_without_payer().await.unwrap_err();

    // Then preflight completed before the payer validation, with no account writes.
    assert!(error.contains("PayerNeed"), "{error}");
    scenario.then_account_unchanged().await;
    assert_eq!(rpc.finish(), ["eth_chainId", "eth_getCode", "eth_getCode"]);
}

#[tokio::test]
#[serial]
async fn wrong_avalanche_rpc_stops_before_service_fee_and_keeps_account_unchanged() {
    // Given an unpaid Avalanche multisig account and an RPC for Ethereum.
    let rpc = FakeEvmRpc::new(&[("eth_chainId", "0x1")]);
    let scenario = EvmDeployScenario::given("avalanche", "Avalanche", &rpc.url).await;

    // When deployment starts, the Safe preflight rejects the RPC.
    let error = scenario.when_deploy_without_payer().await.unwrap_err();

    // Then no service fee is attempted and the account's durable state is unchanged.
    assert!(error.contains("expected 43114"), "{error}");
    scenario.then_account_unchanged().await;
    assert_eq!(rpc.finish(), ["eth_chainId"]);
}

#[tokio::test]
#[serial]
async fn valid_avalanche_rpc_reaches_service_fee_payer_gate_without_mutating_account() {
    // Given an unpaid Avalanche multisig account with valid Safe contracts.
    let rpc = FakeEvmRpc::new(&[
        ("eth_chainId", "0xa86a"),
        ("eth_getCode", "0x6001"),
        ("eth_getCode", "0x6002"),
    ]);
    let scenario = EvmDeployScenario::given("avalanche", "Avalanche", &rpc.url).await;

    // When deployment starts without a service fee payer.
    let error = scenario.when_deploy_without_payer().await.unwrap_err();

    // Then preflight completed before the payer validation, with no account writes.
    assert!(error.contains("PayerNeed"), "{error}");
    scenario.then_account_unchanged().await;
    assert_eq!(rpc.finish(), ["eth_chainId", "eth_getCode", "eth_getCode"]);
}
