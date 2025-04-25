use tokio_stream::StreamExt as _;
use wallet_api::{test::env::get_manager, FrontendNotifyEvent};

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

    let wallet = wallet_manager
        .create_wallet(test_params.create_wallet_req)
        .await
        .result;
    tracing::warn!("wallet: {wallet:#?}");

    let sync_res = wallet_manager.sync_assets(vec![], None, vec![]).await;
    tracing::info!("sync res: {sync_res:#?}");
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
