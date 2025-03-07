use wallet_api::{
    request::transaction::Signer, response_vo::transaction::TransferParams, MemberVo,
};

use crate::get_manager;

#[tokio::test]
async fn test_create_multisig_account() {
    let wallet_manager = get_manager().await;
    let address = "TNPTj8Dbba6YxW5Za6tFh6SJMZGbUyucXQ".to_string();
    let chain_code = "tron".to_string();

    let threshold = 2;
    let member1 = MemberVo::new(
        "account_1".to_string(),
        "TNPTj8Dbba6YxW5Za6tFh6SJMZGbUyucXQ".to_string(),
    );

    let member2 = MemberVo::new(
        "account_0".to_string(),
        "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string(),
    );

    // let member3 = MemberVo::new(
    //     "account_3".to_string(),
    //     "TUe3T6ErJvnoHMQwVrqK246MWeuCEBbyuR".to_string(),
    // );

    let member_list = vec![member1, member2];

    let res = wallet_manager
        .create_multisig_account(
            "".to_string(),
            address,
            chain_code,
            threshold,
            member_list,
            None,
        )
        .await;

    tracing::info!("{:?}", serde_json::to_string(&res));
}

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

#[tokio::test]
async fn test_create_transfer() {
    let manager = get_manager().await;

    let password = "123456".to_string();

    let signer = Signer {
        address: "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string(),
        permission_id: 4,
    };

    let params = TransferParams {
        from: "TNPTj8Dbba6YxW5Za6tFh6SJMZGbUyucXQ".to_owned(),
        to: "TUe3T6ErJvnoHMQwVrqK246MWeuCEBbyuR".to_owned(),
        value: "5".to_owned(),
        expiration: Some(5),
        chain_code: "tron".to_owned(),
        symbol: "TRX".to_owned(),
        notes: Some("salary".to_string()),
        spend_all: false,
        signer: Some(signer),
    };

    // 创建交易
    let res = manager.create_multisig_queue(params, password).await;
    let res = serde_json::to_string(&res).unwrap();
    tracing::info!("tx info of = {:?}", res);
}

#[tokio::test]
async fn test_queue_list() {
    let manager = get_manager().await;

    // 列表
    let res = manager.multisig_queue_list(None, None, 3, 0, 10).await;
    let res = serde_json::to_string(&res).unwrap();
    tracing::info!("queue list = {}", res);
}

#[tokio::test]
async fn test_queue_info() {
    let manager = get_manager().await;

    // 队列详情
    let id = "213831908549857280".to_string();
    let res = manager.multisig_queue_info(id).await;
    let res = serde_json::to_string(&res).unwrap();
    tracing::info!("queue info = {}", res);
}

// 签名交易
#[tokio::test]
async fn test_sign_transaction() {
    let wallet_manager = get_manager().await;

    let queue_id = "236256927787651072".to_owned();
    let status = 1;
    let password = "123456".to_string();
    let sign = wallet_manager
        .sign_transaction(queue_id, status, password, None)
        .await;

    tracing::info!("sign res  = {:?}", sign);
}

#[tokio::test]
async fn test_multisig_transfer_fee() {
    let wallet_manager = get_manager().await;

    let queue_id = "213839824367521792".to_owned();
    let fee = wallet_manager
        .estimate_multisig_transfer_fee(queue_id)
        .await;

    tracing::info!("transfer fee = {}", serde_json::to_string(&fee).unwrap());
}

// 执行交易
#[tokio::test]
async fn test_execute() {
    let wallet_manager = get_manager().await;
    let id = "236631132983136256".to_string();

    let password = "123456".to_string();
    let fee = None;

    let result = wallet_manager
        .exec_transaction(id, password, fee, None)
        .await;
    tracing::info!("execute res = {}", serde_json::to_string(&result).unwrap());
}

#[tokio::test]
async fn test_check_ongoing() {
    let wallet_manager = get_manager().await;

    let chain_code = "btc".to_string();
    let address = "7xFhDzUVuirPCW8buDk9AqFcyuZ6CzMYv1Ah1GzK6Q5a".to_string();
    let rs = wallet_manager
        .check_ongoing_queue(chain_code, address)
        .await;

    tracing::info!("res {}", serde_json::to_string(&rs).unwrap());
}

#[tokio::test]
async fn test_cancel_queue() {
    let wallet_manager = get_manager().await;

    let queue_id = "236235581254930432".to_string();
    let rs = wallet_manager.cancel_queue(queue_id).await;

    tracing::info!("res {}", serde_json::to_string(&rs).unwrap());
}
