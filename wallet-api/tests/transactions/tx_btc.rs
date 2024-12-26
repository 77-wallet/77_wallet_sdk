use crate::get_manager;
use wallet_api::request::transaction;
// 余额测试
#[tokio::test]
async fn test_balance() {
    let wallet_manager = get_manager().await;

    let addr = "bc1qgs3l6uh0atn3ks807anzy8sqhvtc2j9dv8axa7";
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

    let from = "bc1qc7ggkxcdlm4xppjjja5c55acrl8uhfry36pple";
    let to = "3J2sYwhMJjmjQHe8Ujja9JfySYxu657YGM";
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

    let from = "bc1qlhmzrl93pnrfdwtxr4edcse73jkk2xwuleu94u";
    let to = "bcrt1qwmactnqqvl6d3ddxudatqxvvys33275zqls052";
    let value = "0.0003";
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
