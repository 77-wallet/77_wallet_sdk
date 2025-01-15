use std::{env, path::PathBuf};
use tokio_stream::StreamExt;
use wallet_api::{notify::FrontendNotifyEvent, WalletManager};
use wallet_utils::init_test_log;

mod account;
mod address_book;
mod bill;
mod config;
mod multisig_account;
mod multisig_tx;
mod phrase;
mod stake;

pub async fn get_manager() -> WalletManager {
    init_test_log();
    let path = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("test_data")
        .to_string_lossy()
        .to_string();

    // let sender = Some();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<FrontendNotifyEvent>();
    let mut rx = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);

    tokio::spawn(async move {
        while let Some(_data) = rx.next().await {
            tracing::info!("data: {_data:?}");
        }
    });

    let config = wallet_api::Config::new(&wallet_api::test::env::get_config().unwrap()).unwrap();
    WalletManager::new("guangxiang", "ANDROID", &path, Some(tx), config)
        .await
        .unwrap()
}
