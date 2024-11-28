use wallet_api::request::stake::{DelegateReq, FreezeBalanceReq, UnFreezeBalanceReq};

use crate::get_manager;

#[tokio::test]
async fn test_account_resource() {
    let manager = get_manager().await;
    let account = "TQHq9gP34tLiE2Eg1MeAQjhN6KA6oLRBos".to_string();
    let res = manager.resource_info(account).await;

    tracing::info!(
        "account resource = {:?}",
        serde_json::to_string(&res).unwrap()
    );
}

#[tokio::test]
async fn test_freeze() {
    let manager = get_manager().await;
    let req = FreezeBalanceReq {
        owner_address: "TGyw6wH5UT5GVY5v6MTWedabScAwF4gffQ".to_string(),
        resource: "bandwidth".to_string(),
        frozen_balance: "100".to_string(),
    };
    let password = "123456".to_string();

    let res = manager.freeze_balance(req, password).await;
    tracing::info!(
        "freeze response = {:?}",
        serde_json::to_string(&res).unwrap()
    );
}

#[tokio::test]
async fn test_unfreeze() {
    let manager = get_manager().await;

    let req = UnFreezeBalanceReq {
        owner_address: "TZ92GD6UbW8MMk6XD6pxKTGzUGs42No6vn".to_string(),
        resource: "bandwidth".to_string(),
        unfreeze_balance: "100".to_string(),
    };

    let password = "123456".to_string();
    let res = manager.un_freeze_balance(req, password).await;

    tracing::info!(
        "unfreeze response = {:?}",
        serde_json::to_string(&res).unwrap()
    );
}

#[tokio::test]
async fn test_freeze_list() {
    let manager = get_manager().await;

    let owner = "TZ92GD6UbW8MMk6XD6pxKTGzUGs42No6vn".to_string();
    let resource = "bandwidth".to_string();
    let res = manager.freeze_list(owner, resource, 0, 10).await;
    tracing::info!(
        "freeze list response = {:?}",
        serde_json::to_string(&res).unwrap()
    );
}

#[tokio::test]
async fn test_withdraw() {
    let manager = get_manager().await;

    let owner_address = "TZ92GD6UbW8MMk6XD6pxKTGzUGs42No6vn".to_string();
    let password = "123456".to_string();

    let res = manager.withdraw_unfreeze(owner_address, password).await;
    tracing::info!(
        "withdraw response = {:?}",
        serde_json::to_string(&res).unwrap()
    );
}

#[tokio::test]
async fn test_delegate() {
    let manager = get_manager().await;

    let req = DelegateReq {
        owner_address: "TGyw6wH5UT5GVY5v6MTWedabScAwF4gffQ".to_string(),
        receiver_address: "TZ92GD6UbW8MMk6XD6pxKTGzUGs42No6vn".to_string(),
        balance: "10".to_string(),
        resource: "bandwidth".to_string(),
        lock: false,
        lock_period: 0,
    };
    let password = "123456".to_string();
    let res = manager.delegate_resource(req, password).await;

    tracing::info!(
        "delegate response = {:?}",
        serde_json::to_string(&res).unwrap()
    );
}

#[tokio::test]
async fn test_delegate_list() {
    let manager = get_manager().await;

    let owner_address = "TGyw6wH5UT5GVY5v6MTWedabScAwF4gffQ".to_string();
    let resource_type = "bandwidth".to_string();
    let res = manager
        .delegate_list(owner_address, resource_type, 0, 10)
        .await;

    tracing::info!(
        "delegate list response = {:?}",
        serde_json::to_string(&res).unwrap()
    );
}

#[tokio::test]
async fn test_un_delegate() {
    let manager = get_manager().await;
    let id = "167436918328528896".to_string();
    let password = "123456".to_string();
    let res = manager.un_delegate_resource(id, password).await;

    tracing::info!(
        "un delegate response = {:?}",
        serde_json::to_string(&res).unwrap()
    );
}

#[tokio::test]
async fn test_request_resource() {
    let manager = get_manager().await;

    let address = "TQHq9gP34tLiE2Eg1MeAQjhN6KA6oLRBos".to_string();
    let bandwidth = 267;
    let energy = 0;
    let value = "13.602".to_string();
    let to = "TNPTj8Dbba6YxW5Za6tFh6SJMZGbUyucXQ".to_string();
    let symbol = "trx".to_string();

    let res = manager
        .request_resource(address, energy, bandwidth, value, symbol, to)
        .await;

    tracing::info!(
        "request resource response = {:?}",
        serde_json::to_string(&res).unwrap()
    );
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
    let energy = 1000;

    let res = manager.request_energy(address, energy).await;

    tracing::info!(
        "request resource = {}",
        serde_json::to_string(&res).unwrap()
    );
}
