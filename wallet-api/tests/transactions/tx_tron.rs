use crate::get_manager;
use wallet_api::request::transaction::{self, Signer};

#[tokio::test]
async fn test_balance() {
    let wallet_manager = get_manager().await;

    let addr = "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1";
    let chain_code = "tron";
    let symbol = "TRX";
    // let symbol = "USDT";
    let balance = wallet_manager
        .chain_balance(addr, chain_code, &symbol)
        .await;

    tracing::info!("balance: {:?}", balance);
}

// 交易手续费
#[tokio::test]
async fn test_fee() {
    let wallet_manager = get_manager().await;

    let from = "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1";
    let to = "TTofbJMU2iMRhA39AJh51sYvhguWUnzeB1";
    let value = "10";
    let chain_code = "tron";
    let symbol = "TRX";
    // let symbol = "USDT";

    let mut params = transaction::BaseTransferReq::new(
        from.to_string(),
        to.to_string(),
        value.to_string(),
        chain_code.to_string(),
        symbol.to_string(),
    );
    params.with_notes("test aa".to_string());

    let res = wallet_manager.transaction_fee(params).await;
    tracing::info!("res: {}", serde_json::to_string(&res).unwrap());
}

// 转账
#[tokio::test]
async fn test_transfer() {
    let wallet_manager = get_manager().await;

    let from = "TFzMRRzQFhY9XFS37veoswLRuWLNtbyhiB";
    let to = "TTofbJMU2iMRhA39AJh51sYvhguWUnzeB1";
    let value = "0.1";
    let symbol = "TRX";
    let chain_code = "tron";
    // let password = "123456";
    let password = "q1111111";
    let notes = "test".to_string();

    let signer = Signer {
        address: "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string(),
        permission_id: 5,
    };
    let mut base = transaction::BaseTransferReq::new(
        from.to_string(),
        to.to_string(),
        value.to_string(),
        chain_code.to_string(),
        symbol.to_string(),
    );
    base.with_notes(notes);

    let params = transaction::TransferReq {
        base,
        password: password.to_string(),
        fee_setting: "".to_string(),
        signer: Some(signer),
    };

    let transfer = wallet_manager.transfer(params).await;
    println!("transfer: {:?}", transfer);
}

// 转账有补贴的情况
#[tokio::test]
async fn test_transfer_with_subsidy() {
    let wallet_manager = get_manager().await;

    let from = "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1";
    let to = "TTofbJMU2iMRhA39AJh51sYvhguWUnzeB1";
    let value = "0.1";
    let symbol = "USDT";
    let chain_code = "tron";
    let password = "123456";

    // request delegate
    let res = wallet_manager.request_energy(from.to_string(), 50000).await;
    tracing::warn!("request_energy: {:?}", res);

    let mut base = transaction::BaseTransferReq::new(
        from.to_string(),
        to.to_string(),
        value.to_string(),
        chain_code.to_string(),
        symbol.to_string(),
    );
    base.request_resource_id = res.result;

    let params = transaction::TransferReq {
        base,
        password: password.to_string(),
        fee_setting: "".to_string(),
        signer: None,
    };

    let transfer = wallet_manager.transfer(params).await;
    tracing::info!("transfer: {:?}", transfer);
}
