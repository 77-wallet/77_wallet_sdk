use crate::get_manager;
use anyhow::Result;
use serde_json::json;
use serial_test::serial;
use wallet_api::testkit::mqtt::{api_wallet_pool, core_pool, exec_wallet_order_payload, task_pool};
use wallet_database::{
    dao::{
        assets::{AssetsDao, CreateAssetsVo},
        chain::ChainDao,
    },
    entities::{
        api_assets::ApiCreateAssetsVo,
        api_chain::{ApiChainCreateVo, NodeBindType},
        api_coin::ApiCoinData,
        assets::AssetsId,
        chain::ChainCreateVo,
    },
    repositories::{
        api_wallet::{assets::ApiAssetsRepo, chain::ApiChainRepo, coin::ApiCoinRepo},
        task_queue::TaskQueueRepo,
    },
};

async fn wait_task_done(msg_id: &str) -> Result<u8> {
    let task_pool = task_pool()?;
    for _ in 0..80 {
        if let Some(task) = TaskQueueRepo::task_detail(&task_pool, msg_id).await? {
            if task.status == 2 || task.status == 3 {
                return Ok(task.status);
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    anyhow::bail!("timeout waiting task status, msg_id={}", msg_id);
}

async fn ensure_sol_chain_active() -> Result<()> {
    let pool = api_wallet_pool()?;
    ApiChainRepo::add(
        &pool,
        ApiChainCreateVo::new(
            "Solana",
            "sol",
            &[String::from("m/44'/501'/0'/0'")],
            NodeBindType::AutoBackend,
            "SOL",
        ),
    )
    .await?;
    Ok(())
}

async fn ensure_eth_chain_active() -> Result<()> {
    let pool = core_pool()?;
    let chain = ChainCreateVo::new(
        "Ethereum",
        "eth",
        &[String::from("eth")],
        NodeBindType::AutoBackend,
        "ETH",
    );
    ChainDao::upsert(pool.as_ref(), chain).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address() -> Result<()> {
    let _manager = get_manager().await;
    ensure_sol_chain_active().await?;
    let api_pool = api_wallet_pool()?;

    let token = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
    let address = "3jVrVbEPDd35piQUxur1Gki8bkz4XkhZTXZHmfSnmHEd";
    let now = wallet_utils::time::now();

    let coin = ApiCoinData::new(
        Some("USD Coin".to_string()),
        "USDC",
        "sol",
        Some(token.to_string()).into(),
        Some("1".to_string()),
        None,
        6,
        1,
        1,
        1,
        now,
        Some(now),
    );
    ApiCoinRepo::upsert_multi_coin(&api_pool, vec![coin]).await?;

    let asset = ApiCreateAssetsVo::new(
        AssetsId::new(address, "sol", Some(token.to_string()).into()),
        "USDC",
        6,
        None,
        0,
    )
    .with_name("USD Coin");
    ApiAssetsRepo::upsert_assets_multi(&api_pool, vec![asset]).await?;

    let msg_id = format!("bug-sol-usdc-{}", now.timestamp_millis());
    let payload = json!({
        "appId": "100d855909c0d553cf9",
        "bizType": "ACCT_CHANGE",
        "body": {
            "blockHeight": 405834872,
            "chainCode": "sol",
            "fromAddr": "9PG6RaXiNm1x5jcVHosc1LnwUgcE3NLLLd7yLHfqStYM",
            "isMultisig": 0,
            "status": true,
            "symbol": "usd coin",
            "toAddr": address,
            "token": token,
            "transactionFee": 0.002045217,
            "transactionTime": "2026-03-12 03:46:40",
            "transferType": 0,
            "txHash": "56jdRtHj6LWHLiSz86tKLG5dj8RXbLHNKW8VVmQqEfYbpmdkb1w5Ko8JpdyH2iSy9J56175Yie3vJboGgyXfrryh",
            "txKind": 1,
            "value": 1.1,
            "valueUsdt": 1.09994888401493623
        },
        "clientId": "4206b0fecd683a1505d24a135b606e9c",
        "deviceType": "ANDROID",
        "sn": "b35f7b556b87c87bb1928ea6ab12ef6918b71f5c37fbd53b88e9353ea2093f0b",
        "uid": "5bdb1b748bb617d6683f57565b1493cfa5f9e45f3086daf265ca2e0cd325c15e",
        "walletType": "API_WAW",
        "msgId": msg_id
    });

    exec_wallet_order_payload(&payload).await?;
    let status = wait_task_done(&msg_id).await?;
    assert_eq!(status, 2, "ApiWalletAcctChange task should succeed");

    let saved = ApiAssetsRepo::find_by_id(
        &api_pool,
        &AssetsId::new(address, "sol", Some(token.to_string()).into()),
    )
    .await?;
    assert!(saved.is_some());
    assert_eq!(saved.unwrap().symbol, "USDC");

    Ok(())
}

#[tokio::test]
#[serial]
async fn acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch() -> Result<()> {
    let _manager = get_manager().await;
    ensure_eth_chain_active().await?;
    let pool = core_pool()?;

    let token = "0xdac17f958d2ee523a2206206994597c13d831ec7";
    let address = "0x148805B49819371EEF9A822f7F880b42Cf67834D";
    let now = wallet_utils::time::now();

    let asset = CreateAssetsVo::new(
        AssetsId::new(address, "eth", Some(token.to_string()).into()),
        "USDT",
        6,
        None,
        0,
    )
    .with_name("Tether USD");
    AssetsDao::upsert_assets(pool.as_ref(), asset).await?;
    assert!(AssetsDao::get_by_addr_token(pool.as_ref(), "eth", token, address).await?.is_some());

    let msg_id = format!("normal-bug-usdt-{}", now.timestamp_millis());
    let payload = json!({
        "msgId": msg_id,
        "bizType": "ACCT_CHANGE",
        "body": {
            "blockHeight": 21342785,
            "chainCode": "eth",
            "fromAddr": "0x8F1E2a99CB688587c02B8b836Ba9Ca39dC60D63B",
            "isMultisig": 0,
            "notes": "acct-change-symbol-mismatch",
            "queueId": "",
            "status": true,
            "symbol": "tether usd",
            "toAddr": address,
            "token": token,
            "transactionFee": 0.00135940441821096,
            "transactionTime": "2026-03-12 10:13:47",
            "transferType": 0,
            "txHash": "0xaaa362dfd318f4da95e2d1e71c8c2a2ceabc8fd5df85e7c144843e6fc55f25e0",
            "txKind": 1,
            "value": 0.1112
        },
        "clientId": "7552bd49a9407eb98164c129d11da7e2",
        "deviceType": "IOS",
        "sn": "5bb0eada7cb7290b5d196362e6def48dcb9703e1468c0fb28eb7dd61073875e6",
        "walletType": "NORMAL_WALLET"
    });

    exec_wallet_order_payload(&payload).await?;
    let status = wait_task_done(&msg_id).await?;
    assert_eq!(status, 2, "AcctChange task should succeed");

    let saved = AssetsDao::get_by_addr_token(pool.as_ref(), "eth", token, address).await?;
    assert!(saved.is_some());
    assert_eq!(saved.unwrap().symbol, "USDT");

    Ok(())
}

#[tokio::test]
#[serial]
async fn acct_change_normal_wallet_syncs_native_by_empty_token_when_token_missing() -> Result<()> {
    let _manager = get_manager().await;
    ensure_eth_chain_active().await?;
    let pool = core_pool()?;

    let address = "0x6F17DfC6a4E6B1f7A0A0eD3a4b2f1Bf49E2d0B73";
    let now = wallet_utils::time::now();

    let asset = CreateAssetsVo::new(
        AssetsId::new(address, "eth", Some(String::new()).into()),
        "ETH",
        18,
        None,
        0,
    )
    .with_name("Ethereum");
    AssetsDao::upsert_assets(pool.as_ref(), asset).await?;
    assert!(AssetsDao::get_by_addr_token(pool.as_ref(), "eth", "", address).await?.is_some());

    let msg_id = format!("normal-bug-native-{}", now.timestamp_millis());
    let payload = json!({
        "msgId": msg_id,
        "bizType": "ACCT_CHANGE",
        "body": {
            "blockHeight": 21342786,
            "chainCode": "eth",
            "fromAddr": "0x1111111111111111111111111111111111111111",
            "isMultisig": 0,
            "notes": "acct-change-native-token-missing",
            "queueId": "",
            "status": true,
            "symbol": "ether",
            "toAddr": address,
            "transactionFee": 0.00135940441821096,
            "transactionTime": "2026-03-12 10:13:48",
            "transferType": 0,
            "txHash": "0xbbb362dfd318f4da95e2d1e71c8c2a2ceabc8fd5df85e7c144843e6fc55f25e1",
            "txKind": 1,
            "value": 0.2223
        },
        "clientId": "7552bd49a9407eb98164c129d11da7e3",
        "deviceType": "IOS",
        "sn": "5bb0eada7cb7290b5d196362e6def48dcb9703e1468c0fb28eb7dd61073875e7",
        "walletType": "NORMAL_WALLET"
    });

    exec_wallet_order_payload(&payload).await?;
    let status = wait_task_done(&msg_id).await?;
    assert_eq!(status, 2, "AcctChange native task should succeed");

    let saved = AssetsDao::get_by_addr_token(pool.as_ref(), "eth", "", address).await?;
    assert!(saved.is_some());
    assert_eq!(saved.unwrap().symbol, "ETH");

    Ok(())
}
