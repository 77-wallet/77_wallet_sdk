use crate::get_manager;
use anyhow::Result;
use serde_json::json;
use wallet_api::test::mqtt::{api_wallet_pool, exec_wallet_order_payload, task_pool};
use wallet_database::{
    entities::{
        api_assets::ApiCreateAssetsVo,
        api_coin::ApiCoinData,
        assets::{AssetsId, AssetsIdVo},
    },
    repositories::{
        api_wallet::{assets::ApiAssetsRepo, coin::ApiCoinRepo},
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

#[tokio::test]
async fn acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address() -> Result<()> {
    let _manager = get_manager().await;
    let api_pool = api_wallet_pool()?;

    let token = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
    let address = "3jVrVbEPDd35piQUxur1Gki8bkz4XkhZTXZHmfSnmHEd";
    let now = wallet_utils::time::now();

    let coin = ApiCoinData::new(
        Some("USD Coin".to_string()),
        "USDC",
        "sol",
        Some(token.to_string()),
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
        AssetsId::new(address, "sol", "USDC", Some(token.to_string())),
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
        &AssetsIdVo::new(address, "sol", Some(token.to_string())),
    )
    .await?;
    assert!(saved.is_some());
    assert_eq!(saved.unwrap().symbol, "USDC");

    Ok(())
}
