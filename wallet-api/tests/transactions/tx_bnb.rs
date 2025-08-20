use crate::get_manager;
use wallet_api::request::transaction;

// 余额测试
#[tokio::test]
async fn test_balance() {
    let wallet_manager = get_manager().await;

    let addr = "0x0Bb5c82158760F1C1c73722e4aD51EE24f1559c3";
    // let addr = "0x5985CE40d3dACf7c1352e464691BC7fb03215928";
    let chain_code = "bnb";
    // let symbol = "STK";
    let symbol = "USDT";
    let token_address = None;

    let balance = wallet_manager
        .chain_balance(addr, chain_code, &symbol, token_address)
        .await;

    println!("balance: {:?}", balance);
}

//交易的手续费
#[tokio::test]
async fn test_fee() {
    let wallet_manager = get_manager().await;

    let from = "0x5d38C9d80A89f9A6464fC34E8bbCfEB2aD56dAc9";
    let to = "0xF7d5c082Ce49922913404b56168EBa82Dda4c1F7";
    let value = "0.0001";
    let chain_code = "bnb";
    let symbol = "BNB";

    let params = transaction::BaseTransferReq::new(from, to, value, chain_code, symbol);
    let res = wallet_manager.transaction_fee(params).await;

    tracing::info!("res: {}", serde_json::to_string(&res).unwrap());
}

// 转账
#[tokio::test]
async fn test_transfer() {
    let wallet_manager = get_manager().await;

    let from = "0x38Fb5978e1C0D2A419Acd3ae3e99CD57bf331fc3";
    let to = "0xF7d5c082Ce49922913404b56168EBa82Dda4c1F7";
    let value = "0.0001";
    let chain_code = "bnb";
    let symbol = "BNB";
    let password = "123456";
    // let notes = "test";

    let base = transaction::BaseTransferReq::new(from, to, value, chain_code, symbol);
    let params = transaction::TransferReq {
        base,
        password: password.to_string(),
        fee_setting:
            r#"{"gasLimit":23100,"baseFee":"0","priorityFee":"1000000000","maxFeePerGas":"1000000000"}"#
                .to_string(),
        signer:None,
    };

    let token_fee = wallet_manager.transfer(params).await;
    println!("token transaction: {:?}", token_fee);
}
