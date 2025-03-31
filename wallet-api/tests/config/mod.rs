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
    let value = 1.0;
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

#[tokio::test]
async fn test_global_msg() {
    let wallet_manager = get_manager().await;

    let res = wallet_manager.global_msg().await;

    tracing::info!("{}", serde_json::to_string(&res).unwrap());
}

#[tokio::test]
async fn test_request_invite_summary() {
    let wallet_manager = get_manager().await;

    let endpoint = "invite/summary".to_string();
    let body =
        r#"{"uid":"573648e4144e13848374bc68bdea1c5f862d94ba0eb8b48fdc1461cd25d3fe2b"}"#.to_string();

    let res = wallet_manager.request(endpoint, body).await;

    tracing::info!("{}", serde_json::to_string(&res).unwrap());
}
