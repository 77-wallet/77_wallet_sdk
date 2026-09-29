use super::estimate_transfer_fee;
use crate::request::transaction::BaseTransferReq;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::{Duration, Instant},
};
use wallet_chain_interact::eth::{EthChain, Provider};
use wallet_database::entities::asset_token_key::AssetTokenKey;
use wallet_transport::client::RpcClient;
use wallet_transport_backend::{api::BackendApi, response_vo::coin::TokenCurrency};
use wallet_types::chain::{chain::ChainCode, network::NetworkKind};

const FROM: &str = "0x0000000000000000000000000000000000000001";
const TO: &str = "0x0000000000000000000000000000000000000002";
const TOKEN: &str = "0x0000000000000000000000000000000000000003";
const ADDITIONAL_WEI: u64 = 10_000_000_000_000;

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
        assert_eq!(requests[2].1["method"], "scroll_estimateL1DataFee");
        assert_eq!(requests[2].1["params"][0]["to"], TOKEN);
        assert!(requests[2].1["params"][0]["input"].as_str().unwrap().starts_with("0xa9059cbb"));
    } else {
        assert_eq!(requests[0].1["method"], "eth_getBalance");
        assert_eq!(requests[2].1["method"], "eth_call");
        assert_eq!(requests[2].1["params"][0]["to"], "0x420000000000000000000000000000000000000f");
    }
}

#[tokio::test]
async fn base_native_quote_uses_core_fee_without_changing_execution_settings() {
    assert_quote(ChainCode::Base, None).await;
}

#[tokio::test]
async fn scroll_erc20_quote_keeps_transfer_calldata_and_adds_core_fee() {
    assert_quote(ChainCode::Scroll, Some(TOKEN)).await;
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
