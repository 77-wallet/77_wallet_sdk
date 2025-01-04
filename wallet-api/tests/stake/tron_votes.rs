use wallet_api::request::stake::{VoteWitnessReq, VotesReq, WithdrawBalanceReq};
use wallet_database::entities::bill::BillKind;

use crate::get_manager;

#[tokio::test]
async fn test_vote() {
    let manager = get_manager().await;
    let owner_address = "TUDrRQ6zvwXhW3ScTxwGv8nwicLShVVWoF";
    let vote_witness_req = VoteWitnessReq::new(
        owner_address,
        vec![VotesReq::new("TEp1ru7opCexkbFM9ChK6DFfL2XFSfUo2N", 100)],
    );
    let password = "123456"; // Replace with the actual password

    let res = manager.votes(vote_witness_req, password).await;

    tracing::info!("votes {}", serde_json::to_string(&res).unwrap());
}

#[tokio::test]
async fn test_votes_fee() {
    let manager = get_manager().await;
    let vote_address = "TA4pHhHgobzSGH3CWPsZ5URNk3QkzUEggX";
    let vote_count = 1;
    let votes = VotesReq::new(vote_address, vote_count);
    let owner_address = "TFdDqaoMkPbWWv9EUTbmfGP142f9ysiJq2";
    let req = VoteWitnessReq::new(owner_address, vec![votes]);

    let bill_kind = BillKind::Vote.to_i8() as i64;

    let content = serde_json::to_string(&req).unwrap();
    let res = manager.estimate_stake_fee(bill_kind, content).await;

    tracing::info!("fee {}", serde_json::to_string(&res).unwrap());
}

#[tokio::test]
async fn test_withdraw_fee() {
    let manager = get_manager().await;

    let owner_address = "TJXWzjm6EuS7tzSXRBf9sHYBA5pcbsW7as";
    let req = WithdrawBalanceReq::new(owner_address);

    let bill_kind = BillKind::WithdrawReward.to_i8() as i64;

    let content = serde_json::to_string(&req).unwrap();
    let res = manager.estimate_stake_fee(bill_kind, content).await;

    tracing::info!("fee {}", serde_json::to_string(&res).unwrap());
}
