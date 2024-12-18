use crate::get_manager;
use wallet_api::request::stake::DelegateReq;

#[tokio::test]
async fn test_query_available_max() {
    let manager = get_manager().await;

    let account = "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string();
    let resource_type = "energy".to_string();
    let res = manager.get_can_delegated_max(account, resource_type).await;

    tracing::info!("delegate {}", serde_json::to_string(&res).unwrap());
}

#[tokio::test]
async fn test_delegate() {
    let manager = get_manager().await;

    let req = DelegateReq {
        owner_address: "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string(),
        receiver_address: "TNPTj8Dbba6YxW5Za6tFh6SJMZGbUyucXQ".to_string(),
        balance: "100".to_string(),
        resource: "energy".to_string(),
        lock: false,
        lock_period: 0,
    };
    let password = "123456".to_string();
    let res = manager.delegate_resource(req, password).await;

    tracing::info!("delegate {}", serde_json::to_string(&res).unwrap());
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
