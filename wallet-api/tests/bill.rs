use std::{env, path::PathBuf};
use wallet_api::{request::transaction::QueryBillReusltReq, WalletManager};
use wallet_utils::init_test_log;

async fn get_manager() -> WalletManager {
    init_test_log();
    let path = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("test_data")
        .to_string_lossy()
        .to_string();

    WalletManager::new("sn", "ANDROID", &path, None)
        .await
        .unwrap()
}

#[tokio::test]
async fn bill_detail() {
    let wallet_manager = get_manager().await;

    let hash = "64a923c3ebf0d13be8cab729e632e0fa48a4bc1a6640c5cdb50cffb143ed11e9";
    let owner = "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1";
    let detail = wallet_manager.bill_detail(&hash, &owner).await;

    tracing::info!("result {}", serde_json::to_string(&detail).unwrap());
}

#[tokio::test]
async fn bill_lists() {
    let wallet_manager = get_manager().await;

    let root_addr: Option<String> = None;
    let account_id: Option<u32> = None;
    let addr = Some("0x148805B49819371EEF9A822f7F880b42Cf67834D".to_string());
    let chain_code: Option<String> = Some("eth".to_string());
    // let symbol = Some("TRX".to_string());
    let symbol = None;
    let is_multisig = None;
    let filter_min_value = Some(false);
    let start_time = None;
    let end_time = None;

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

    let req = vec![QueryBillReusltReq {
        tx_hash: "0x10dfac1d3835dfd850ccdf79ba1f440630f127197d3285b8c30930688b460e0a".to_string(),
        transfer_type: 1,
    }];

    let c = wallet_manager.query_tx_result(req).await;

    tracing::info!("查询结果{:?}", c);
}

#[tokio::test]
async fn test_sync_bill() {
    let wallet_manager = get_manager().await;

    let chain_code = "eth".to_string();
    let address = "0x1457a81B300cB106187Dd227b0319E2a851BAb24".to_string();
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

    let symbol = "WIN".to_string();
    let addr = "TAqUJ9enU8KkZYySA51iQim7TxbbdLR2wn".to_string();
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
