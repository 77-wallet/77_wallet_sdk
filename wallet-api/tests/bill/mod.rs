use wallet_api::request::transaction::QueryBillResultReq;

use crate::get_manager;

#[tokio::test]
async fn bill_detail() {
    let wallet_manager = get_manager().await;

    let hash = "04c47bce8ef92ba8b00db758642f24aa2f1b3703abedf2a10b7fadb9c82011f8";
    let owner = "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1";
    let detail = wallet_manager.bill_detail(&hash, &owner).await;

    tracing::info!("result {}", serde_json::to_string(&detail).unwrap());
}

#[tokio::test]
async fn bill_list_by_hashs() {
    let owner = "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string();
    let hashs = vec![
        "8428e5eab37eab44a477bd252a1e08025dd7f0ca725ff469475865e981d36554".to_string(),
        "ff3cc86c69983e774469aa3cc309b51aedb83dae9a4053427e35b88d20397ffb".to_string(),
    ];

    let wallet_manager = get_manager().await;

    let res = wallet_manager.list_by_hashs(owner, hashs).await;
    tracing::info!("result {}", serde_json::to_string(&res).unwrap());
}

#[tokio::test]
async fn bill_lists() {
    let wallet_manager = get_manager().await;

    let root_addr: Option<String> = None;
    let account_id: Option<u32> = None;
    let addr = Some("TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string());
    let chain_code: Option<String> = Some("tron".to_string());
    // let symbol = Some("TRX".to_string());
    let symbol = Some("TRX".to_string());
    let is_multisig = None;
    let filter_min_value = Some(false);
    let start_time = None;
    let end_time = None;

    let transfer_type = None;

    let page = 0;
    let page_size = 5;

    let detail = wallet_manager
        .bill_lists(
            root_addr,
            account_id,
            is_multisig,
            addr,
            chain_code,
            symbol,
            filter_min_value,
            start_time,
            end_time,
            transfer_type,
            page,
            page_size,
        )
        .await;
    // tracing::info!("{}", serde_json::to_string(&detail).unwrap());
    tracing::info!("{:#?}", detail);
}

#[tokio::test]
async fn query_bill_result() {
    let wallet_manager = get_manager().await;

    let req = vec![QueryBillResultReq {
        tx_hash: "2PfPFzv1u9yPgFsNdK4Amku2jb6cZ39u2WueAPkdnGJuuBvajDBfxWfvVESK8oCxctXFtyYwAxRvnfgfsrcWcZTx".to_string(),
        owner: "5PvYfcS6M4VpeUoFCVQEWjkM4fFyxh4Gq2HTovW45LdN".to_string(),
    }];

    let c = wallet_manager.query_tx_result(req).await;

    tracing::info!("查询结果{:?}", c);
}

#[tokio::test]
async fn test_sync_bill() {
    let wallet_manager = get_manager().await;

    let chain_code = "sol".to_string();
    let address = "BkbVqU6xGt58eqAqZYk32BpC3stazj8oeKFbLdqsvdDK".to_string();
    let _c = wallet_manager.sync_bill(chain_code, address).await;
    tracing::warn!("同步结果{:?}", _c);
}

#[tokio::test]
async fn test_sync_bill_by_address() {
    let wallet_manager = get_manager().await;

    let wallet_address = "0x3d669d78532F763118561b55daa431956ede4155".to_string();
    let account_id = 1;
    let _c = wallet_manager
        .sync_bill_by_wallet_and_account(wallet_address, account_id)
        .await;
    tracing::warn!("同步结果{:?}", _c);
}

#[tokio::test]
async fn recent_bill() {
    let wallet_manager = get_manager().await;

    let symbol = "TRX".to_string();
    let addr = "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string();
    let chain_code = "tron".to_string();
    let page = 0;
    let page_size = 10;

    let detail = wallet_manager
        .recent_bill(symbol, addr, chain_code, page, page_size)
        .await;

    // tracing::info!(
    //     "recent bill lists = {}",
    //     serde_json::to_string(&detail).unwrap()
    // );
    tracing::warn!("{:#?}", detail);
}
#[tokio::test]
async fn coin_currency_price() {
    let wallet_manager = get_manager().await;

    let symbol = "TRX".to_string();
    let chain_code = "tron".to_string();

    let res = wallet_manager.coin_currency_price(chain_code, symbol).await;

    tracing::info!("{}", serde_json::to_string(&res).unwrap());
}

#[tokio::test]
async fn test_create_bill() {
    let _wallet_manager = get_manager().await;

    let kind = wallet_database::entities::bill::BillKind::Transfer;
    // 第一次主币
    let params = wallet_database::entities::bill::NewBillEntity::new(
        "121xxx".to_string(),
        "from".to_string(),
        "to".to_string(),
        200.0,
        "eth".to_string(),
        "USDT".to_string(),
        false,
        kind,
        "hello".to_string(),
    )
    .with_transaction_fee("0.003");

    wallet_api::domain::bill::BillDomain::create_bill(params)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_create() {
    let value = wallet_utils::unit::convert_to_u256("0.00330888", 18).unwrap();

    // let b_fee = wallet_utils::unit::u256_from_str("35054491035").unwrap();
    // let p_fee = wallet_utils::unit::u256_from_str("15597128697").unwrap();
    let m_fee = wallet_utils::unit::u256_from_str("65847105651").unwrap();

    let gas_limit = wallet_utils::unit::u256_from_str("23100").unwrap();

    let cost = gas_limit * m_fee;

    let aa = value + cost;
    println!("cost {},aa = {}", cost, aa)
}
