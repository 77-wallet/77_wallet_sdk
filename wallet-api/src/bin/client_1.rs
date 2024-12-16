use tokio_stream::StreamExt as _;
use wallet_api::{
    notify::FrontendNotifyEvent,
    test::env::{setup_test_environment, TestData, TestWalletEnv},
    InitDeviceReq,
};

// TFzMRRzQFhY9XFS37veoswLRuWLNtbyhiB

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // std::env::set_var("RUST_BACKTRACE", "1");
    wallet_utils::init_test_log();

    let phrase = Some(
        "chuckle practice chicken permit swarm giant improve absurd melt kitchen oppose scrub"
            .to_string(),
    );
    // let phrase = Some(
    //     "arrest hover fury mercy slim answer hospital area morning student riot deal".to_string(),
    // );
    // let phrase = Some(
    //     "spoil first width hat submit inflict impact quantum love funny warrior spike".to_string(),
    // );
    // let phrase = Some(
    //     "fetch bronze forward wish only gentle picture noise vocal essay devote steel".to_string(),
    // );
    // let phrase = Some(
    //     "will match face problem tongue fortune rebuild stool moon assist virtual lounge"
    //         .to_string(),
    // );
    // let phrase = Some(
    //     "drum planet ugly present absorb chair simple shiver honey object captain unable"
    //         .to_string(),
    // );
    // let phrase = Some(
    //     "loan tiny planet lucky rigid clip coil recall praise obvious debris dilemma".to_string(),
    // );
    let phrase = Some(
        "divorce word join around degree mother quiz math just custom lunar angle".to_string(),
    );
    let phrase = Some(
        "often insect unknown ignore chronic dumb grow express plug purpose enhance glad"
            .to_string(),
    );
    // let phrase = Some(
    //     "pave sphere only enhance long between finger pudding undo escape avoid avoid".to_string(),
    // );
    // let phrase = Some(
    //     "nose bird celery bread slice hero black session tonight winner pitch foot".to_string(),
    // );
    // let phrase =
    //     Some("fan swamp loop mesh enact tennis priority artefact canal hour skull joy".to_string());
    // let phrase = Some(
    //     "will match face problem tongue fortune rebuild stool moon assist virtual lounge"
    //         .to_string(),
    // );
    // drum planet drum present absorb chair simple shiver honey object captain unable
    // let phrase = None;
    // let phrase = Some(
    //     "embrace still summer two neglect lawsuit museum captain reward bronze dish curve"
    //         .to_string(),
    // );

    let TestData {
        wallet_manager,
        wallet_env: env,
        ..
    } = setup_test_environment(None, None, false, phrase)
        .await
        .unwrap();
    // wallet_api::WalletManager::init_log(Some("warn"))
    //     .await
    //     .unwrap();
    // Self::init_log(Some("error")).await?;

    let TestWalletEnv {
        language_code,
        phrase,
        salt,
        wallet_name,
        password,
    } = env;

    let device_type = "ANDROID";
    // let sn = "wenjing";
    // let sn = "104.2.0.125C00";
    // let sn = "3f76bd432e027aa97d11f2c3f5092bee195991be461486f0466eec9d46940e9e";
    // let sn = "3b2d76e3495e411963e0d0232fbcbc68b0298ca8c6cbaef00d2abb172f28c370";
    // let sn = "9580b55ec4a1d3d3af85077ae0c4c901885b1123e50f830cbd5bfbbe0cb161a3";
    // let sn = "bdb6412a9cb4b12c48ebe1ef4e9f052b07af519b7485cd38a95f38d89df97cb8";
    let sn = "ebe42b137abb313f0d0012f588080395c3742e7eac77e60f43fac0afb363e67c";

    // let client_id = "wenjing";

    // let mqtt_url = "ws://100.106.144.126:8083/mqtt";
    let req = InitDeviceReq {
        device_type: device_type.to_string(),
        sn: sn.to_string(),
        code: "sdk_gphone64_arm64".to_string(),
        system_ver: "15_12228598".to_string(),
        iemi: None,
        meid: None,
        iccid: None,
        mem: None,
        app_id: Some("13065ffa4e8f6958bd6".to_string()),
        package_id: None,
    };

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<FrontendNotifyEvent>();
    let mut rx = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);

    wallet_manager.set_frontend_notify_sender(tx).await?;

    let _res = wallet_manager.init_device(req).await;
    let init_res = wallet_manager.init_data().await;
    // tracing::info!("init_data res: {init_res:?}");

    // tracing::info!("init_device res: {_res:?}");

    // tracing::info!("start create wallet");

    // let account_name = "账户";
    // let start_time = std::time::Instant::now();

    // let _res = wallet_manager
    //     .create_wallet(
    //         language_code,
    //         &phrase,
    //         &salt,
    //         &wallet_name,
    //         account_name,
    //         true,
    //         &password,
    //         None,
    //     )
    //     .await
    //     .result
    //     .unwrap();
    // tracing::info!("create_wallet res: {_res:?}");
    // let elapsed_time = start_time.elapsed();
    // tracing::info!("create_wallet elapsed time: {:?}", elapsed_time);
    // wallet_manager
    //     .create_account(
    //         &_res.address,
    //         &password,
    //         None,
    //         None,
    //         None,
    //         account_name,
    //         true,
    //     )
    //     .await
    //     .result
    //     .unwrap();
    // tracing::info!("create_account res: {_res:?}");
    // let _c = wallet_manager.sync_assets(vec![], None, vec![]).await;

    // wallet_manager
    //     .mqtt_subscribe(vec!["wallet/token/btc/btc".to_string()], None)
    //     .await;

    // tokio::spawn(async move {
    //     tokio::time::sleep(std::time::Duration::from_secs(60)).await;

    //     let unsubscribe = wallet_api::service::wallet::WalletService::new(repo)
    //         .reset()
    //         .await
    //         .unwrap();
    //     let unsubscribe = wallet_manager
    //         .upload_log_file()
    //         .await;
    //     let unsubscribe = wallet_manager
    //         .mqtt_unsubscribe(vec!["wallet/token/eth/eth".to_string()])
    //         .await;
    //     tracing::info!("unsubscribe: {unsubscribe:?}");
    //     let unsubscribe = wallet_manager
    //         .mqtt_unsubscribe(vec!["wallet/token/eth/eth".to_string()])
    //         .await;
    //     tracing::info!("unsubscribe: {unsubscribe:?}");
    // });

    wallet_manager.set_language("CHINESE_SIMPLIFIED").await;

    // let config = wallet_manager.get_config().await;

    // tracing::info!("config: {config:#?}");
    while let Some(data) = rx.next().await {
        tracing::info!("data: {data:?}");
    }
    // loop {
    //     println!("sleep ");
    //     tokio::time::sleep(std::time::Duration::from_secs(10)).await
    // }
    Ok(())
}
