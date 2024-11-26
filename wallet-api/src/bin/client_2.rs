use std::{env, path::PathBuf};
use tokio_stream::StreamExt as _;
use wallet_api::{notify::FrontendNotifyEvent, InitDeviceReq, WalletManager};
use wallet_utils::init_test_log;

// create wallet
#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = get_manager().await;

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<FrontendNotifyEvent>();
    let mut rx = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);

    // 启动检查任务
    let _ = manager.init_data().await;

    // 设置通知前端的流
    manager.set_frontend_notify_sender(tx).await?;

    // 初始化设备并启动mqtt
    manager.init_device(device_req()).await;

    create_wallet(&manager, true).await;

    while let Some(_data) = rx.next().await {
        // tracing::info!("data: {data:?}");
    }
    Ok(())
}

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

async fn create_wallet(manager: &WalletManager, create: bool) {
    if create {
        let phrase = "victory member rely dirt treat woman boring tomato two hollow erosion drop";
        // let phrase =
        //     "will match face problem tongue fortune rebuild stool moon assist virtual lounge";

        // let phrase =
        //     "nose bird celery bread slice hero black session tonight winner pitch foot".to_string();

        // let phrase =
        //     "embrace still summer two neglect lawsuit museum captain reward bronze dish curve";

        // let phrase =
        //     "task mammal cinnamon nuclear old meat giraffe fringe concert imitate lyrics lady";

        let salt = "";
        // let salt = "1234qwer";
        manager
            .create_wallet(1, phrase, salt, "test", "123456", None)
            .await;
    }
}

fn device_req() -> InitDeviceReq {
    let device_type = "ANDROID";
    let sn = "guangxiang";

    InitDeviceReq {
        device_type: device_type.to_string(),
        sn: sn.to_string(),
        code: device_type.to_string(),
        system_ver: device_type.to_string(),
        iemi: Some(device_type.to_string()),
        meid: Some(device_type.to_string()),
        iccid: Some(device_type.to_string()),
        mem: Some(device_type.to_string()),
        app_id: None,
        package_id: None,
    }
}
