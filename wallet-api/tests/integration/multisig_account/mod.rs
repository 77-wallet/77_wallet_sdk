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

#[tokio::test]
#[serial]
async fn avalanche_address_derivation_checks_safe_before_factory_call() {
    // Given an Avalanche account and an RPC with both Safe contracts.
    let rpc = FakeEvmRpc::new(&[
        ("eth_chainId", "0xa86a"),
        ("eth_getCode", "0x6001"),
        ("eth_getCode", "0x6002"),
        ("eth_call", "0x0000000000000000000000002222222222222222222222222222222222222222"),
    ]);
    let scenario = EvmDeployScenario::given("avalanche", "Avalanche", &rpc.url).await;

    // When the adapter derives the address from the Safe factory.
    let (address, salt) = scenario.when_derive_address().await.unwrap();

    // Then the factory call follows preflight and the account remains unchanged.
    assert_eq!(address.to_lowercase(), "0x2222222222222222222222222222222222222222");
    assert!(!salt.is_empty());
    scenario.then_account_unchanged().await;
    assert_eq!(rpc.finish(), ["eth_chainId", "eth_getCode", "eth_getCode", "eth_call"]);
}

#[tokio::test]
#[serial]
async fn avalanche_address_derivation_stops_when_safe_singleton_is_missing() {
    // Given an Avalanche account with no Safe singleton code at the configured RPC.
    let rpc = FakeEvmRpc::new(&[
        ("eth_chainId", "0xa86a"),
        ("eth_getCode", "0x6001"),
        ("eth_getCode", "0x"),
    ]);
    let scenario = EvmDeployScenario::given("avalanche", "Avalanche", &rpc.url).await;

    // When address derivation begins, preflight rejects the missing contract.
    let error = scenario.when_derive_address().await.unwrap_err();

    // Then no factory call or account mutation occurs.
    assert!(error.contains("Safe singleton"), "{error}");
    scenario.then_account_unchanged().await;
    assert_eq!(rpc.finish(), ["eth_chainId", "eth_getCode", "eth_getCode"]);
}
