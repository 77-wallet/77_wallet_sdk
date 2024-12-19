use crate::get_manager;
use wallet_api::request::stake::{FreezeBalanceReq, UnFreezeBalanceReq};

#[tokio::test]
async fn test_account_resource() {
    let manager = get_manager().await;
    let account = "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string();
    let res = manager.resource_info(account).await;

    tracing::info!("resource = {}", serde_json::to_string(&res).unwrap());
}

#[tokio::test]
async fn test_freeze() {
    let manager = get_manager().await;
    let req = FreezeBalanceReq {
        owner_address: "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string(),
        resource: "bandwidth".to_string(),
        frozen_balance: 50,
    };
    let password = "123456".to_string();

    let res = manager.freeze_balance(req, password).await;
    tracing::info!("freeze = {}", serde_json::to_string(&res).unwrap());
}

#[tokio::test]
async fn test_unfreeze() {
    let manager = get_manager().await;

    let req = UnFreezeBalanceReq {
        owner_address: "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string(),
        resource: "energy".to_string(),
        unfreeze_balance: 50,
    };

    let password = "123456".to_string();
    let res = manager.un_freeze_balance(req, password).await;

    tracing::info!("unfreeze  {}", serde_json::to_string(&res).unwrap());
}

#[tokio::test]
async fn test_freeze_list() {
    let manager = get_manager().await;

    let owner = "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string();
    let res = manager.freeze_list(owner).await;
    tracing::info!("freeze  {}", serde_json::to_string(&res).unwrap());
}

#[tokio::test]
async fn test_un_freeze_list() {
    let manager = get_manager().await;

    let owner = "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string();
    let res = manager.un_freeze_list(owner).await;
    tracing::info!("unfreeze = {}", serde_json::to_string(&res).unwrap());
}

#[tokio::test]
async fn test_cancel_all_unfreeze() {
    let manager = get_manager().await;

    let owner = "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string();
    let password = "123456".to_string();

    let res = manager.cancel_all_unfreeze(owner, password).await;
    tracing::info!("unfreeze {}", serde_json::to_string(&res).unwrap());
}

#[tokio::test]
async fn test_withdraw() {
    let manager = get_manager().await;

    let owner_address = "TZ92GD6UbW8MMk6XD6pxKTGzUGs42No6vn".to_string();
    let password = "123456".to_string();

    let res = manager.withdraw_unfreeze(owner_address, password).await;
    tracing::info!("withdraw {}", serde_json::to_string(&res).unwrap());
}

#[tokio::test]
async fn test_get_estimate_resource() {
    let manager = get_manager().await;

    let account = "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string();
    let value = 50;
    let resource_type = "bandwidth".to_string();

    let res = manager
        .get_estimated_resources(account, value, resource_type)
        .await;
    tracing::info!(" response = {}", serde_json::to_string(&res).unwrap());
}

#[tokio::test]
async fn test_system_resource() {
    let manager = get_manager().await;

    let address = "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string();
    let res = manager.system_resource(address).await;
    tracing::info!("system resource = {}", serde_json::to_string(&res).unwrap());
}

#[tokio::test]
async fn test_request_energy() {
    let manager = get_manager().await;

    let address = "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string();
    let energy = 50;

    let res = manager.request_energy(address, energy).await;
    tracing::info!("request {}", serde_json::to_string(&res).unwrap());
}
