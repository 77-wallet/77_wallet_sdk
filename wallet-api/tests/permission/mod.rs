use crate::get_manager;
use wallet_api::{
    domain,
    request::permission::{KeysReq, PermissionReq},
};

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

// manager permission
#[tokio::test]
async fn test_manager_permission() {
    let wallet_manager = get_manager().await;
    let res = wallet_manager.manager_permission().await;
    tracing::info!("{}", serde_json::to_string(&res).unwrap())
}

// 新增权限手续费
#[tokio::test]
async fn test_add_permission_fee() {
    let wallet_manager = get_manager().await;

    let keys = vec![KeysReq {
        address: "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string(),
        weight: 1,
    }];

    let req = PermissionReq {
        grantor_addr: "TUe3T6ErJvnoHMQwVrqK246MWeuCEBbyuR".to_string(),
        name: "转账、质押".to_string(),
        active_id: None,
        threshold: 1,
        operations: vec![1, 2, 3, 5],
        keys,
    };

    let res = wallet_manager
        .modify_permission_fee(req, "new".to_string())
        .await;

    tracing::info!("{}", serde_json::to_string(&res).unwrap())
}

// 新增权限
#[tokio::test]
async fn test_add_permission() {
    let wallet_manager = get_manager().await;

    let keys = vec![KeysReq {
        address: "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string(),
        weight: 1,
    }];

    let req = PermissionReq {
        grantor_addr: "TUe3T6ErJvnoHMQwVrqK246MWeuCEBbyuR".to_string(),
        name: "新增权限".to_string(),
        active_id: None,
        threshold: 1,
        operations: vec![1, 2, 3, 6],
        keys,
    };
    let password = "123456".to_string();

    let res = wallet_manager
        .modify_permission(req, "new".to_string(), password)
        .await;

    tracing::info!("{}", serde_json::to_string(&res).unwrap())
}

// 修改权限
#[tokio::test]
async fn test_up_permision() {
    let wallet_manager = get_manager().await;

    let keys = vec![
        KeysReq {
            address: "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string(),
            weight: 1,
        },
        KeysReq {
            address: "TNPTj8Dbba6YxW5Za6tFh6SJMZGbUyucXQ".to_string(),
            weight: 1,
        },
        // KeysReq {
        //     address: "TDVayp1uF6CD3NGT1ZR4SJcxot5VQHQNtY".to_string(),
        //     weight: 1,
        // },
    ];

    let req = PermissionReq {
        grantor_addr: "TUe3T6ErJvnoHMQwVrqK246MWeuCEBbyuR".to_string(),
        name: "final".to_string(),
        active_id: Some(2),
        threshold: 2,
        operations: vec![1, 54, 55],
        keys,
    };
    let password = "123456".to_string();

    let res = wallet_manager
        .modify_permission(req, "update".to_string(), password)
        .await;

    tracing::info!("{}", serde_json::to_string(&res).unwrap())
}

// 删除权限
#[tokio::test]
async fn test_del_permission() {
    let wallet_manager = get_manager().await;

    let keys = vec![KeysReq {
        address: "TNPTj8Dbba6YxW5Za6tFh6SJMZGbUyucXQ".to_string(),
        weight: 1,
    }];

    let req = PermissionReq {
        grantor_addr: "TUe3T6ErJvnoHMQwVrqK246MWeuCEBbyuR".to_string(),
        name: "修改权限".to_string(),
        active_id: Some(2),
        threshold: 1,
        operations: vec![1],
        keys,
    };
    let password = "123456".to_string();

    let res = wallet_manager
        .modify_permission(req, "delete".to_string(), password)
        .await;

    tracing::info!("{}", serde_json::to_string(&res).unwrap())
}

#[tokio::test]
async fn test_build_multisig_queue() {
    let wallet_manager = get_manager().await;

    let keys = vec![
        KeysReq {
            address: "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string(),
            weight: 1,
        },
        KeysReq {
            address: "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string(),
            weight: 1,
        },
    ];

    let req = PermissionReq {
        grantor_addr: "TNPTj8Dbba6YxW5Za6tFh6SJMZGbUyucXQ".to_string(),
        name: "new_permission".to_string(),
        active_id: None,
        threshold: 1,
        operations: vec![1],
        keys,
    };
    let password = "123456".to_string();
    let expiration = 5;

    let res = wallet_manager
        .build_multisig_queue(req, "new".to_string(), password, expiration)
        .await;

    tracing::info!("{}", serde_json::to_string(&res).unwrap());

    loop {}
}

#[tokio::test]
async fn test_recover_data() {
    let _wallet_manager = get_manager().await;

    let uids = vec!["137eb624118a0224f491d94f153c2ad3b6e55661dbf687d8a8ba8c59aa7ab358".to_string()];
    domain::permission::PermissionDomain::recover_permission(uids)
        .await
        .unwrap();
}
