use crate::get_manager;
use wallet_api::request::stake::{
    BatchDelegate, BatchList, BatchUnDelegate, DelegateReq, UnDelegateReq,
};
use wallet_database::entities::bill::BillKind;

#[tokio::test]
async fn test_query_available_max() {
    let manager = get_manager().await;

    let account = "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string();
    let resource_type = "energy".to_string();
    let res = manager.get_can_delegated_max(account, resource_type).await;

    tracing::info!("delegate {}", serde_json::to_string(&res).unwrap());
}

#[tokio::test]
async fn test_delegate_fee() {
    let manager = get_manager().await;

    let req = DelegateReq {
        owner_address: "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string(),
        receiver_address: "TNPTj8Dbba6YxW5Za6tFh6SJMZGbUyucXQ".to_string(),
        balance: 50,
        resource: "energy".to_string(),
        lock: false,
        lock_period: 0,
    };

    let bill_kind = BillKind::DelegateBandwidth.to_i8();
    let content = serde_json::to_string(&req).unwrap();

    let res = manager.estimate_stake_fee(bill_kind as i64, content).await;
    tracing::info!("delegate fee {}", serde_json::to_string(&res).unwrap());
}

#[tokio::test]
async fn test_delegate() {
    let manager = get_manager().await;

    let req = DelegateReq {
        owner_address: "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string(),
        receiver_address: "TNPTj8Dbba6YxW5Za6tFh6SJMZGbUyucXQ".to_string(),
        balance: 50,
        resource: "energy".to_string(),
        lock: false,
        lock_period: 0,
    };
    let password = "123456".to_string();
    let res = manager.delegate_resource(req, password).await;

    tracing::info!("delegate {}", serde_json::to_string(&res).unwrap());
}

#[tokio::test]
async fn test_batch_delegate_fee() {
    let rerevice1 = BatchList {
        revevie_address: "TNPTj8Dbba6YxW5Za6tFh6SJMZGbUyucXQ".to_string(),
        value: 100,
    };

    let rerevice2 = BatchList {
        revevie_address: "TUe3T6ErJvnoHMQwVrqK246MWeuCEBbyuR".to_string(),
        value: 100,
    };

    let req = BatchDelegate {
        owner_address: "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string(),
        resource_type: "energy".to_string(),
        list: vec![rerevice1, rerevice2],
        lock: false,
        lock_period: 0,
    };

    let bill_kind = BillKind::BatchDelegateBandwidth.to_i8() as i64;
    let content = serde_json::to_string(&req).unwrap();

    let manager = get_manager().await;
    let res = manager.estimate_stake_fee(bill_kind, content).await;

    tracing::info!("delegate {}", serde_json::to_string(&res).unwrap());
}

#[tokio::test]
async fn test_batch_delegate() {
    let rerevice1 = BatchList {
        revevie_address: "TNPTj8Dbba6YxW5Za6tFh6SJMZGbUyucXQ".to_string(),
        value: 100,
    };

    let rerevice2 = BatchList {
        revevie_address: "TUe3T6ErJvnoHMQwVrqK246MWeuCEBbyuR".to_string(),
        value: 100,
    };

    let req = BatchDelegate {
        owner_address: "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string(),
        resource_type: "energy".to_string(),
        list: vec![rerevice1, rerevice2],
        lock: false,
        lock_period: 0,
    };

    let manager = get_manager().await;
    let password = "123456".to_string();
    let res = manager.batch_delegate(req, password).await;

    tracing::info!("delegate {}", serde_json::to_string(&res).unwrap());
}

#[tokio::test]
async fn test_batch_un_delegate_fee() {
    let rerevice1 = BatchList {
        revevie_address: "TNPTj8Dbba6YxW5Za6tFh6SJMZGbUyucXQ".to_string(),
        value: 100,
    };

    let rerevice2 = BatchList {
        revevie_address: "TUe3T6ErJvnoHMQwVrqK246MWeuCEBbyuR".to_string(),
        value: 100,
    };

    let req = BatchUnDelegate {
        owner_address: "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string(),
        resource_type: "energy".to_string(),
        list: vec![rerevice1, rerevice2],
    };

    let bill_kind = BillKind::BatchUnDelegateEnergy.to_i8() as i64;
    let content = serde_json::to_string(&req).unwrap();

    let manager = get_manager().await;
    let res = manager.estimate_stake_fee(bill_kind, content).await;

    tracing::info!("delegate {}", serde_json::to_string(&res).unwrap());
}

#[tokio::test]
async fn test_batch_un_delegate() {
    let rerevice1 = BatchList {
        revevie_address: "TNPTj8Dbba6YxW5Za6tFh6SJMZGbUyucXQ".to_string(),
        value: 100,
    };

    let rerevice2 = BatchList {
        revevie_address: "TUe3T6ErJvnoHMQwVrqK246MWeuCEBbyuR".to_string(),
        value: 100,
    };

    let req = BatchUnDelegate {
        owner_address: "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string(),
        resource_type: "energy".to_string(),
        list: vec![rerevice1, rerevice2],
    };

    let manager = get_manager().await;
    let password = "123456".to_string();
    let res = manager.batch_un_deleate(req, password).await;

    tracing::info!("delegate {}", serde_json::to_string(&res).unwrap());
}

#[tokio::test]
async fn test_delegate_to_other() {
    let manager = get_manager().await;

    let owner_address = "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string();
    let res = manager.delegate_to_other(owner_address).await;

    tracing::info!("delegate {}", serde_json::to_string(&res).unwrap());
}

#[tokio::test]
async fn test_delegate_from_other() {
    let manager = get_manager().await;

    let owner_address = "TNPTj8Dbba6YxW5Za6tFh6SJMZGbUyucXQ".to_string();
    let res = manager.delegate_from_other(owner_address).await;

    tracing::info!("delegate {}", serde_json::to_string(&res).unwrap());
}

#[tokio::test]
async fn test_un_delegate() {
    let manager = get_manager().await;

    let req = UnDelegateReq {
        owner_address: "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string(),
        receiver_address: "TNPTj8Dbba6YxW5Za6tFh6SJMZGbUyucXQ".to_string(),
        balance: 100,
        resource: "energy".to_string(),
    };
    let password = "123456".to_string();
    let res = manager.un_delegate_resource(req, password).await;
    tracing::info!("un delegate  {}", serde_json::to_string(&res).unwrap());
}

#[tokio::test]
async fn test_undelegate_fee() {
    let manager = get_manager().await;

    let req = UnDelegateReq {
        owner_address: "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string(),
        resource: "energy".to_string(),
        receiver_address: "TNPTj8Dbba6YxW5Za6tFh6SJMZGbUyucXQ".to_string(),
        balance: 50,
    };

    let bill_kind = BillKind::UnDelegateEnergy.to_i8() as i64;

    let content = serde_json::to_string(&req).unwrap();
    let res = manager.estimate_stake_fee(bill_kind, content).await;

    tracing::info!("fee {}", serde_json::to_string(&res).unwrap());
}
