use crate::get_manager;
use wallet_api::request::permission::{KeysReq, PermissionReq};

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

    let address = "TUe3T6ErJvnoHMQwVrqK246MWeuCEBbyuR".to_string();
    let res = wallet_manager.account_permission(address).await;

    tracing::info!("{}", serde_json::to_string(&res).unwrap())
}

// 新增权限
#[tokio::test]
async fn test_add_permission() {
    let wallet_manager = get_manager().await;

    let keys = vec![KeysReq {
        address: "TNPTj8Dbba6YxW5Za6tFh6SJMZGbUyucXQ".to_string(),
        weight: 1,
    }];

    let req = PermissionReq {
        address: "TUe3T6ErJvnoHMQwVrqK246MWeuCEBbyuR".to_string(),
        name: "测试".to_string(),
        permission_id: None,
        threshold: 1,
        operations: vec![1, 2],
        keys,
    };
    let password = "123456".to_string();

    let res = wallet_manager.add_permission(req, password).await;

    tracing::info!("{}", serde_json::to_string(&res).unwrap())
}

// 修改权限
#[tokio::test]
async fn test_up_permision() {
    let wallet_manager = get_manager().await;

    let keys = vec![KeysReq {
        address: "TNPTj8Dbba6YxW5Za6tFh6SJMZGbUyucXQ".to_string(),
        weight: 1,
    }];

    let req = PermissionReq {
        address: "TUe3T6ErJvnoHMQwVrqK246MWeuCEBbyuR".to_string(),
        name: "修改权限".to_string(),
        permission_id: Some(3),
        threshold: 1,
        operations: vec![1],
        keys,
    };
    let password = "123456".to_string();

    let res = wallet_manager.up_permission(req, password).await;

    tracing::info!("{}", serde_json::to_string(&res).unwrap())
}

// 删除权限
#[tokio::test]
async fn test_del_permission() {
    let wallet_manager = get_manager().await;

    let address = "TUe3T6ErJvnoHMQwVrqK246MWeuCEBbyuR".to_string();
    let permission_id = 3;
    let password = "123456".to_string();

    let res = wallet_manager
        .del_permission(address, permission_id, password)
        .await;

    tracing::info!("{}", serde_json::to_string(&res).unwrap())
}
