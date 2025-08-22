use std::{env, path::PathBuf, time::Duration};
use tokio::time::interval;
use wallet_api::{CustomEventFormat, LogBasePath, init_logger, start_upload_scheduler};

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let log = dir.join("test_data/log");
    let base_path = LogBasePath(log);

    let format = CustomEventFormat::new("66a7577a2b2f3b0130375e6f".to_string(), "9528".to_string());
    init_logger(format, base_path.clone(), "info").unwrap();

    tokio::spawn(generate_log());

    let config =
        wallet_api::config::Config::new(&wallet_api::test::env::get_config().unwrap()).unwrap();
    let oss_client = wallet_oss::oss_client::OssClient::new(&config.oss);

    println!("bucket_name: {}", config.oss.bucket_name);
    let _c = start_upload_scheduler(base_path, 20, oss_client).await;

    loop {}
}

async fn generate_log() {
    let mut interval = interval(Duration::from_secs(1));
    let mut counter = 0;

    loop {
        interval.tick().await;
        tracing::warn!("test567 {}", counter);
        counter += 1;
    }
}
