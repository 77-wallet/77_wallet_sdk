use tokio_stream::StreamExt as _;
use wallet_api::{notify::FrontendNotifyEvent, test::env::get_manager};

// create wallet
#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    wallet_utils::init_test_log();

    let (wallet_manager, test_params) = get_manager().await.unwrap();

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

    // 创建钱包
    // let _wallet = wallet_manager
    //     .create_wallet(test_params.create_wallet_req)
    //     .await
    //     .result;

    while let Some(_data) = rx.next().await {
        tracing::info!("data: {_data:?}");
    }
    Ok(())
}
