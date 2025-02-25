use crate::get_manager;

// 权限列表
#[tokio::test]
async fn test_permssion_list() {
    let wallet_manager = get_manager().await;

    let res = wallet_manager.permission_list();

    tracing::info!("{}", serde_json::to_string(&res).unwrap())
}

// 权限列表
#[tokio::test]
async fn test_permssion_trans() {
    let wallet_manager = get_manager().await;

    let res = wallet_manager.permssion_trans();

    tracing::info!("{}", serde_json::to_string(&res).unwrap())
}

// 账户权限
#[tokio::test]
async fn test_permssion_accounts() {
    let wallet_manager = get_manager().await;

    let address = "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string();
    let res = wallet_manager.account_permission(address).await;

    tracing::info!("{}", serde_json::to_string(&res).unwrap())
}
