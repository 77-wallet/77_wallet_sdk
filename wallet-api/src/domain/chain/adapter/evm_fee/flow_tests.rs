use super::{
    ensure_send_fee_balance, estimate_transfer_fee, execution_fee_reserve, required_send_fee,
};
use crate::{
    context::Context,
    dirs::Dirs,
    domain::chain::adapter::transaction_adapter::TransactionAdapter,
    request::transaction::{BaseTransferReq, TransferReq},
};
use alloy::primitives::U256;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::{Duration, Instant},
};
use wallet_chain_interact::{
    eth::{EthChain, FeeSetting, Provider, operations::TransferOpt},
    types::ChainPrivateKey,
};
use wallet_database::entities::asset_token_key::AssetTokenKey;
use wallet_transport::client::RpcClient;
use wallet_transport_backend::{api::BackendApi, response_vo::coin::TokenCurrency};
use wallet_types::chain::{chain::ChainCode, network::NetworkKind};

const FROM: &str = "0x0000000000000000000000000000000000000001";
// Address of the deterministic public test-only key used by signed-send fixtures.
const SIGNING_FROM: &str = "0x7e5f4552091a69125d5dfcb7b8c2659029395bdf";
const TO: &str = "0x0000000000000000000000000000000000000002";
const TOKEN: &str = "0x0000000000000000000000000000000000000003";
const ADDITIONAL_WEI: u64 = 10_000_000_000_000;

async fn maximum_test_context() -> (tempfile::TempDir, &'static Context) {
    let directory = tempfile::tempdir().unwrap();
    let config = crate::config::Config::new(
        r#"
app_code: "max_test"
crypto:
  aes_key: "1234567890abcdef"
  aes_iv: "abcdef1234567890"
backend_api:
  dev_url: "http://127.0.0.1:9"
  test_url: "http://127.0.0.1:9"
  prod_url: "http://127.0.0.1:9"
aggregate_api:
  dev_url: "http://127.0.0.1:9"
  test_url: "http://127.0.0.1:9"
  prod_url: "http://127.0.0.1:9"
"#,
    )
    .unwrap();
    let dirs = Dirs::new(directory.path().to_str().unwrap()).unwrap();
    let ctx =
        Box::leak(Box::new(Context::new_without_handles_for_test(dirs, config).await.unwrap()));
    (directory, ctx)
}

#[tokio::test]
async fn native_maximum_recalculates_value_before_broadcast() {
    let (_directory, ctx) = maximum_test_context().await;
    for code in [
        ChainCode::Ethereum,
        ChainCode::BnbSmartChain,
        ChainCode::Polygon,
        ChainCode::Avalanche,
        ChainCode::Arbitrum,
        ChainCode::Base,
        ChainCode::Scroll,
    ] {
        // Given a stale UI maximum which leaves only the execution reserve.
        let extra_calls = if matches!(code, ChainCode::Base | ChainCode::Scroll) { 2 } else { 0 };
        let (url, worker) = fake_services(5 + extra_calls, false);
        let (chain, _) = clients(&url, code);
        let adapter = TransactionAdapter::Ethereum(chain);
        let mut base =
            BaseTransferReq::new(SIGNING_FROM, TO, "0.9999685", &code.to_string(), "ETH");
        base.with_decimals(18);
        base.spend_all = true;
        let mut request = TransferReq {
            base,
            password: String::new(),
            fee_setting: json!({
                "gasLimit": 21000, "baseFee": "1000000000", "priorityFee": "100000000",
                "maxFeePerGas": "1500000000"
            })
            .to_string(),
            signer: None,
        };
        // When SDK submits to a local fake RPC using a public test-only key.
        let response =
            adapter.transfer(ctx, &mut request, ChainPrivateKey::from(format!("{:064x}", 1))).await;
        let calls = worker.join().unwrap();
        // Then the final request/bill amount matches the signed transaction amount.
        assert!(response.is_ok(), "{code}: {:?}", response.err());
        let reserve = if code == ChainCode::BnbSmartChain {
            23_100_000_000_000_u64
        } else {
            31_500_000_000_000
        };
        let additional = if extra_calls > 0 { ADDITIONAL_WEI } else { 0 };
        let expected = U256::from(1_000_000_000_000_000_000_u64 - reserve - additional);
        assert_eq!(wallet_utils::unit::convert_to_u256(&request.base.value, 18).unwrap(), expected);
        let bill: wallet_database::entities::bill::NewBillEntity =
            wallet_database::repositories::bill::BillRepo::try_build_bill_from(&request).unwrap();
        assert_eq!(bill.value, request.base.value.parse::<f64>().unwrap());
        let raw = &calls.last().unwrap().1["params"][0];
        let bytes = alloy::primitives::hex::decode(raw.as_str().unwrap()).unwrap();
        assert_eq!(bytes[0], 2, "EIP-1559 envelope");
        let mut payload = &bytes[1..];
        alloy::rlp::Header::decode(&mut payload).unwrap();
        for _ in 0..6 {
            let header = alloy::rlp::Header::decode(&mut payload).unwrap();
            payload = &payload[header.payload_length..];
        }
        let value = <U256 as alloy::rlp::Decodable>::decode(&mut payload).unwrap();
        assert_eq!(value, expected);
        assert_eq!(calls.last().unwrap().1["method"], "eth_sendRawTransaction");
    }
}

fn maximum_fee_setting() -> FeeSetting {
    FeeSetting {
        base_fee: U256::from(1_u64),
        gas_limit: U256::from(30_u64),
        max_fee_per_gas: U256::from(1_u64),
        max_priority_fee_per_gas: U256::ZERO,
    }
}

#[tokio::test]
async fn maximum_reserves_increasing_final_amount_fee_without_releasing_previous_reserve() {
    let (url, worker) = fake_services_with_fees(3, false, vec![10, 20, 5]);
    let (chain, _) = clients(&url, ChainCode::Scroll);
    let mut transfer = TransferOpt::new(FROM, TO, U256::from(999_u64), None).unwrap();
    let amount = super::prepare_native_maximum(
        &chain,
        &mut transfer,
        U256::from(100_u64),
        &mut maximum_fee_setting(),
        false,
    )
    .await
    .unwrap();
    assert_eq!(amount, U256::from(50_u64));
    assert_eq!(transfer.base.value, amount);
    assert_eq!(worker.join().unwrap().len(), 3);
}

#[tokio::test]
async fn maximum_unstable_fee_stops_after_four_estimates_without_broadcast() {
    let (url, worker) = fake_services_with_fees(4, false, vec![10, 20, 30, 40]);
    let (chain, _) = clients(&url, ChainCode::Scroll);
    let mut transfer = TransferOpt::new(FROM, TO, U256::from(1_u64), None).unwrap();
    let error = super::prepare_native_maximum(
        &chain,
        &mut transfer,
        U256::from(100_u64),
        &mut maximum_fee_setting(),
        false,
    )
    .await
    .unwrap_err();
    assert!(format!("{error:?}").contains("InsufficientFeeBalance"));
    let calls = worker.join().unwrap();
    assert_eq!(calls.len(), 4);
    assert!(calls.iter().all(|(_, body)| body["method"] == "scroll_estimateL1DataFee"));
}

#[tokio::test]
async fn maximum_cannot_spend_the_fee_reserve_or_ignore_rpc_failure() {
    for (balance, fail_fee, calls_expected) in [(30_u64, false, 0), (31, false, 1), (100, true, 1)]
    {
        let (url, worker) = fake_services(calls_expected, fail_fee);
        let (chain, _) = clients(&url, ChainCode::Scroll);
        let mut transfer = TransferOpt::new(FROM, TO, U256::from(1_u64), None).unwrap();
        let result = super::prepare_native_maximum(
            &chain,
            &mut transfer,
            U256::from(balance),
            &mut maximum_fee_setting(),
            false,
        )
        .await;
        assert!(result.is_err());
        let calls = worker.join().unwrap();
        assert_eq!(calls.len(), calls_expected);
        assert!(calls.iter().all(|(_, body)| body["method"] != "eth_sendRawTransaction"));
    }
}

#[tokio::test]
async fn native_maximum_rpc_failure_does_not_change_request_or_broadcast() {
    let (_directory, ctx) = maximum_test_context().await;
    for code in [ChainCode::Base, ChainCode::Scroll] {
        let (url, worker) = fake_services(3, true);
        let (chain, _) = clients(&url, code);
        let adapter = TransactionAdapter::Ethereum(chain);
        let mut base = BaseTransferReq::new(FROM, TO, "0.9999685", &code.to_string(), "ETH");
        base.with_decimals(18);
        base.spend_all = true;
        let mut request = TransferReq {
            base,
            password: String::new(),
            fee_setting: json!({"gasLimit": 21000, "baseFee": "1000000000",
                "priorityFee": "100000000", "maxFeePerGas": "1500000000"})
            .to_string(),
            signer: None,
        };
        let error = adapter
            .transfer(ctx, &mut request, ChainPrivateKey::from("unused-private-key"))
            .await
            .err()
            .expect("fee RPC must fail before signing");
        assert!(format!("{error:?}").contains("method not found"));
        assert_eq!(request.base.value, "0.9999685");
        let calls = worker.join().unwrap();
        assert_eq!(calls.len(), 3);
        assert!(calls.iter().all(|(_, body)| !matches!(
            body["method"].as_str(),
            Some("eth_sendRawTransaction" | "eth_getTransactionCount")
        )));
    }
}

#[tokio::test]
async fn testnet_maximum_freezes_refreshed_fee_before_calculation_and_signing() {
    let (_directory, ctx) = maximum_test_context().await;
    let (url, worker) = fake_services(7, false);
    let (mut chain, _) = clients(&url, ChainCode::Ethereum);
    chain.network = NetworkKind::Testnet;
    let adapter = TransactionAdapter::Ethereum(chain);
    let mut base = BaseTransferReq::new(SIGNING_FROM, TO, "0.9999685", "eth", "ETH");
    base.with_decimals(18);
    base.spend_all = true;
    let mut request = TransferReq {
        base,
        password: String::new(),
        fee_setting: json!({
            "gasLimit": 21000, "baseFee": "1000000000", "priorityFee": "100000000",
            "maxFeePerGas": "1500000000"
        })
        .to_string(),
        signer: None,
    };
    let response =
        adapter.transfer(ctx, &mut request, ChainPrivateKey::from(format!("{:064x}", 1))).await;
    let calls = worker.join().unwrap();
    assert!(response.is_ok(), "{:?}", response.err());
    // Core get_fee adds 10% gas headroom and 2 gwei priority to 1 gwei gas price.
    assert_eq!(request.base.value, "0.9999307");
    let estimate = calls.iter().find(|(_, body)| body["method"] == "eth_estimateGas").unwrap();
    assert_eq!(estimate.1["params"][0]["value"], "0x1");
    assert_eq!(calls.iter().filter(|(_, body)| body["method"] == "eth_estimateGas").count(), 1);
    assert_eq!(calls.last().unwrap().1["method"], "eth_sendRawTransaction");
    let raw = calls.last().unwrap().1["params"][0].as_str().unwrap();
    let bytes = alloy::primitives::hex::decode(raw).unwrap();
    assert_eq!(bytes[0], 2);
    let mut payload = &bytes[1..];
    alloy::rlp::Header::decode(&mut payload).unwrap();
    let fields: Vec<U256> =
        (0..5).map(|_| <U256 as alloy::rlp::Decodable>::decode(&mut payload).unwrap()).collect();
    assert_eq!(fields[2], U256::from(2_000_000_000_u64));
    assert_eq!(fields[3], U256::from(3_000_000_000_u64));
    assert_eq!(fields[4], U256::from(23_100_u64));
}

#[tokio::test]
async fn native_contract_maximum_reestimates_gas_for_final_amount() {
    let (_directory, ctx) = maximum_test_context().await;
    for network in [NetworkKind::Mainnet, NetworkKind::Testnet] {
        // Given a payable contract with a minimum deposit and value-dependent gas.
        let calls_expected = if network == NetworkKind::Mainnet { 8 } else { 9 };
        let (url, worker) = fake_services_with_gas(
            calls_expected,
            false,
            vec![ADDITIONAL_WEI],
            vec![30000, 40000, 20000],
            true,
        );
        let (mut chain, _) = clients(&url, ChainCode::Ethereum);
        chain.network = network;
        let adapter = TransactionAdapter::Ethereum(chain);
        let mut base = BaseTransferReq::new(SIGNING_FROM, TO, "0.9999685", "eth", "ETH");
        base.with_decimals(18);
        base.spend_all = true;
        let mut request = TransferReq {
            base,
            password: String::new(),
            fee_setting: json!({
                "gasLimit": 21000, "baseFee": "1000000000", "priorityFee": "100000000",
                "maxFeePerGas": "1500000000"
            })
            .to_string(),
            signer: None,
        };
        // When the final-value gas requirement rises then falls.
        let response =
            adapter.transfer(ctx, &mut request, ChainPrivateKey::from(format!("{:064x}", 1))).await;
        let calls = worker.join().unwrap();
        // Then keep the largest gas reserve and send the final candidate, never 1 wei.
        assert!(response.is_ok(), "{network:?}: {:?}", response.err());
        assert_eq!(
            request.base.value,
            if network == NetworkKind::Mainnet { "0.999934" } else { "0.999868" }
        );
        let raw = calls.last().unwrap().1["params"][0].as_str().unwrap();
        let bytes = alloy::primitives::hex::decode(raw).unwrap();
        let mut payload = &bytes[1..];
        alloy::rlp::Header::decode(&mut payload).unwrap();
        let fields: Vec<U256> = (0..5)
            .map(|_| <U256 as alloy::rlp::Decodable>::decode(&mut payload).unwrap())
            .collect();
        assert_eq!(fields[4], U256::from(44000_u64));
        let to = alloy::rlp::Header::decode(&mut payload).unwrap();
        payload = &payload[to.payload_length..];
        let value = <U256 as alloy::rlp::Decodable>::decode(&mut payload).unwrap();
        assert_eq!(value, wallet_utils::unit::convert_to_u256(&request.base.value, 18).unwrap());
        assert_eq!(calls.len(), calls_expected);
    }
}

#[tokio::test]
async fn native_contract_gas_rpc_failure_preserves_request_and_never_broadcasts() {
    let (_directory, ctx) = maximum_test_context().await;
    let (url, worker) = fake_services_with_gas(3, true, vec![ADDITIONAL_WEI], vec![30000], true);
    let (chain, _) = clients(&url, ChainCode::Ethereum);
    let adapter = TransactionAdapter::Ethereum(chain);
    let mut base = BaseTransferReq::new(SIGNING_FROM, TO, "0.9999685", "eth", "ETH");
    base.with_decimals(18);
    base.spend_all = true;
    let mut request = TransferReq {
        base,
        password: String::new(),
        fee_setting: json!({"gasLimit": 21000, "baseFee": "1000000000",
            "priorityFee": "100000000", "maxFeePerGas": "1500000000"})
        .to_string(),
        signer: None,
    };
    let error = adapter
        .transfer(ctx, &mut request, ChainPrivateKey::from("unused-private-key"))
        .await
        .err()
        .expect("gas RPC failure must abort before signing");
    assert!(format!("{error:?}").contains("method not found"));
    assert_eq!(request.base.value, "0.9999685");
    let calls = worker.join().unwrap();
    assert_eq!(calls.len(), 3);
    assert!(calls.iter().all(|(_, body)| !matches!(
        body["method"].as_str(),
        Some("eth_sendRawTransaction" | "eth_getTransactionCount")
    )));
}

#[tokio::test]
async fn maximum_quote_uses_latest_native_balance_but_preserves_manual_and_token_checks() {
    for (spend_all, token, expected_calls) in
        [(true, None, 4), (false, None, 1), (true, Some(TOKEN), 1)]
    {
        let (url, worker) = fake_services(expected_calls, false);
        let (chain, backend) = clients(&url, ChainCode::Scroll);
        let mut req = BaseTransferReq::new(FROM, TO, "2", "scroll", "ETH");
        req.with_decimals(18);
        req.spend_all = spend_all;
        if let Some(token) = token {
            req.with_token(AssetTokenKey::Contract(token.to_string()));
        }
        let price = TokenCurrency::new("scroll", "ETH", "ETH", Some(2000.0), None, 1.0, 18);
        let result = estimate_transfer_fee(&chain, &req, &backend, "USD", price).await;
        let calls = worker.join().unwrap();
        assert_eq!(calls.len(), expected_calls);
        if spend_all && token.is_none() {
            assert!(result.is_ok(), "{:?}", result.err());
            let gas = calls.iter().find(|(_, body)| body["method"] == "eth_estimateGas").unwrap();
            assert_eq!(gas.1["params"][0]["value"], "0xde0b6b3a7640000");
        } else {
            assert!(format!("{:?}", result.unwrap_err()).contains("InsufficientBalance"));
        }
    }
}

#[tokio::test]
async fn erc20_spend_all_keeps_requested_token_amount() {
    let (_directory, ctx) = maximum_test_context().await;
    let (url, worker) = fake_services(6, false);
    let (chain, _) = clients(&url, ChainCode::Base);
    let adapter = TransactionAdapter::Ethereum(chain);
    let mut base = BaseTransferReq::new(SIGNING_FROM, TO, "0.1", "base", "USDC");
    base.with_decimals(18);
    base.with_token(AssetTokenKey::Contract(TOKEN.to_string()));
    base.spend_all = true;
    let mut request = TransferReq {
        base,
        password: String::new(),
        fee_setting: json!({
            "gasLimit": 21000, "baseFee": "1000000000", "priorityFee": "100000000",
            "maxFeePerGas": "1500000000"
        })
        .to_string(),
        signer: None,
    };
    let response =
        adapter.transfer(ctx, &mut request, ChainPrivateKey::from(format!("{:064x}", 1))).await;
    let calls = worker.join().unwrap();
    assert!(response.is_ok(), "{:?}", response.err());
    assert_eq!(request.base.value, "0.1");
    assert_eq!(calls.len(), 6);
}

#[tokio::test]
async fn send_entry_rejects_base_and_scroll_when_only_l1_fee_is_missing() {
    let directory = tempfile::tempdir().unwrap();
    let config = crate::config::Config::new(
        r#"
app_code: "send_entry_test"
crypto:
  aes_key: "1234567890abcdef"
  aes_iv: "abcdef1234567890"
backend_api:
  dev_url: "http://127.0.0.1:9"
  test_url: "http://127.0.0.1:9"
  prod_url: "http://127.0.0.1:9"
aggregate_api:
  dev_url: "http://127.0.0.1:9"
  test_url: "http://127.0.0.1:9"
  prod_url: "http://127.0.0.1:9"
"#,
    )
    .unwrap();
    let dirs = Dirs::new(directory.path().to_str().unwrap()).unwrap();
    let ctx =
        Box::leak(Box::new(Context::new_without_handles_for_test(dirs, config).await.unwrap()));

    for code in [ChainCode::Base, ChainCode::Scroll] {
        let (url, worker) = fake_services(2, false);
        let (chain, _) = clients(&url, code);
        let adapter = TransactionAdapter::Ethereum(chain);
        let mut base = BaseTransferReq::new(FROM, TO, "0.999965", &code.to_string(), "ETH");
        base.with_decimals(18);
        let mut request = TransferReq {
            base,
            password: String::new(),
            fee_setting: json!({
                "gasLimit": 21000,
                "baseFee": "1000000000",
                "priorityFee": "100000000",
                "maxFeePerGas": "1500000000"
            })
            .to_string(),
            signer: None,
        };

        let error = match adapter
            .transfer(ctx, &mut request, ChainPrivateKey::from("unused-private-key"))
            .await
        {
            Ok(_) => panic!("{code}: L1 fee must prevent send before signing or broadcast"),
            Err(error) => error,
        };
        assert!(format!("{error:?}").contains("InsufficientFeeBalance"), "{code}: {error:?}");

        let requests = worker.join().unwrap();
        assert_eq!(requests.len(), 2, "{code}");
        assert_eq!(requests[0].1["method"], "eth_getBalance", "{code}");
        assert_eq!(
            requests[1].1["method"],
            if code == ChainCode::Base { "eth_call" } else { "scroll_estimateL1DataFee" },
            "{code}"
        );
    }
}

#[test]
fn send_preflight_reserves_submitted_eip1559_fee_cap() {
    let fee = FeeSetting {
        gas_limit: U256::from(21_000_u64),
        base_fee: U256::from(1_000_000_000_u64),
        max_priority_fee_per_gas: U256::from(100_000_000_u64),
        max_fee_per_gas: U256::from(1_500_000_000_u64),
    };
    let expected = fee.gas_limit * fee.max_fee_per_gas;
    assert!(expected > fee.transaction_fee());

    for code in [
        ChainCode::Ethereum,
        ChainCode::Polygon,
        ChainCode::Avalanche,
        ChainCode::Arbitrum,
        ChainCode::Base,
        ChainCode::Scroll,
    ] {
        let reserve = execution_fee_reserve(code, &fee);
        assert_eq!(reserve, expected, "{code}");
        let required = reserve + U256::from(ADDITIONAL_WEI);
        assert!(ensure_send_fee_balance(required, required).is_ok());
        assert!(ensure_send_fee_balance(required - U256::from(1_u64), required).is_err());
    }
    assert_eq!(execution_fee_reserve(ChainCode::BnbSmartChain, &fee), fee.transaction_fee());
}

#[test]
fn send_preflight_requires_the_full_fee_at_the_wei_boundary() {
    let required = U256::from(21_000_u64) + U256::from(ADDITIONAL_WEI);

    assert!(ensure_send_fee_balance(required, required).is_ok());
    let error = ensure_send_fee_balance(required - U256::from(1_u64), required).unwrap_err();
    assert!(format!("{error:?}").contains("InsufficientFeeBalance"));
}

#[tokio::test]
async fn send_preflight_reserves_additional_fee_for_base_and_scroll() {
    for code in [ChainCode::Base, ChainCode::Scroll] {
        let (url, worker) = fake_services(1, false);
        let (chain, _) = clients(&url, code);
        let transfer = TransferOpt::new(FROM, TO, U256::from(100_u64), None).unwrap();
        let required = required_send_fee(&chain, &transfer, U256::from(21_000_u64)).await.unwrap();
        assert_eq!(required, U256::from(21_000 + ADDITIONAL_WEI), "{code}");
        let requests = worker.join().unwrap();
        assert_eq!(requests.len(), 1, "{code}");
    }
}

#[tokio::test]
async fn send_preflight_uses_erc20_transfer_shape_for_base_and_scroll() {
    for code in [ChainCode::Base, ChainCode::Scroll] {
        let (url, worker) = fake_services(1, false);
        let (chain, _) = clients(&url, code);
        let transfer =
            TransferOpt::new(FROM, TO, U256::from(100_u64), Some(TOKEN.to_string())).unwrap();

        let required = required_send_fee(&chain, &transfer, U256::from(42_000_u64)).await.unwrap();

        assert_eq!(required, U256::from(42_000 + ADDITIONAL_WEI), "{code}");
        let requests = worker.join().unwrap();
        assert_eq!(requests.len(), 1, "{code}");
        match code {
            ChainCode::Base => {
                assert_eq!(requests[0].1["method"], "eth_call");
                let calldata = requests[0].1["params"][0]["data"].as_str().unwrap();
                let unsigned_size =
                    u64::from_str_radix(&calldata[calldata.len() - 64..], 16).unwrap();
                assert_eq!(unsigned_size, 324);
            }
            ChainCode::Scroll => {
                assert_eq!(requests[0].1["method"], "scroll_estimateL1DataFee");
                let tx = &requests[0].1["params"][0];
                assert_eq!(tx["to"], TOKEN);
                assert_eq!(tx["value"], "0x0");
                assert!(tx["input"].as_str().unwrap().starts_with("0xa9059cbb"));
            }
            _ => unreachable!(),
        }
    }
}

#[tokio::test]
async fn send_preflight_keeps_execution_fee_on_non_l2_chain() {
    let (url, worker) = fake_services(0, false);
    let (chain, _) = clients(&url, ChainCode::Polygon);
    let transfer = TransferOpt::new(FROM, TO, U256::from(100_u64), None).unwrap();
    let required = required_send_fee(&chain, &transfer, U256::from(21_000_u64)).await.unwrap();
    assert_eq!(required, U256::from(21_000_u64));
    assert!(worker.join().unwrap().is_empty());
}

#[tokio::test]
async fn send_preflight_aborts_when_scroll_additional_fee_rpc_fails() {
    let (url, worker) = fake_services(1, true);
    let (chain, _) = clients(&url, ChainCode::Scroll);
    let transfer = TransferOpt::new(FROM, TO, U256::from(100_u64), None).unwrap();

    let result = required_send_fee(&chain, &transfer, U256::from(21_000_u64)).await;

    assert!(format!("{:?}", result.unwrap_err()).contains("method not found"));
    let requests = worker.join().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].1["method"], "scroll_estimateL1DataFee");
}

fn quoted_amount(value: &Value) -> U256 {
    wallet_utils::unit::convert_to_u256(value.as_str().unwrap(), 18).unwrap()
}

fn read_request(socket: &mut std::net::TcpStream) -> (String, Value) {
    socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut bytes = Vec::new();
    let mut buf = [0_u8; 4096];
    let (headers_end, content_length) = loop {
        let size = socket.read(&mut buf).unwrap();
        assert!(size > 0, "HTTP request ended before headers");
        bytes.extend_from_slice(&buf[..size]);
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            let headers = std::str::from_utf8(&bytes[..end]).unwrap();
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap();
            break (end + 4, content_length);
        }
    };
    while bytes.len() < headers_end + content_length {
        let size = socket.read(&mut buf).unwrap();
        assert!(size > 0, "HTTP request ended before body");
        bytes.extend_from_slice(&buf[..size]);
    }
    let path = std::str::from_utf8(&bytes).unwrap().lines().next().unwrap().to_string();
    let body = serde_json::from_slice(&bytes[headers_end..headers_end + content_length]).unwrap();
    (path, body)
}

fn fake_services(
    expected_calls: usize,
    fail_additional_fee: bool,
) -> (String, thread::JoinHandle<Vec<(String, Value)>>) {
    fake_services_with_fees(expected_calls, fail_additional_fee, vec![ADDITIONAL_WEI])
}

fn fake_services_with_fees(
    expected_calls: usize,
    fail_additional_fee: bool,
    fees: Vec<u64>,
) -> (String, thread::JoinHandle<Vec<(String, Value)>>) {
    fake_services_with_gas(expected_calls, fail_additional_fee, fees, vec![21000], false)
}

fn fake_services_with_gas(
    expected_calls: usize,
    fail_additional_fee: bool,
    fees: Vec<u64>,
    gas_limits: Vec<u64>,
    is_contract: bool,
) -> (String, thread::JoinHandle<Vec<(String, Value)>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let worker = thread::spawn(move || {
        let mut requests = Vec::new();
        let mut fee_index = 0;
        let mut gas_index = 0;
        let deadline = Instant::now() + Duration::from_secs(10);
        while requests.len() < expected_calls && Instant::now() < deadline {
            let (mut socket, _) = match listener.accept() {
                Ok(connection) => connection,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(error) => panic!("fake server accept failed: {error}"),
            };
            socket.set_nonblocking(false).unwrap();
            let (path, request) = read_request(&mut socket);
            let additional = fees[fee_index.min(fees.len() - 1)];
            if matches!(request["method"].as_str(), Some("eth_call" | "scroll_estimateL1DataFee")) {
                fee_index += 1;
            }
            let fee = format!("0x{additional:x}");
            let result = match request["method"].as_str() {
                Some("eth_getBalance") => json!("0xde0b6b3a7640000"),
                Some("eth_call") if request["params"][0]["to"] == TOKEN => {
                    json!(format!("0x{:064x}", 1_000_000_000_000_000_000_u64))
                }
                Some("eth_call") => json!(format!("0x{additional:064x}")),
                Some("scroll_estimateL1DataFee") => json!(fee),
                Some("eth_gasPrice") => json!("0x3b9aca00"),
                Some("eth_getTransactionCount") => json!("0x0"),
                Some("eth_chainId") => json!("0x1"),
                Some("eth_sendRawTransaction") => json!(format!("0x{:064x}", 1)),
                Some("eth_getCode") => json!(if is_contract { "0x6000" } else { "0x" }),
                Some("eth_estimateGas") => {
                    if is_contract {
                        let value = request["params"][0]["value"].as_str().unwrap();
                        let amount = wallet_utils::unit::u256_from_str(value).unwrap();
                        assert!(
                            amount >= U256::from(500_000_000_000_000_000_u64),
                            "payable contract rejects a probe below its minimum deposit"
                        );
                    }
                    let gas = gas_limits[gas_index.min(gas_limits.len() - 1)];
                    gas_index += 1;
                    json!(format!("0x{gas:x}"))
                }
                None if path.contains("/token/findGasTracker") => json!({
                    "success": true,
                    "data": {
                        "safeGasPrice": "1",
                        "proposeGasPrice": "2",
                        "fastGasPrice": "3",
                        "suggestBaseFee": "1",
                        "gasUsedRatio": null
                    }
                }),
                other => panic!("unexpected request: {path} {other:?}"),
            };
            let response = if request["method"].is_null() {
                result
            } else if fail_additional_fee
                && (matches!(
                    request["method"].as_str(),
                    Some("eth_call" | "scroll_estimateL1DataFee")
                ) || (is_contract && request["method"] == "eth_estimateGas"))
            {
                json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":-32601,"message":"method not found"}})
            } else {
                json!({"jsonrpc":"2.0","id":request["id"],"result":result})
            };
            let body = response.to_string();
            write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
            requests.push((path, request));
        }
        requests
    });
    (url, worker)
}

fn clients(url: &str, code: ChainCode) -> (EthChain, BackendApi) {
    let provider =
        Provider::new(RpcClient::new(url, None, Some(Duration::from_secs(5))).unwrap()).unwrap();
    let chain = EthChain::new(provider, NetworkKind::Mainnet, code).unwrap();
    let cryptor = wallet_utils::cbc::AesCbcCryptor::new("0000000000000000", "0000000000000000");
    let backend = BackendApi::new(Some(url.to_string()), None, cryptor).unwrap();
    (chain, backend)
}

async fn assert_quote(code: ChainCode, token: Option<&str>) {
    let (url, worker) = fake_services(4, false);
    let (chain, backend) = clients(&url, code);
    let mut req = BaseTransferReq::new(FROM, TO, "0.1", &code.to_string(), "ETH");
    req.with_decimals(18);
    if let Some(address) = token {
        req.with_token(AssetTokenKey::Contract(address.to_string()));
    }
    let price = TokenCurrency::new(&code.to_string(), "ETH", "ETH", Some(2_000.0), None, 1.0, 18);

    let quote = estimate_transfer_fee(&chain, &req, &backend, "USD", price).await.unwrap();
    let quote: Value = serde_json::from_str(&quote).unwrap();
    let requests = worker.join().unwrap();

    assert_eq!(requests.len(), 4);
    assert!(requests[1].0.contains("/token/findGasTracker"));
    assert_eq!(requests[3].1["method"], "eth_estimateGas");
    assert_eq!(quote["default"], "propose");
    assert_eq!(quote["data"].as_array().unwrap().len(), 3);
    for (index, (expected_type, estimate, maximum, priority, max_per_gas)) in [
        ("safe", "0.000033100231", "0.00003772", "10000", "1200000000"),
        ("propose", "0.0000562", "0.00006544", "1000000000", "2400000000"),
        ("fast", "0.0000793", "0.00009316", "2000000000", "3600000000"),
    ]
    .iter()
    .enumerate()
    {
        let tier = &quote["data"][index];
        assert_eq!(tier["type"], *expected_type);
        assert_eq!(
            quoted_amount(&tier["estimateFee"]["amount"]),
            wallet_utils::unit::convert_to_u256(estimate, 18).unwrap()
        );
        assert_eq!(
            quoted_amount(&tier["maxFee"]["amount"]),
            wallet_utils::unit::convert_to_u256(maximum, 18).unwrap()
        );
        assert_eq!(tier["feeSetting"]["gasLimit"], 23_100);
        assert_eq!(tier["feeSetting"]["priorityFee"], *priority);
        assert_eq!(tier["feeSetting"]["maxFeePerGas"], *max_per_gas);
    }
    if token.is_some() {
        assert_eq!(requests[0].1["method"], "eth_call");
        assert_eq!(requests[3].1["params"][0]["to"], TOKEN);
        assert!(requests[3].1["params"][0]["input"].as_str().unwrap().starts_with("0xa9059cbb"));
    } else {
        assert_eq!(requests[0].1["method"], "eth_getBalance");
        assert_eq!(requests[3].1["params"][0]["to"], TO);
    }
    match code {
        ChainCode::Base => {
            assert_eq!(requests[2].1["method"], "eth_call");
            assert_eq!(
                requests[2].1["params"][0]["to"],
                "0x420000000000000000000000000000000000000f"
            );
            let data = requests[2].1["params"][0]["data"].as_str().unwrap();
            let unsigned_size = u64::from_str_radix(&data[data.len() - 64..], 16).unwrap();
            assert_eq!(unsigned_size, if token.is_some() { 324 } else { 256 });
        }
        ChainCode::Scroll => {
            assert_eq!(requests[2].1["method"], "scroll_estimateL1DataFee");
            assert_eq!(requests[2].1["params"][0]["to"], token.unwrap_or(TO));
            assert_eq!(requests[2].1["params"][1], "latest");
            if token.is_some() {
                assert!(
                    requests[2].1["params"][0]["input"].as_str().unwrap().starts_with("0xa9059cbb")
                );
            }
        }
        _ => unreachable!("this helper only covers Base and Scroll"),
    }
}

#[tokio::test]
async fn base_native_quote_uses_core_fee_without_changing_execution_settings() {
    assert_quote(ChainCode::Base, None).await;
}

#[tokio::test]
async fn base_erc20_quote_keeps_transfer_calldata_and_adds_core_fee() {
    assert_quote(ChainCode::Base, Some(TOKEN)).await;
}

#[tokio::test]
async fn non_l1_data_fee_chains_quote_three_tiers_without_extra_rpc() {
    for code in [ChainCode::Polygon, ChainCode::Avalanche, ChainCode::Arbitrum] {
        let (url, worker) = fake_services(3, false);
        let (chain, backend) = clients(&url, code);
        let mut req = BaseTransferReq::new(FROM, TO, "0.1", &code.to_string(), "ETH");
        req.with_decimals(18);
        let price =
            TokenCurrency::new(&code.to_string(), "ETH", "ETH", Some(2_000.0), None, 1.0, 18);

        let quote = estimate_transfer_fee(&chain, &req, &backend, "USD", price).await.unwrap();
        let quote: Value = serde_json::from_str(&quote).unwrap();
        let requests = worker.join().unwrap();

        assert_eq!(requests.len(), 3, "{code}");
        assert_eq!(requests[0].1["method"], "eth_getBalance", "{code}");
        assert!(requests[1].0.contains("/token/findGasTracker"), "{code}");
        assert_eq!(requests[2].1["method"], "eth_estimateGas", "{code}");
        assert_eq!(quote["default"], "propose", "{code}");
        let tiers = quote["data"].as_array().unwrap();
        assert_eq!(tiers.len(), 3, "{code}");
        for (tier, (kind, estimate, maximum)) in tiers.iter().zip([
            ("safe", "0.000023100231", "0.00002772"),
            ("propose", "0.0000462", "0.00005544"),
            ("fast", "0.0000693", "0.00008316"),
        ]) {
            assert_eq!(tier["type"], kind, "{code}");
            assert_eq!(
                quoted_amount(&tier["estimateFee"]["amount"]),
                wallet_utils::unit::convert_to_u256(estimate, 18).unwrap(),
                "{code} {kind}"
            );
            assert_eq!(
                quoted_amount(&tier["maxFee"]["amount"]),
                wallet_utils::unit::convert_to_u256(maximum, 18).unwrap()
            );
            assert_eq!(tier["feeSetting"]["gasLimit"], 23_100, "{code}");
        }
    }
}

#[tokio::test]
async fn scroll_erc20_quote_keeps_transfer_calldata_and_adds_core_fee() {
    assert_quote(ChainCode::Scroll, Some(TOKEN)).await;
}

#[tokio::test]
async fn scroll_native_quote_uses_core_fee_without_changing_execution_settings() {
    assert_quote(ChainCode::Scroll, None).await;
}

#[tokio::test]
async fn scroll_additional_fee_rpc_error_fails_sdk_quote() {
    let (url, worker) = fake_services(3, true);
    let (chain, backend) = clients(&url, ChainCode::Scroll);
    let mut req = BaseTransferReq::new(FROM, TO, "0.1", "scroll", "ETH");
    req.with_decimals(18);
    let price = TokenCurrency::new("scroll", "ETH", "ETH", Some(2_000.0), None, 1.0, 18);

    let result = estimate_transfer_fee(&chain, &req, &backend, "USD", price).await;
    let requests = worker.join().unwrap();

    let error = result.expect_err("Core additional-fee RPC failure must abort the quote");
    assert!(format!("{error:?}").contains("method not found"), "unexpected error: {error:?}");
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[2].1["method"], "scroll_estimateL1DataFee");
    assert!(requests.iter().all(|(_, body)| body["method"] != "eth_estimateGas"));
}
