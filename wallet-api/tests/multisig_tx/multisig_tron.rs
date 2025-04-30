use crate::get_manager;
use wallet_api::{
    request::transaction::Signer, response_vo::transaction::TransferParams, MemberVo,
};

#[tokio::test]
async fn test_create_multisig_account() {
    let wallet_manager = get_manager().await;
    let address = "TLK9t3ht5GE1oYPx8pdoG1PScQdJgS7Pwb".to_string();
    let chain_code = "tron".to_string();

    let threshold = 2;
    let member1 = MemberVo::new(
        "account_1".to_string(),
        "TLK9t3ht5GE1oYPx8pdoG1PScQdJgS7Pwb".to_string(),
    );

    let member2 = MemberVo::new(
        "account_0".to_string(),
        "TPJpRdGsED9VEffUnFoaXyYtkpn2MXxtow".to_string(),
    );

    // let member3 = MemberVo::new(
    //     "account_3".to_string(),
    //     "TF5qaPzkzB9s8o8omFP4wNwW1Gxtcx4zQr".to_string(),
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

    let _signer = Signer {
        address: "TKDDywzwyYJD8n1BMy5cqr7cxjEFaKJ8h3".to_string(),
        permission_id: 2,
    };
    let signer = None;

    let params = TransferParams {
        from: "TQnSwWGaFkT2zjumDJkbaFi4uRAvEq4An1".to_owned(),
        to: "TXjTCY6MvvTpxiNaHdkYsRk5FUZ4kh3fUh".to_owned(),
        value: "2".to_owned(),
        expiration: Some(5),
        chain_code: "tron".to_owned(),
        symbol: "TRX".to_owned(),
        notes: Some("salary".to_string()),
        spend_all: false,
        signer,
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
    let res = manager.multisig_queue_list(None, None, 2, 0, 10).await;
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

    let queue_id = "254347646645440512".to_owned();
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

    let queue_id = "255392028073005056".to_owned();
    let fee = wallet_manager
        .estimate_multisig_transfer_fee(queue_id)
        .await;

    tracing::info!("transfer fee = {}", serde_json::to_string(&fee).unwrap());
}

// 执行交易
#[tokio::test]
async fn test_execute() {
    let wallet_manager = get_manager().await;
    let id = "255436715676798976".to_string();

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

    let queue_id = "255435582476521472".to_string();
    let rs = wallet_manager.cancel_queue(queue_id).await;

    tracing::info!("res {}", serde_json::to_string(&rs).unwrap());
}
