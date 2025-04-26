use crate::get_manager;
use wallet_api::request::transaction;
// 余额测试
#[tokio::test]
async fn test_balance() {
    let wallet_manager = get_manager().await;

    let addr = "LPksEuS2ZeN89BwKQkJw4HAAivrruFDn3j";
    let chain_code = "ltc";
    let symbol = "LTC";

    let balance = wallet_manager
        .chain_balance(addr, chain_code, &symbol)
        .await;

    println!("balance: {:?}", balance);
}

//交易的手续费
#[tokio::test]
async fn test_fee() {
    let wallet_manager = get_manager().await;

    let from = "LPksEuS2ZeN89BwKQkJw4HAAivrruFDn3j";
<<<<<<< HEAD
    let to = "ltc1pwwjsndarfzcppq69ax5ghce4q074c2856rcy094tz55du05uxlkqs940d9";
=======
    let to = "ltc1q4qj00nf5ye30a6ctfgegczfsjja0749ysthwms";
>>>>>>> f3f332f (ltc transaction)
    let value = "0.001";
    let chain_code = "ltc";
    let symbol = "LTC";

    let mut params = transaction::BaseTransferReq::new(
        from.to_string(),
        to.to_string(),
        value.to_string(),
        chain_code.to_string(),
        symbol.to_string(),
    );
    params.with_spend_all(false);

    let res = wallet_manager.transaction_fee(params).await;
    tracing::info!("token_fee: {}", serde_json::to_string(&res).unwrap());
}

// 转账
#[tokio::test]
async fn test_transfer() {
    let wallet_manager = get_manager().await;

    let from = "LPksEuS2ZeN89BwKQkJw4HAAivrruFDn3j";
<<<<<<< HEAD
    let to = "ltc1pwwjsndarfzcppq69ax5ghce4q074c2856rcy094tz55du05uxlkqs940d9";
=======
    let to = "ltc1q4qj00nf5ye30a6ctfgegczfsjja0749ysthwms";
>>>>>>> f3f332f (ltc transaction)
    let value = "0.001";
    let chain_code = "ltc";
    let symbol = "LTC";
    let password = "123456";

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
        signer:None,
    };

    let token_fee = wallet_manager.transfer(params).await;
    tracing::info!(
        "test_transfer: {}",
        serde_json::to_string(&token_fee).unwrap()
    );
}
