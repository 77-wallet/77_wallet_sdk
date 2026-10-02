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
const TO: &str = "0x0000000000000000000000000000000000000002";
const TOKEN: &str = "0x0000000000000000000000000000000000000003";
const ADDITIONAL_WEI: u64 = 10_000_000_000_000;

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
        let request = TransferReq {
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
            .transfer(ctx, &request, ChainPrivateKey::from("unused-private-key"))
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

fn quoted_amount(value: &Value) -> f64 {
    value.as_str().unwrap().parse().unwrap()
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
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let worker = thread::spawn(move || {
        let mut requests = Vec::new();
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
            let fee = format!("0x{ADDITIONAL_WEI:x}");
            let result = match request["method"].as_str() {
                Some("eth_getBalance") => json!("0xde0b6b3a7640000"),
                Some("eth_call") if request["params"][0]["to"] == TOKEN => {
                    json!(format!("0x{:064x}", 1_000_000_000_000_000_000_u64))
                }
                Some("eth_call") => json!(format!("0x{ADDITIONAL_WEI:064x}")),
                Some("scroll_estimateL1DataFee") => json!(fee),
                Some("eth_estimateGas") => json!("0x5208"),
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
                && matches!(request["method"].as_str(), Some("scroll_estimateL1DataFee"))
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
        ("safe", 0.0000331, 0.00003772, "10000", "1200000000"),
        ("propose", 0.0000562, 0.00006544, "1000000000", "2400000000"),
        ("fast", 0.0000793, 0.00009316, "2000000000", "3600000000"),
    ]
    .iter()
    .enumerate()
    {
        let tier = &quote["data"][index];
        assert_eq!(tier["type"], *expected_type);
        assert!((quoted_amount(&tier["estimateFee"]["amount"]) - estimate).abs() < 1e-12);
        assert!((quoted_amount(&tier["maxFee"]["amount"]) - maximum).abs() < 1e-12);
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
            ("safe", 0.0000231, 0.00002772),
            ("propose", 0.0000462, 0.00005544),
            ("fast", 0.0000693, 0.00008316),
        ]) {
            assert_eq!(tier["type"], kind, "{code}");
            assert!(
                (quoted_amount(&tier["estimateFee"]["amount"]) - estimate).abs() < 1e-12,
                "{code} {kind}: expected {estimate}, got {}",
                quoted_amount(&tier["estimateFee"]["amount"]),
            );
            assert!((quoted_amount(&tier["maxFee"]["amount"]) - maximum).abs() < 1e-12);
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
