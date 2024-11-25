use crate::get_manager;
use wallet_api::request::transaction;

// 余额测试
#[tokio::test]
async fn test_balance() {
    let wallet_manager = get_manager().await;

    let addr = "4tyeH6KgV2ZHsE7D4ctxT2wpfqYqe5aMM7VJABqaQ3H9";
    let chain_code = "sol";
    let symbol = "SOL";
    // let symbol = "STK";

    let balance = wallet_manager
        .chain_balance(addr, chain_code, &symbol)
        .await;

    println!("balance: {:?}", balance);
}

//交易的手续费
#[tokio::test]
async fn test_fee() {
    let wallet_manager = get_manager().await;

    let from = "4tyeH6KgV2ZHsE7D4ctxT2wpfqYqe5aMM7VJABqaQ3H9";
    let to = "8mod4aqksHLqPsxuXADZSrv4kpAbDiw3CPGPYeFgjMQJ";
    let value = "0.1";
    let chain_code = "sol";
    let symbol = "SOL";

    let params = transaction::BaseTransferReq::new(
        from.to_string(),
        to.to_string(),
        value.to_string(),
        chain_code.to_string(),
        symbol.to_string(),
    );
    let res = wallet_manager.transaction_fee(params).await;
    tracing::info!("fee: {:#?}", res);
}

// 转账
#[tokio::test]
async fn test_transfer() {
    let wallet_manager = get_manager().await;

    let from = "4tyeH6KgV2ZHsE7D4ctxT2wpfqYqe5aMM7VJABqaQ3H9";
    let to = "8mod4aqksHLqPsxuXADZSrv4kpAbDiw3CPGPYeFgjMQJ";
    let value = "0.1";
    let chain_code = "sol";
    let symbol = "SOL";
    // let symbol = "STK";
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
        fee_setting: "".to_string(),
    };

    let token_fee = wallet_manager.transfer(params).await;
    println!("token transaction: {:?}", token_fee);
}
