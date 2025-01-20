use tokio_stream::StreamExt as _;
use wallet_api::{notify::FrontendNotifyEvent, test::env::get_manager};

// TFzMRRzQFhY9XFS37veoswLRuWLNtbyhiB

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    wallet_utils::init_test_log();
    // wallet_api::WalletManager::init_log(Some("warn"))
    //     .await
    //     .unwrap();
    // Self::init_log(Some("error")).await?;
    let (wallet_manager, mut test_params) = get_manager().await.unwrap();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<FrontendNotifyEvent>();
    let mut rx = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);

    wallet_manager.set_frontend_notify_sender(tx).await?;

    if wallet_manager
        .init_device(test_params.device_req)
        .await
        .code
        != 200
    {
        return Ok(());
    };
    wallet_manager.init_data().await?;

    let wallet = wallet_manager
        .create_wallet(test_params.create_wallet_req)
        .await
        .result;
    tracing::warn!("wallet: {wallet:#?}");
    let wallet = wallet.unwrap();
    test_params.create_account_req.wallet_address = wallet.address.clone();
    wallet_manager
        .create_account(test_params.create_account_req)
        .await
        .result;
    // let _c = wallet_manager.sync_assets(vec![], None, vec![]).await;
    // wallet_manager.recover_multisig_data(&wallet.address).await;
    // wallet_manager.set_language("CHINESE_SIMPLIFIED").await;
    let config = wallet_manager.get_config().await;
    tracing::info!("config result: {config:#?}");
    let res = wallet_utils::serde_func::serde_to_string(&config)?;
    tracing::info!("config result: {res}");
    while let Some(_data) = rx.next().await {
        tracing::info!("data: {_data:?}");
    }
    Ok(())
}
