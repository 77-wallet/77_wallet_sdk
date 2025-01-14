use crate::multisig_tx::get_manager;
use wallet_api::{
    response_vo::{transaction::TransferParams, MultisigQueueFeeParams},
    MemberVo,
};

#[tokio::test]
async fn test_create_multisig_account_sol() {
    let wallet_manager = get_manager().await;
    let address = "4tyeH6KgV2ZHsE7D4ctxT2wpfqYqe5aMM7VJABqaQ3H9".to_string();
    let chain_code = "sol".to_string();
    let threshold = 2;

    let member1 = MemberVo {
        name: "alice".to_string(),
        address: "4tyeH6KgV2ZHsE7D4ctxT2wpfqYqe5aMM7VJABqaQ3H9".to_string(),
        confirmed: 0,
        pubkey: "".to_string(),
        uid: "".to_string(),
    };
    let member2 = MemberVo {
        name: "bob".to_string(),
        address: "8mod4aqksHLqPsxuXADZSrv4kpAbDiw3CPGPYeFgjMQJ".to_string(),
        confirmed: 0,
        pubkey: "".to_string(),
        uid: "".to_string(),
    };
    let member3 = MemberVo {
        name: "carol".to_string(),
        address: "2Gm9EvTvQETCnGXZCvNa1f6wasyXSTmmC2kJ6Nk5vFxB".to_string(),
        confirmed: 0,
        pubkey: "".to_string(),
        uid: "".to_string(),
    };
    let member_list = vec![member1, member2, member3];

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

    // let addr = "6CeRb3itd3VE3jTJRBTVgzQKQRiJYbAJDxPKcNAKY2n4";
    let addr = "2t2bb63CcxSE6gWZHvAHc6q24ub9vyoWFEKuxqALkyfX";
    let chain_code = "sol";
    let symbol = "SOL";
    let balance = wallet_manager
        .chain_balance(addr, chain_code, &symbol)
        .await;

    tracing::info!("balance: {:?}", balance);
}

#[tokio::test]
async fn test_create_queue_fee() {
    let manager = get_manager().await;

    let params = MultisigQueueFeeParams {
        from: "E14TXognDNooKg4NsWjYnEj6FP9HmDFPFokP2QZwY3CG".to_owned(),
        to: "GE93MHXVvnsbhxu7Ttpp7zTiipJeCX3QFXueSK2dCJe6".to_owned(),
        value: "0.001".to_owned(),
        chain_code: "sol".to_owned(),
        symbol: "SOL".to_owned(),
    };

    // 创建交易
    let res = manager.create_queue_fee(params).await;
    let res = serde_json::to_string(&res).unwrap();
    tracing::info!("transaction fee = {}", res);
}

#[tokio::test]
async fn test_create_transfer() {
    let manager = get_manager().await;

    let password = "123456".to_string();
    let params = TransferParams {
        // from: "6CeRb3itd3VE3jTJRBTVgzQKQRiJYbAJDxPKcNAKY2n4".to_owned(),
        from: "2t2bb63CcxSE6gWZHvAHc6q24ub9vyoWFEKuxqALkyfX".to_owned(),
        to: "GE93MHXVvnsbhxu7Ttpp7zTiipJeCX3QFXueSK2dCJe6".to_owned(),
        value: "0.01".to_owned(),
        expiration: Some(1),
        chain_code: "sol".to_owned(),
        symbol: "SOL".to_owned(),
        password,
        notes: Some("salary".to_string()),
        spend_all: false,
    };

    // 创建交易
    let res = manager.create_multisig_queue(params).await;
    let res = serde_json::to_string(&res).unwrap();
    tracing::info!("tx info of = {:?}", res);
}

#[tokio::test]
async fn test_queue_list() {
    let manager = get_manager().await;

    // 创建交易
    let res = manager.multisig_queue_list(None, None, 1, 0, 10).await;
    let res = serde_json::to_string(&res).unwrap();
    tracing::info!("queue list = {:?}", res);
}

#[tokio::test]
async fn test_queue_info() {
    let manager = get_manager().await;

    // 队列详情
    let id = "159405911742484480".to_string();
    let res = manager.multisig_queue_info(id).await;
    let res = serde_json::to_string(&res).unwrap();
    tracing::info!("queue info = {:?}", res);
}

// 签名交易
#[tokio::test]
async fn test_sign_transaction() {
    let wallet_manager = get_manager().await;

    let queue_id = "185488559585759232".to_owned();
    let status = 1;
    let password = "123456".to_string();

    let address = "8mod4aqksHLqPsxuXADZSrv4kpAbDiw3CPGPYeFgjMQJ".to_string();

    let sign = wallet_manager
        .sign_transaction(queue_id, status, password, Some(address))
        .await;

    tracing::info!("sign res  = {:?}", sign);
}

#[tokio::test]
async fn test_sign_fee() {
    let wallet_manager = get_manager().await;

    let queue_id = "185488559585759232".to_owned();
    let address = "2Gm9EvTvQETCnGXZCvNa1f6wasyXSTmmC2kJ6Nk5vFxB".to_string();
    let sign = wallet_manager.sign_fee(queue_id, address).await;

    tracing::info!("sign fee  = {:?}", sign);
}

#[tokio::test]
async fn test_multisig_transfer_fee() {
    let wallet_manager = get_manager().await;

    let queue_id = "173490125987254272".to_owned();
    let fee = wallet_manager
        .estimate_multisig_transfer_fee(queue_id)
        .await;

    tracing::info!("transfer fee = {:?}", serde_json::to_string(&fee));
}

// 执行交易
#[tokio::test]
async fn test_execute() {
    let wallet_manager = get_manager().await;

    let id = "185488559585759232".to_string();
    let pass = "123456".to_string();
    let fee_setting = None;
    let request_resource_id = None;

    let result = wallet_manager
        .exec_transaction(id, pass, fee_setting, request_resource_id)
        .await;
    tracing::info!("execute res = {:?}", serde_json::to_string(&result));
}
