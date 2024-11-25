use crate::get_manager;
use wallet_api::request::transaction;

// 余额测试
#[tokio::test]
async fn test_balance() {
    let wallet_manager = get_manager().await;

    let addr = "0xd2fC4383d6E8A2177Ac93D9f291f6dc98f6895c9";
    // let addr = "0x5985CE40d3dACf7c1352e464691BC7fb03215928";
    let chain_code = "bnb";
    // let symbol = "STK";
    let symbol = "BNB";

    let balance = wallet_manager
        .chain_balance(addr, chain_code, &symbol)
        .await;

    println!("balance: {:?}", balance);
}

//交易的手续费
#[tokio::test]
async fn test_fee() {
    let wallet_manager = get_manager().await;

    let from = "0xd2fC4383d6E8A2177Ac93D9f291f6dc98f6895c9";
    let to = "0x3EC161C02Cd5a49EE8657947DC99DA58D1259aA1";
    let value = "0.001";
    let chain_code = "bnb";
    let symbol = "BNB";

    let params = transaction::BaseTransferReq::new(
        from.to_string(),
        to.to_string(),
        value.to_string(),
        chain_code.to_string(),
        symbol.to_string(),
    );
    let res = wallet_manager.transaction_fee(params).await;

    tracing::info!("res: {}", serde_json::to_string(&res).unwrap());
}

// 转账
#[tokio::test]
async fn test_transfer() {
    let wallet_manager = get_manager().await;

    let from = "0xdc4778f200c36a1C9dEeb3164cEE8366aD1F9455";
    let to = "0xd2fC4383d6E8A2177Ac93D9f291f6dc98f6895c9";
    let value = "0.02";
    let chain_code = "bnb";
    let symbol = "BNB";
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
        fee_setting:
            r#"{"gasLimit":22796,"baseFee":"0","priorityFee":"1000000000","maxFeePerGas":"1500000000"}"#
                .to_string(),
    };

    let token_fee = wallet_manager.transfer(params).await;
    println!("token transaction: {:?}", token_fee);
}
