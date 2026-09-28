use super::*;
use alloy::{
    network::TransactionBuilder,
    primitives::{Address, U256},
    rpc::types::TransactionRequest,
};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};
use wallet_chain_interact::{eth::operations::TransferOpt, types::Transaction as _};

fn start_rpc(result: Value) -> (String, thread::JoinHandle<Value>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let worker = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut bytes = Vec::new();
        let mut buf = [0_u8; 4096];
        let (body_start, length) = loop {
            let count = socket.read(&mut buf).unwrap();
            assert!(count > 0);
            bytes.extend_from_slice(&buf[..count]);
            if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                let headers = std::str::from_utf8(&bytes[..end]).unwrap();
                let length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap();
                break (end + 4, length);
            }
        };
        while bytes.len() < body_start + length {
            let count = socket.read(&mut buf).unwrap();
            assert!(count > 0);
            bytes.extend_from_slice(&buf[..count]);
        }
        let request: Value =
            serde_json::from_slice(&bytes[body_start..body_start + length]).unwrap();
        let response = if result.get("error").is_some() {
            json!({"jsonrpc":"2.0","id":request["id"],"error":result["error"]})
        } else {
            json!({"jsonrpc":"2.0","id":request["id"],"result":result})
        }
        .to_string();
        write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.len(), response).unwrap();
        request
    });
    (url, worker)
}

#[tokio::test]
async fn scroll_l1_fee_rpc_uses_transfer_payload_and_parses_wei() {
    let (url, worker) = start_rpc(json!("0x38d7ea4c68000"));
    let to: Address = "0x0000000000000000000000000000000000000002".parse().unwrap();
    let tx = TransactionRequest::default().to(to).with_value(U256::from(1_u64));

    let fee = estimate_l1_data_fee(&url, None, tx).await.unwrap();

    let request = worker.join().unwrap();
    assert_eq!(fee, U256::from(1_000_000_000_000_000_u64));
    assert_eq!(request["method"], "scroll_estimateL1DataFee");
    assert_eq!(request["params"][0]["to"], "0x0000000000000000000000000000000000000002");
    assert_eq!(request["params"][0]["value"], "0x1");
    assert_eq!(request["params"][1], "latest");
}

#[tokio::test]
async fn scroll_l1_fee_rpc_rejects_invalid_fee_instead_of_zero() {
    let (url, worker) = start_rpc(json!("not-a-hex-fee"));
    let tx = TransactionRequest::default();

    let result = estimate_l1_data_fee(&url, None, tx).await;

    worker.join().unwrap();
    assert!(result.is_err());
}

#[tokio::test]
async fn scroll_l1_fee_rpc_rejects_json_rpc_error() {
    let (url, worker) = start_rpc(json!({"error":{"code":-32601,"message":"method not found"}}));
    let result = estimate_l1_data_fee(&url, None, TransactionRequest::default()).await;

    worker.join().unwrap();
    assert!(result.is_err());
}

#[tokio::test]
async fn scroll_l1_fee_rpc_rejects_zero_fee_for_nonempty_transfer() {
    let (url, worker) = start_rpc(json!("0x0"));
    let to: Address = "0x0000000000000000000000000000000000000002".parse().unwrap();
    let tx = TransactionRequest::default().to(to).with_value(U256::from(1_u64));

    let result = estimate_l1_data_fee(&url, None, tx).await;

    worker.join().unwrap();
    assert!(result.is_err());
}

#[tokio::test]
async fn scroll_l1_fee_rpc_includes_erc20_transfer_calldata() {
    let (url, worker) = start_rpc(json!("0x1"));
    let params = TransferOpt::new(
        "0x0000000000000000000000000000000000000001",
        "0x0000000000000000000000000000000000000002",
        U256::from(10_u64),
        Some("0x0000000000000000000000000000000000000003".to_string()),
    )
    .unwrap();

    let fee = estimate_l1_data_fee(&url, None, params.build_transaction().unwrap()).await.unwrap();

    let request = worker.join().unwrap();
    assert_eq!(fee, U256::from(1_u64));
    assert_eq!(request["params"][0]["to"], "0x0000000000000000000000000000000000000003");
    assert_eq!(request["params"][0]["value"], "0x0");
    let data = request["params"][0]["input"].as_str().unwrap();
    assert!(data.starts_with("0xa9059cbb"));
    assert!(data.ends_with("000000000000000000000000000000000000000000000000000000000000000a"));
}
