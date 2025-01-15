use crate::get_manager;

#[tokio::test]
async fn test_config_list() {
    let wallet_manager = get_manager().await;

    let configs = wallet_manager.get_configs().await;

    tracing::info!("{}", serde_json::to_string(&configs).unwrap());
}

#[tokio::test]
async fn test_set_min_value_config() {
    let wallet_manager = get_manager().await;

    let symbol = "TRx".to_string();
    let value = 49.0;
    let switch = true;

    let configs = wallet_manager
        .set_min_value_config(symbol, value, switch)
        .await;
    tracing::info!("{}", serde_json::to_string(&configs).unwrap());
}

#[tokio::test]
async fn test_get_min_value_config() {
    let wallet_manager = get_manager().await;

    let symbol = "dai".to_string();

    let configs = wallet_manager.get_min_value_config(symbol).await;
    tracing::info!("{}", serde_json::to_string(&configs).unwrap());
}

// #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
// async fn test_app_token() {
//     let wallet_manager = get_manager().await;

//     for _ in 0..5 {
//         tokio::time::sleep(std::time::Duration::from_secs(4)).await;

//         match Context::get_rpc_header().await {
//             Ok(c) => match serde_json::to_string(&c) {
//                 Ok(json) => tracing::info!("{}", json),
//                 Err(e) => tracing::error!("Serialization error: {:?}", e),
//             },
//             Err(e) => tracing::error!("Error getting RPC header: {:?}", e),
//         }
//     }
// }
