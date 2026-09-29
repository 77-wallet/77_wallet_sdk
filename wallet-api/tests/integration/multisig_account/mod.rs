mod support;

use serial_test::serial;
use support::{EvmDeployScenario, FakeEvmRpc};

#[tokio::test]
#[serial]
async fn legacy_evm_safe_paid_retry_reuses_stored_address() {
    for (chain_code, chain_name) in [("eth", "Ethereum"), ("bnb", "BNB")] {
        // Given a paid Safe account whose previous attempt persisted an address and salt.
        let rpc = FakeEvmRpc::new(&[]);
        let scenario = EvmDeployScenario::given(chain_code, chain_name, &rpc.url).await;
        scenario.given_paid_failed_deployment_with_address().await;

        // When the retry reaches the unavailable initiator signer.
        let error = scenario.when_retry_deployment_without_signer().await;

        // Then no new address is derived and the original Safe identity remains.
        assert!(error.contains("Account not found"), "{chain_code}: {error}");
        scenario.then_stored_address_is_unchanged().await;
        assert!(rpc.finish().is_empty());
    }
}

#[tokio::test]
#[serial]
async fn new_evm_chains_paid_retry_reuses_stored_safe_address_before_signer_lookup() {
    for (chain_code, chain_name, chain_id) in [
        ("polygon", "Polygon", "0x89"),
        ("avalanche", "Avalanche", "0xa86a"),
        ("arbitrum", "Arbitrum", "0xa4b1"),
        ("base", "Base", "0x2105"),
        ("scroll", "Scroll", "0x82750"),
    ] {
        // Given a paid Safe account whose previous deployment saved its address.
        let rpc = FakeEvmRpc::new(&[
            ("eth_chainId", chain_id),
            ("eth_getCode", "0x6001"),
            ("eth_getCode", "0x6002"),
        ]);
        let scenario = EvmDeployScenario::given(chain_code, chain_name, &rpc.url).await;
        scenario.given_paid_failed_deployment_with_address().await;

        // When retrying without an available initiator signer.
        let error = scenario.when_retry_deployment_without_signer().await;

        // Then derivation is skipped and the stored Safe identity is retained.
        assert!(error.contains("Account not found"), "{chain_code}: {error}");
        scenario.then_stored_address_is_unchanged().await;
        assert_eq!(rpc.finish(), ["eth_chainId", "eth_getCode", "eth_getCode"]);
    }
}

fn polygon_deployment_receipt(success: bool) -> Vec<(&'static str, serde_json::Value)> {
    let hash = format!("0x{}", "11".repeat(32));
    let zero_hash = format!("0x{}", "00".repeat(32));
    let zero_bloom = format!("0x{}", "00".repeat(256));
    let receipt = serde_json::json!({
        "type": "0x0", "status": if success { "0x1" } else { "0x0" },
        "cumulativeGasUsed": "0x5208", "logsBloom": zero_bloom,
        "logs": [], "transactionHash": hash, "blockHash": hash,
        "blockNumber": "0x1", "transactionIndex": "0x0",
        "gasUsed": "0x5208", "effectiveGasPrice": "0x1",
        "from": "0x1111111111111111111111111111111111111111",
        "to": "0x2222222222222222222222222222222222222222",
        "contractAddress": null
    });
    let block = serde_json::json!({
        "hash": hash, "parentHash": zero_hash, "sha3Uncles": zero_hash,
        "miner": "0x0000000000000000000000000000000000000000",
        "stateRoot": zero_hash, "transactionsRoot": zero_hash,
        "receiptsRoot": zero_hash, "logsBloom": format!("0x{}", "00".repeat(256)),
        "difficulty": "0x0", "number": "0x1", "gasLimit": "0x100000",
        "gasUsed": "0x5208", "timestamp": "0x1", "extraData": "0x",
        "transactions": [], "uncles": []
    });
    vec![("eth_getTransactionReceipt", receipt), ("eth_getBlockByHash", block)]
}

#[tokio::test]
#[serial]
async fn polygon_successful_deployment_receipt_marks_account_on_chain() {
    let rpc = FakeEvmRpc::new_json(polygon_deployment_receipt(true));
    let scenario = EvmDeployScenario::given("polygon", "Polygon", &rpc.url).await;
    scenario.given_pending_deployment().await;
    assert_eq!(scenario.when_sync_deployment().await, 3);
    assert_eq!(rpc.finish(), ["eth_getTransactionReceipt", "eth_getBlockByHash"]);
}

#[tokio::test]
#[serial]
async fn polygon_failed_deployment_receipt_marks_account_failed() {
    let rpc = FakeEvmRpc::new_json(polygon_deployment_receipt(false));
    let scenario = EvmDeployScenario::given("polygon", "Polygon", &rpc.url).await;
    scenario.given_pending_deployment().await;
    assert_eq!(scenario.when_sync_deployment().await, 4);
    assert_eq!(rpc.finish(), ["eth_getTransactionReceipt", "eth_getBlockByHash"]);
}

#[tokio::test]
#[serial]
async fn polygon_pending_deployment_queries_safe_receipt_and_keeps_status_on_rpc_error() {
    // Given a pending Polygon Safe deployment and an unavailable receipt response.
    let rpc = FakeEvmRpc::new(&[("eth_getTransactionReceipt", "unavailable")]);
    let scenario = EvmDeployScenario::given("polygon", "Polygon", &rpc.url).await;
    scenario.given_pending_deployment().await;

    // When the account list synchronizes deployment status.
    let status = scenario.when_sync_deployment().await;

    // Then it queries through the Safe adapter and preserves the pending DB status.
    assert_eq!(status, 5);
    scenario.then_stop_syncing().await;
    assert_eq!(rpc.finish(), ["eth_getTransactionReceipt"]);
}

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

#[tokio::test]
#[serial]
async fn wrong_arbitrum_rpc_stops_before_service_fee_and_keeps_account_unchanged() {
    // Given an unpaid Arbitrum multisig account and an RPC for Ethereum.
    let rpc = FakeEvmRpc::new(&[("eth_chainId", "0x1")]);
    let scenario = EvmDeployScenario::given("arbitrum", "Arbitrum", &rpc.url).await;

    // When deployment starts, Safe preflight rejects the RPC.
    let error = scenario.when_deploy_without_payer().await.unwrap_err();

    // Then no service fee is attempted and durable account state is unchanged.
    assert!(error.contains("expected 42161"), "{error}");
    scenario.then_account_unchanged().await;
    assert_eq!(rpc.finish(), ["eth_chainId"]);
}

#[tokio::test]
#[serial]
async fn valid_arbitrum_rpc_reaches_service_fee_payer_gate_without_mutating_account() {
    // Given an unpaid Arbitrum multisig account and both Safe contracts.
    let rpc = FakeEvmRpc::new(&[
        ("eth_chainId", "0xa4b1"),
        ("eth_getCode", "0x6001"),
        ("eth_getCode", "0x6002"),
    ]);
    let scenario = EvmDeployScenario::given("arbitrum", "Arbitrum", &rpc.url).await;

    // When deployment starts without a service fee payer.
    let error = scenario.when_deploy_without_payer().await.unwrap_err();

    // Then preflight completed before payer validation and no account write occurred.
    assert!(error.contains("PayerNeed"), "{error}");
    scenario.then_account_unchanged().await;
    assert_eq!(rpc.finish(), ["eth_chainId", "eth_getCode", "eth_getCode"]);
}

#[tokio::test]
#[serial]
async fn wrong_base_rpc_stops_before_service_fee_and_keeps_account_unchanged() {
    // Given an unpaid Base multisig account and an RPC for Ethereum.
    let rpc = FakeEvmRpc::new(&[("eth_chainId", "0x1")]);
    let scenario = EvmDeployScenario::given("base", "Base", &rpc.url).await;

    // When deployment starts, Safe preflight rejects the RPC.
    let error = scenario.when_deploy_without_payer().await.unwrap_err();

    // Then no service fee is attempted and durable account state is unchanged.
    assert!(error.contains("expected 8453"), "{error}");
    scenario.then_account_unchanged().await;
    assert_eq!(rpc.finish(), ["eth_chainId"]);
}

#[tokio::test]
#[serial]
async fn valid_base_rpc_reaches_service_fee_payer_gate_without_mutating_account() {
    // Given an unpaid Base multisig account and both Safe contracts.
    let rpc = FakeEvmRpc::new(&[
        ("eth_chainId", "0x2105"),
        ("eth_getCode", "0x6001"),
        ("eth_getCode", "0x6002"),
    ]);
    let scenario = EvmDeployScenario::given("base", "Base", &rpc.url).await;

    // When deployment starts without a service fee payer.
    let error = scenario.when_deploy_without_payer().await.unwrap_err();

    // Then preflight completed before payer validation and no account write occurred.
    assert!(error.contains("PayerNeed"), "{error}");
    scenario.then_account_unchanged().await;
    assert_eq!(rpc.finish(), ["eth_chainId", "eth_getCode", "eth_getCode"]);
}

#[tokio::test]
#[serial]
async fn wrong_scroll_rpc_stops_before_service_fee_and_keeps_account_unchanged() {
    // Given an unpaid Scroll multisig account and an RPC for Ethereum.
    let rpc = FakeEvmRpc::new(&[("eth_chainId", "0x1")]);
    let scenario = EvmDeployScenario::given("scroll", "Scroll", &rpc.url).await;

    // When deployment starts, Safe preflight rejects the RPC.
    let error = scenario.when_deploy_without_payer().await.unwrap_err();

    // Then no service fee is attempted and durable account state is unchanged.
    assert!(error.contains("expected 534352"), "{error}");
    scenario.then_account_unchanged().await;
    assert_eq!(rpc.finish(), ["eth_chainId"]);
}

#[tokio::test]
#[serial]
async fn valid_scroll_rpc_reaches_service_fee_payer_gate_without_mutating_account() {
    // Given an unpaid Scroll multisig account and both Safe contracts.
    let rpc = FakeEvmRpc::new(&[
        ("eth_chainId", "0x82750"),
        ("eth_getCode", "0x6001"),
        ("eth_getCode", "0x6002"),
    ]);
    let scenario = EvmDeployScenario::given("scroll", "Scroll", &rpc.url).await;

    // When deployment starts without a service fee payer.
    let error = scenario.when_deploy_without_payer().await.unwrap_err();

    // Then preflight completed before payer validation and no account write occurred.
    assert!(error.contains("PayerNeed"), "{error}");
    scenario.then_account_unchanged().await;
    assert_eq!(rpc.finish(), ["eth_chainId", "eth_getCode", "eth_getCode"]);
}
