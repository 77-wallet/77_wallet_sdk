use crate::get_manager;
use wallet_api::{
    domain,
    request::permission::{KeysReq, PermissionReq},
};

// 权限列表
#[tokio::test]
async fn test_permission_list() {
    let wallet_manager = get_manager().await;

    let res = wallet_manager.permission_list();

    tracing::info!("{}", serde_json::to_string(&res).unwrap())
}

// 账户权限
#[tokio::test]
async fn test_permission_accounts() {
    let wallet_manager = get_manager().await;

    let address = "TFtvHtfuLo5xJJe9HpSAEaEi4bzT8Eeyu2".to_string();
    let res = wallet_manager.account_permission(address).await;

    tracing::info!("{}", serde_json::to_string(&res).unwrap())
}

// manager permission
#[tokio::test]
async fn test_manager_permission() {
    let wallet_manager = get_manager().await;
    let address = "TKDDywzwyYJD8n1BMy5cqr7cxjEFaKJ8h3".to_string();
    let res = wallet_manager.manager_permission(address).await;
    tracing::info!("{}", serde_json::to_string(&res).unwrap())
}

// 新增权限手续费
#[tokio::test]
async fn test_add_permission_fee() {
    let wallet_manager = get_manager().await;

    let keys = vec![
        KeysReq {
            address: "TJkMavCTA2qd7TLzWehtMnKnxkSeaWAdcq".to_string(),
            weight: 1,
        },
        KeysReq {
            address: "TE4xhjv6dvEbYxXGjP4ntnN3viSN9Nf8Qv".to_string(),
            weight: 116,
        },
        KeysReq {
            address: "TQsWaoYYwZ4EVj9wgDG4bfdjwYYRejQsTC".to_string(),
            weight: 14,
        },
    ];

    let req = PermissionReq {
        grantor_addr: "TQsWaoYYwZ4EVj9wgDG4bfdjwYYRejQsTC".to_string(),
        name: "转账、质押,12".to_string(),
        active_id: Some(8),
        threshold: 1,
        operations: vec![1, 2, 3, 5],
        keys,
    };

    let res = wallet_manager
        .modify_permission_fee(req, "update".to_string())
        .await;

    tracing::info!("{}", serde_json::to_string(&res).unwrap())
}

// 新增权限
#[tokio::test]
async fn test_add_permission() {
    let wallet_manager = get_manager().await;

    let keys = vec![KeysReq {
        address: "TWtoyV1B5z33PNU5BGzAMgcu2NQzctbgSv".to_string(),
        weight: 1,
    }];

    let req = PermissionReq {
        grantor_addr: "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string(),
        name: "picker".to_string(),
        active_id: None,
        threshold: 1,
        operations: vec![0, 2, 5, 12, 16, 48, 58],
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
async fn test_up_permission() {
    let wallet_manager = get_manager().await;

    let keys = vec![
        KeysReq {
            address: "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string(),
            weight: 1,
        },
        KeysReq {
            address: "TKDDywzwyYJD8n1BMy5cqr7cxjEFaKJ8h3".to_string(),
            weight: 1,
        },
        KeysReq {
            address: "TNcRALWJNRtM5zfLQFvbiuycue9ZcxFFjQ".to_string(),
            weight: 1,
        },
    ];

    let req = PermissionReq {
        grantor_addr: "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string(),
        name: "update name".to_string(),
        active_id: Some(2),
        threshold: 2,
        operations: vec![1, 54, 55, 59, 56, 57, 58],
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
        active_id: Some(4),
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
            address: "TDEmB5MnTQBSm6LLtMsuG4RJ8ajZsoacbG".to_string(),
            weight: 1,
        },
        KeysReq {
            address: "TRkW3hXjPsMMeksVPeAYg3rw5egSdA49uZ".to_string(),
            weight: 1,
        },
        KeysReq {
            address: "TTbAVnrXbhrMGXYxPH9sfJYpYFujn1QBJf".to_string(),
            weight: 1,
        },
        KeysReq {
            address: "TYD6wPezLZKAqHEH5SexzX9kAocYibB3er".to_string(),
            weight: 1,
        },
    ];

    let req = PermissionReq {
        grantor_addr: "TTbAVnrXbhrMGXYxPH9sfJYpYFujn1QBJf".to_string(),
        name: "stake".to_string(),
        active_id: Some(2),
        threshold: 4,
        operations: vec![1, 54, 55, 59, 56, 57, 58],
        keys,
    };
    let password = "123456".to_string();
    let expiration = 5;

    let res = wallet_manager
        .build_multisig_queue(req, "update".to_string(), password, expiration)
        .await;

    tracing::info!("{}", serde_json::to_string(&res).unwrap());
}

#[tokio::test]
async fn test_recover_data() {
    let _wallet_manager = get_manager().await;

    let uids = vec!["TL55jNbXWeM6se5fpKBQTTmH45HZ7stvW3".to_string()];
    domain::permission::PermissionDomain::recover_permission(uids)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_backup() {
    let _wallet_manager = get_manager().await;

    tokio::time::sleep(tokio::time::Duration::from_secs(4)).await;
    tracing::info!("xxx")
}
