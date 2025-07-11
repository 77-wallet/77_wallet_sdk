use crate::get_manager;
use wallet_api::request::transaction;

// 余额测试
#[tokio::test]
async fn test_balance() {
    let wallet_manager = get_manager().await;

    let addr = "0xfba1550112b16f3608669c8ab4268366c7bacb3a2cb844594ad67c21af85a1dd";
    let chain_code = "sui";
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

    let from = "0xfba1550112b16f3608669c8ab4268366c7bacb3a2cb844594ad67c21af85a1dd";
    let to = "0xa042c3ba8208964374cc050922ec94e85fdffe9fc0cd656fb623642ae2fdb4c0";
    let value = "0.01";
    let chain_code = "sui";
    let symbol = "USDT";

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

    let from = "0xfba1550112b16f3608669c8ab4268366c7bacb3a2cb844594ad67c21af85a1dd";
    let to = "0x427e388a85bbb00da32af7716b15749a147affb8ed3a87cd7ce1f98f0b20bc6f";
    let value = "0.01";
    let chain_code = "sui";
    let symbol = "SUI";
    let password = "123456";

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
        fee_setting: r#""#.to_string(),
        signer: None,
    };

    let token_fee = wallet_manager.transfer(params).await;
    println!("token transaction: {:?}", token_fee);
}
