use crate::multisig_tx::get_manager;
use wallet_api::{response_vo::transaction::TransferParams, MemberVo};

#[tokio::test]
async fn test_create_multisig_account() {
    let wallet_manager = get_manager().await;
    let address = "0xdc4778f200c36a1C9dEeb3164cEE8366aD1F9455".to_string();
    let chain_code = "eth".to_string();
    let threshold = 2;

    let member1 = MemberVo::new(
        "alice".to_string(),
        "0xdc4778f200c36a1C9dEeb3164cEE8366aD1F9455".to_string(),
    );

    let member2 = MemberVo::new(
        "bob".to_string(),
        "0xB0E488276b0467aa98ba3c74e098F3C845B78974".to_string(),
    );

    // let member3 = Member::new(
    //     "charlie".to_string(),
    //     "0x0fE1bF79406257BaF7114EacA9BE5b78F27A4441".to_string(),
    // );
    let member_list = vec![member1, member2];

    let res = wallet_manager
        .create_multisig_account(
            "multisig".to_string(),
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

    let addr = "0xdc4778f200c36a1C9dEeb3164cEE8366aD1F9455";
    let chain_code = "eth";
    // let symbol = "bnb";
    let symbol = "ETH";
    let balance = wallet_manager
        .chain_balance(addr, chain_code, &symbol)
        .await;

    tracing::info!("balance: {:?}", balance);
}

#[tokio::test]
async fn test_create_transfer() {
    let manager = get_manager().await;

    let password = "123456".to_string();
    let params = TransferParams {
        from: "0xd2fC4383d6E8A2177Ac93D9f291f6dc98f6895c9".to_owned(),
        to: "0x3EC161C02Cd5a49EE8657947DC99DA58D1259aA1".to_owned(),
        value: "0.001".to_owned(),
        expiration: Some(10),
        chain_code: "bnb".to_owned(),
        symbol: "BNB".to_owned(),
        password,
        notes: Some("salary".to_string()),
    };

    // 创建交易
    let res = manager.create_multisig_queue(params).await;
    let res = serde_json::to_string(&res).unwrap();
    tracing::info!("tx info of = {:?}", res);
}

#[tokio::test]
async fn test_queue_list() {
    let manager = get_manager().await;

    let res = manager.multisig_queue_list(None, None, 1, 0, 10).await;
    let res = serde_json::to_string(&res).unwrap();
    tracing::info!("queue list = {:?}", res);
}

#[tokio::test]
async fn test_queue_info() {
    let manager = get_manager().await;

    // 队列详情
    let id = "177281407150854144".to_string();
    let res = manager.multisig_queue_info(id).await;
    let res = serde_json::to_string(&res).unwrap();
    tracing::info!("queue info = {:?}", res);
}

// 签名交易
#[tokio::test]
async fn test_sign_transaction() {
    let wallet_manager = get_manager().await;

    let queue_id = "169234572050042880".to_owned();
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

    let queue_id = "203256774709612544".to_owned();
    let fee = wallet_manager
        .estimate_multisig_transfer_fee(queue_id)
        .await;

    tracing::info!("transfer fee = {:?}", serde_json::to_string(&fee).unwrap());
}

// 执行交易
#[tokio::test]
async fn test_execute() {
    let wallet_manager = get_manager().await;
    let id = "177281407150854144".to_string();

    let fee = r#"{"gasLimit": 262975,"baseFee": "3329262291","priorityFee": "0","maxFeePerGas": "3995114749"}"#
    .to_string();
    let fee = Some(fee);

    let password = "123456".to_string();
    let result = wallet_manager
        .exec_transaction(id, password, fee, None)
        .await;
    tracing::info!(
        "execute res = {:?}",
        serde_json::to_string(&result).unwrap()
    );
}
