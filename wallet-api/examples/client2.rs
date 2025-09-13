use tokio_stream::StreamExt as _;
use wallet_api::{FrontendNotifyEvent, WalletManager, test::env::get_manager};

// create wallet
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    wallet_utils::init_test_log();

    let (wallet_manager, test_params) = get_manager().await.unwrap();
    wallet_manager.set_invite_code(None).await;

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<FrontendNotifyEvent>();
    let mut rx = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);

    wallet_manager.set_frontend_notify_sender(tx).await?;

    if wallet_manager.init(test_params.device_req).await.code != 200 {
        tracing::error!("init failed");
        return Ok(());
    };

    // 创建钱包
    if true {
        let _wallet = wallet_manager.create_wallet(test_params.create_wallet_req).await.result;
    }

    while let Some(_data) = rx.next().await {
        tracing::info!("data: {_data:?}");
    }
    Ok(())
}

async fn _log_report() {
    let client_id = "test_data";
    // 获取项目根目录
    let storage_dir =
        std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join(client_id);

    let dirs = Dirs::new(&storage_dir.to_string_lossy()).unwrap();
    WalletManager::init_log(None, "66a7577a2b2f3b0130375e6f", &dirs, "9528").await.unwrap();
}
