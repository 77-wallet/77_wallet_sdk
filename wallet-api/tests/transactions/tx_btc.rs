use crate::get_manager;
use wallet_api::request::transaction;
// 余额测试
#[tokio::test]
async fn test_balance() {
    let wallet_manager = get_manager().await;

    let addr = "2N3w6RypEQBGpDLd7Dq57TmYTZLZwoV3ft2";
    let chain_code = "btc";
    let symbol = "BTC";

    let balance = wallet_manager
        .chain_balance(addr, chain_code, &symbol)
        .await;

    println!("balance: {:?}", balance);
}

//交易的手续费
#[tokio::test]
async fn test_fee() {
    let wallet_manager = get_manager().await;

    let from = "2N3w6RypEQBGpDLd7Dq57TmYTZLZwoV3ft2";
    let to = "bcrt1qwmactnqqvl6d3ddxudatqxvvys33275zqls052";
    let value = "10";
    let chain_code = "btc";
    let symbol = "BTC";

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

    let from = "2N3w6RypEQBGpDLd7Dq57TmYTZLZwoV3ft2";
    let to = "bcrt1qwmactnqqvl6d3ddxudatqxvvys33275zqls052";
    let value = "10";
    let chain_code = "btc";
    let symbol = "BTC";
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
        fee_setting:  r#"{"gasLimit":50706,"baseFee":"14058071718","priorityFee":"64000000","maxFeePerGas":"21183107577"}"#.to_string(),
    };

    let token_fee = wallet_manager.transfer(params).await;
    tracing::info!(
        "test_transfer: {}",
        serde_json::to_string(&token_fee).unwrap()
    );
}
