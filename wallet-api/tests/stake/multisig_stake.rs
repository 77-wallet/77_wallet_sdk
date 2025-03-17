use crate::get_manager;
use wallet_api::request::{
    stake::{DelegateReq, FreezeBalanceReq, UnDelegateReq, VoteWitnessReq, VotesReq},
    transaction::Signer,
};
use wallet_database::entities::bill::BillKind;

#[tokio::test]
async fn test_build_freeze() {
    let manager = get_manager().await;

    let singer = Signer {
        address: "TNPTj8Dbba6YxW5Za6tFh6SJMZGbUyucXQ".to_string(),
        permission_id: 4,
    };

    let req = FreezeBalanceReq {
        owner_address: "TNPTj8Dbba6YxW5Za6tFh6SJMZGbUyucXQ".to_string(),
        resource: "energy".to_string(),
        frozen_balance: 50,
        signer: Some(singer),
    };

    let bill_kind = BillKind::DelegateEnergy.to_i8() as i64;
    let content = serde_json::to_string(&req).unwrap();

    // let content = r#"{"ownerAddress":"TVx7Pi8Ftgzd7AputaoLidBR3Vb9xKfhqY","receiverAddress":"TTD5EM94SmLPSTyvzjiisjB71QCD4vHQcm","balance":5,"resource":"ENERGY","lock":false,"lockPeriod":0,"signer":{"address":"TVx7Pi8Ftgzd7AputaoLidBR3Vb9xKfhqY","permissionId":2}}"#.to_string();

    let password = "123456".to_string();
    let res = manager
        .build_multisig_stake(bill_kind, content, 1, password)
        .await;

    tracing::info!("delegate {}", serde_json::to_string(&res).unwrap());
}

#[tokio::test]
async fn test_build_delegate() {
    let manager = get_manager().await;

    let req = DelegateReq {
        owner_address: "TNPTj8Dbba6YxW5Za6tFh6SJMZGbUyucXQ".to_string(),
        receiver_address: "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string(),
        balance: 100000,
        resource: "energy".to_string(),
        lock: false,
        lock_period: 10000000.0,
        signer: None,
    };

    let bill_kind = BillKind::DelegateEnergy.to_i8() as i64;
    let content = serde_json::to_string(&req).unwrap();

    let password = "123456".to_string();
    let res = manager
        .build_multisig_stake(bill_kind, content, 1, password)
        .await;

    tracing::info!("delegate {}", serde_json::to_string(&res).unwrap());
}

#[tokio::test]
async fn test_build_un_delegate() {
    let manager = get_manager().await;

    let req = UnDelegateReq {
        owner_address: "TNPTj8Dbba6YxW5Za6tFh6SJMZGbUyucXQ".to_string(),
        receiver_address: "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string(),
        balance: 3,
        resource: "energy".to_string(),
        signer: None,
    };

    let bill_kind = BillKind::UnDelegateEnergy.to_i8() as i64;
    let content = serde_json::to_string(&req).unwrap();

    let password = "123456".to_string();
    let res = manager
        .build_multisig_stake(bill_kind, content, 1, password)
        .await;

    tracing::info!("delegate {}", serde_json::to_string(&res).unwrap());
}

#[tokio::test]
async fn test_build_vote() {
    let manager = get_manager().await;

    let owner_address = "TFdDqaoMkPbWWv9EUTbmfGP142f9ysiJq2";
    let req = VoteWitnessReq::new(
        owner_address,
        vec![VotesReq::new("TA4pHhHgobzSGH3CWPsZ5URNk3QkzUEggX", 1)],
        None,
    );
    let bill_kind = BillKind::Vote.to_i8() as i64;
    let content = serde_json::to_string(&req).unwrap();

    let password = "123456".to_string();
    let res = manager
        .build_multisig_stake(bill_kind, content, 1, password)
        .await;

    tracing::info!("vote {}", serde_json::to_string(&res).unwrap());
}
