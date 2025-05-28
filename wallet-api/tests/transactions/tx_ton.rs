use crate::get_manager;
use wallet_api::request::transaction;

// 余额测试
#[tokio::test]
async fn test_balance() {
    let wallet_manager = get_manager().await;

    let addr = "UQAj45nzNLyAKtnP038PCrqGxwUEpgdrGyz9keGedamIafpw";
    let chain_code = "ton";
    let symbol = "USDT";

    let balance = wallet_manager
        .chain_balance(addr, chain_code, &symbol)
        .await;

    println!("balance: {:?}", balance);
}

//交易的手续费
#[tokio::test]
async fn test_fee() {
    let wallet_manager = get_manager().await;

    let from = "UQAj45nzNLyAKtnP038PCrqGxwUEpgdrGyz9keGedamIafpw";
    let to = "UQAPwCDD910mi8FO1cd5qYdfTHWwEyqMB-RsGkRv-PI2w05u";
    let value = "0.01";
    let chain_code = "ton";
    let symbol = "TON";

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

    let from = "UQAj45nzNLyAKtnP038PCrqGxwUEpgdrGyz9keGedamIafpw";
    let to = "UQAPwCDD910mi8FO1cd5qYdfTHWwEyqMB-RsGkRv-PI2w05u";
    let value = "0.01";
    let chain_code = "ton";
    let symbol = "USDT";
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
        fee_setting: "".to_string(),
        signer: None,
    };

    let token_fee = wallet_manager.transfer(params).await;
    println!("token transaction: {:?}", token_fee);
}

#[test]
fn fee_cal() {
    let before = 8.780073749;
    let after = 8.773793064;

    let diff = before - after;
    let fee = 0.002574306;

    println!("diff: {}", diff);
    println!("feee: {}", fee);
}
