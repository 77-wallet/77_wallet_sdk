use crate::get_manager;
use wallet_api::request::transaction;
// 余额测试
#[tokio::test]
async fn test_balance() {
    let wallet_manager = get_manager().await;

    let addr = "0x998522f928A37837Fa8d6743713170243b95f98a";
    let chain_code = "eth";
    let symbol = "USDT";
    let token_address = Some("0xdAC17F958D2ee523a2206206994597C13D831ec7".to_string());

    let balance = wallet_manager
        .chain_balance(addr, chain_code, &symbol, token_address)
        .await;

    println!("balance: {:?}", balance);
}

//交易的手续费
#[tokio::test]
async fn test_fee() {
    let wallet_manager = get_manager().await;

    let from = "0xdc4778f200c36a1C9dEeb3164cEE8366aD1F9455";
    let to = "0x998522f928A37837Fa8d6743713170243b95f98a";
    let value = "0.000001";
    let chain_code = "eth";
    let symbol = "ETH";
    // let symbol = "USDT";

    let params = transaction::BaseTransferReq::new(
        from.to_string(),
        to.to_string(),
        value.to_string(),
        chain_code.to_string(),
        symbol.to_string(),
    );

    let res = wallet_manager.transaction_fee(params).await;
    tracing::info!("token_fee: {}", serde_json::to_string(&res).unwrap());
}

// 转账
#[tokio::test]
async fn test_transfer() {
    let wallet_manager = get_manager().await;

    let from = "0xdc4778f200c36a1C9dEeb3164cEE8366aD1F9455";
    let to = "0x998522f928A37837Fa8d6743713170243b95f98a";
    let value = "0.000000908698916";
    let chain_code = "eth";
    let symbol = "ETH";
    // let symbol = "USDT";
    let password = "123456";
    // let notes = "test";

    let base = transaction::BaseTransferReq::new(
        from.to_string(),
        to.to_string(),
        value.to_string(),
        chain_code.to_string(),
        symbol.to_string(),
    );
    let params = transaction::TransferReq {
        base,
        password: password.to_string(),
        fee_setting:  r#"{"gasLimit":23100,"baseFee":"8030957221","priorityFee":"74260000","maxFeePerGas":"12100000000"}"#.to_string(),
        signer:None,
    };

    let token_fee = wallet_manager.transfer(params).await;
    tracing::info!(
        "test_transfer: {}",
        serde_json::to_string(&token_fee).unwrap()
    );
}
