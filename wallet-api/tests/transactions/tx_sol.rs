use crate::get_manager;
use wallet_api::request::transaction;

// 余额测试
#[tokio::test]
async fn test_balance() {
    let wallet_manager = get_manager().await;

    let addr = "37qZgmfhQNvjTfycUeXte3sAucAY4iaqoTZfhFxZb7L1";
    let chain_code = "sol";
    let symbol = "SOL";
    // let token_address = Some("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string());
    let token_address = None;

    let balance = wallet_manager.chain_balance(addr, chain_code, &symbol, token_address).await;

    println!("balance: {:?}", balance);
}

//交易的手续费
#[tokio::test]
async fn test_fee() {
    let wallet_manager = get_manager().await;

    let from = "HVWfJZUBx4Vxn8TgQivP4p3j5gm5pXzBDPeq3JUqZSW";
    let to = "E3LwNpfHFWKnzqdBjmEbjBdYYByfrQCgeDYoemxs3yLu";
    let value = "0.01";
    let chain_code = "sol";
    let symbol = "USDT";

    let params = transaction::BaseTransferReq::new(from, to, value, chain_code, symbol);
    let res = wallet_manager.transaction_fee(params).await;
    tracing::info!("fee: {:#?}", res);
}

// 转账
#[tokio::test]
async fn test_transfer() {
    let wallet_manager = get_manager().await;

    let from = "CynRxb8RZTuX3cAdTAPoKLLysUnDS482E9z54ySLimQ";
    let to = "GE93MHXVvnsbhxu7Ttpp7zTiipJeCX3QFXueSK2dCJe6";
    let value = "0.005";
    let chain_code = "sol";
    let symbol = "SOL";
    // let symbol = "STK";
    let password = "123456";
    // let notes = "test";

    let base = transaction::BaseTransferReq::new(from, to, value, chain_code, symbol);
    let params = transaction::TransferReq {
        base,
        password: password.to_string(),
        fee_setting: "".to_string(),
        signer: None,
    };

    let token_fee = wallet_manager.transfer(params).await;
    println!("token transaction: {:?}", token_fee);
}
