use crate::get_manager;
use wallet_api::request::stake::FreezeBalanceReq;
use wallet_database::entities::bill::BillKind;

#[tokio::test]
async fn test_build_freeze() {
    let manager = get_manager().await;

    let req = FreezeBalanceReq {
        owner_address: "TNPTj8Dbba6YxW5Za6tFh6SJMZGbUyucXQ".to_string(),
        resource: "bandwidth".to_string(),
        frozen_balance: 100,
    };

    let bill_kind = BillKind::FreezeBandwidth.to_i8() as i64;
    let content = serde_json::to_string(&req).unwrap();

    let password = "123456".to_string();
    let res = manager
        .build_multisig_stake(bill_kind, content, 1, password)
        .await;

    tracing::info!("delegate {}", serde_json::to_string(&res).unwrap());
}
