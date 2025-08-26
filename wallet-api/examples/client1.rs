use tokio_stream::StreamExt as _;
use wallet_api::{FrontendNotifyEvent, test::env::get_manager};

// TFzMRRzQFhY9XFS37veoswLRuWLNtbyhiB

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    wallet_utils::init_test_log();
    // wallet_api::WalletManager::init_log(Some("warn"))
    //     .await
    //     .unwrap();
    // Self::init_log(Some("error")).await?;
    let (wallet_manager, test_params) = get_manager().await.unwrap();
    // wallet_api::WalletManager::init_log(Some("info"), "xxxx").await?;

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<FrontendNotifyEvent>();
    let mut rx = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);
    wallet_manager.set_frontend_notify_sender(tx).await?;

    if wallet_manager.init(test_params.device_req).await.code != 200 {
        tracing::error!("init failed");
        return Ok(());
    };

    let res = wallet_manager
        .set_invite_code(Some("I1912683353004912640".to_string()))
        .await;
    let res = wallet_utils::serde_func::serde_to_string(&res).unwrap();
    tracing::info!("res: {res:?}");

    // let wallet = wallet_manager
    //     .create_wallet(test_params.create_wallet_req)
    //     .await
    //     .result;
    // tracing::warn!("wallet: {wallet:#?}");

    // let topics = vec![
    //     "wallet/token/eth/usdc".to_string(),
    //     "wallet/token/tron/trx".to_string(),
    //     "wallet/token/doge/doge".to_string(),
    //     "wallet/token/tron/sun".to_string(),
    //     "wallet/token/tron/win".to_string(),
    //     "wallet/token/eth/hkby".to_string(),
    //     "wallet/token/btc/btc".to_string(),
    //     "wallet/token/eth/eth".to_string(),
    //     "wallet/token/bnb/bnb".to_string(),
    //     "wallet/token/sol/sol".to_string(),
    //     "wallet/token/ltc/ltc".to_string(),
    //     "wallet/token/eth/link".to_string(),
    //     "wallet/token/ton/ton".to_string(),
    //     "wallet/token/sui/sui".to_string(),
    //     "wallet/token/eth/cake".to_string(),
    //     "wallet/token/sol/usdt".to_string(),
    // ];
    // {
    //     wallet_manager.mqtt_subscribe(topics, None).await;
    // }

    // let sync_res = wallet_manager.sync_assets(vec![], None, vec![]).await;
    // tracing::info!("sync res: {sync_res:#?}");
    // let wallet = wallet.unwrap();
    // test_params.create_account_req.wallet_address = wallet.address.clone();

    // let config = wallet_manager.get_config().await;
    // tracing::info!("config result: {config:#?}");
    // let res = wallet_utils::serde_func::serde_to_string(&config)?;
    // tracing::info!("config result: {res}");
    while let Some(_data) = rx.next().await {
        tracing::info!("data: {_data:?}");
    }
    Ok(())
}
