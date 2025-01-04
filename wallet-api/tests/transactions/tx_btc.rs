use crate::get_manager;
use wallet_api::request::transaction;
// 余额测试
#[tokio::test]
async fn test_balance() {
    let wallet_manager = get_manager().await;

    let addr = "bc1qlmu59q3zjmzfqsljx860pw8sulvwfvgdh337mn";
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

    let from = "bc1qlmu59q3zjmzfqsljx860pw8sulvwfvgdh337mn";
    let to = "3L4PXQqgsh4j6yoGvXLdaHJWPJQumG1yA4";
    let value = "0.00087585";
    let chain_code = "btc";
    let symbol = "BTC";

    let mut params = transaction::BaseTransferReq::new(
        from.to_string(),
        to.to_string(),
        value.to_string(),
        chain_code.to_string(),
        symbol.to_string(),
    );
    params.with_spend_all(true);

    let res = wallet_manager.transaction_fee(params).await;
    tracing::info!("token_fee: {}", serde_json::to_string(&res).unwrap());
}

// 转账
#[tokio::test]
async fn test_transfer() {
    let wallet_manager = get_manager().await;

    let from = "bc1qlmu59q3zjmzfqsljx860pw8sulvwfvgdh337mn";
    let to = "1Jv4gmzbgfJW9zFYLV3rDrd7GroUEuqBTz";
    let value = "0.0000005";
    let chain_code = "btc";
    let symbol = "BTC";
    let password = "123456";
    // let notes = "test";

    let mut base = transaction::BaseTransferReq::new(
        from.to_string(),
        to.to_string(),
        value.to_string(),
        chain_code.to_string(),
        symbol.to_string(),
    );
    base.with_spend_all(false);
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
