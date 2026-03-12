use crate::get_manager;
use anyhow::Result;
use serde_json::json;
use wallet_api::test::mqtt::{core_pool, exec_wallet_order_payload, task_pool};
use wallet_database::{
    dao::assets::CreateAssetsVo,
    entities::assets::{AssetsEntity, AssetsId},
    repositories::task_queue::TaskQueueRepo,
};

async fn wait_task_done(msg_id: &str) -> Result<u8> {
    let pool = task_pool()?;
    for _ in 0..80 {
        if let Some(task) = TaskQueueRepo::task_detail(&pool, msg_id).await?
            && (task.status == 2 || task.status == 3)
        {
            return Ok(task.status);
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    anyhow::bail!("timeout waiting task status, msg_id={}", msg_id);
}

#[tokio::test]
async fn acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch() -> Result<()> {
    let _manager = get_manager().await;
    let pool = core_pool()?;

    let token = "0xdac17f958d2ee523a2206206994597c13d831ec7";
    let address = "0x148805B49819371EEF9A822f7F880b42Cf67834D";
    let now = wallet_utils::time::now();

    let asset = CreateAssetsVo::new(
        AssetsId::new(address, "eth", "USDT", Some(token.to_string())),
        6,
        None,
        0,
    )
    .with_name("Tether USD");
    AssetsEntity::upsert_assets(pool.as_ref(), asset).await?;

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

    let saved = AssetsEntity::get_by_addr_token(pool.as_ref(), "eth", token, address).await?;
    assert!(saved.is_some());
    assert_eq!(saved.unwrap().symbol, "USDT");

    Ok(())
}

#[tokio::test]
async fn acct_change_normal_wallet_syncs_native_by_empty_token_when_token_missing() -> Result<()> {
    let _manager = get_manager().await;
    let pool = core_pool()?;

    let address = "0x6F17DfC6a4E6B1f7A0A0eD3a4b2f1Bf49E2d0B73";
    let now = wallet_utils::time::now();

    let asset =
        CreateAssetsVo::new(AssetsId::new(address, "eth", "ETH", Some(String::new())), 18, None, 0)
            .with_name("Ethereum");
    AssetsEntity::upsert_assets(pool.as_ref(), asset).await?;

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

    let saved = AssetsEntity::get_by_addr_token(pool.as_ref(), "eth", "", address).await?;
    assert!(saved.is_some());
    assert_eq!(saved.unwrap().symbol, "ETH");

    Ok(())
}
