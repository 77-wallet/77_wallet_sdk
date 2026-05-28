#![allow(dead_code)]
#![allow(unused)]

use std::sync::Arc;

use tokio_stream::StreamExt as _;
use wallet_api::{messaging::notify::FrontendNotifyEvent, testkit::env::get_manager_with_config};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (wallet_manager, _test_params) = get_manager_with_config("client4.toml").await?;
    let dirs = wallet_api::get_context()?.get_global_dirs();
    let _ = wallet_api::xlog::init_log(Some("info"), &"app_code", &dirs, "sn_client4").await;
    tracing::info!("[CLIENT4] init_api_swap");
    wallet_manager.init_api_swap().await?;
    let wallet_password = "q1111111";

    let _ = wallet_manager.set_passwd_cache(wallet_password).await;
    tracing::info!("[CLIENT4] set_frontend_notify_sender");
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<FrontendNotifyEvent>();
    let mut rx = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);
    wallet_manager.set_frontend_notify_sender(tx).await?;
    tracing::info!("[CLIENT4] set_invite_code");
    let res = wallet_manager.set_invite_code(Some("I1912683353004912640".to_string())).await?;
    let res = wallet_utils::serde_func::serde_to_string(&res).unwrap();
    tracing::info!("[CLIENT4] set_invite_code res: {res}");

    loop {
        tokio::select! {
            msg = rx.next() => {
                let data = serde_json::to_string(&msg).unwrap();
                tracing::info!("[CLIENT4] 前端收到数据: {data:?}");
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("[CLIENT4] ctrl_c");
                let _ = wallet_manager.close().await;
                break;
            }
        }
    }
    Ok(())
}
