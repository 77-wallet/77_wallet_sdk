use super::{ApiCollectRepo, SignedCollectCandidate};
use crate::{
    ApiTransactionDbPool,
    repositories::{api_wallet::nonce::ApiNonceRepo, test_helper::setup_api_transaction_pool},
};

async fn setup(name: &str, trade: &str) -> ApiTransactionDbPool {
    let pool = setup_api_transaction_pool(name).await;
    ApiCollectRepo::upsert_api_collect(
        &pool,
        "uid",
        "collect",
        "sender",
        "receiver",
        "1",
        "v",
        "arbitrum",
        None,
        "ETH",
        trade,
        2,
        crate::entities::api_collect::ApiCollectStatus::Init,
        1,
    )
    .await
    .unwrap();
    ApiCollectRepo::set_order_ack_sent(&pool, trade).await.unwrap();
    assert_eq!(ApiCollectRepo::update_building_at(&pool, trade).await.unwrap(), 1);
    pool
}
fn candidate(trade: &str) -> SignedCollectCandidate<'_> {
    SignedCollectCandidate {
        trade_no: trade,
        from_addr: "sender",
        chain_code: "arbitrum",
        tx_hash: "hash",
        raw_tx: "raw",
        transaction_fee: "0.1",
        nonce: 6,
    }
}

#[tokio::test]
async fn collect_signed_candidate_success_commits_raw_status_and_nonce_once() {
    let pool = setup("collect_candidate_success", "collect").await;
    assert_eq!(
        ApiCollectRepo::reserve_signed_collect(&pool, &candidate("collect")).await.unwrap(),
        1
    );
    let row = ApiCollectRepo::get_api_collect_by_trade_no(&pool, "collect").await.unwrap();
    assert_eq!(row.raw_tx.as_deref(), Some("raw"));
    assert_eq!(row.nonce, 6);
    assert_eq!(row.status, row.recompute_status());
    assert_eq!(ApiNonceRepo::get_api_nonce(&pool, "sender", "arbitrum").await.unwrap(), 6);
    assert_eq!(
        ApiCollectRepo::reserve_signed_collect(&pool, &candidate("collect")).await.unwrap(),
        0
    );
    let mut different = candidate("collect");
    different.raw_tx = "different";
    assert!(ApiCollectRepo::reserve_signed_collect(&pool, &different).await.is_err());
}

#[tokio::test]
async fn collect_signed_candidate_competing_allocator_leaves_no_raw() {
    let pool = setup("collect_candidate_conflict", "collect").await;
    ApiNonceRepo::set_nonce_floor(&pool, "sender", "arbitrum", 6).await.unwrap();
    assert!(ApiCollectRepo::reserve_signed_collect(&pool, &candidate("collect")).await.is_err());
    assert!(
        ApiCollectRepo::get_api_collect_by_trade_no(&pool, "collect")
            .await
            .unwrap()
            .raw_tx
            .is_none()
    );
    assert_eq!(ApiNonceRepo::get_api_nonce(&pool, "sender", "arbitrum").await.unwrap(), 6);
}

#[tokio::test]
async fn collect_signed_candidate_stale_lifecycle_and_identity_roll_back_nonce() {
    let pool = setup("collect_candidate_stale", "collect").await;
    let mut wrong = candidate("collect");
    wrong.from_addr = "other";
    assert!(ApiCollectRepo::reserve_signed_collect(&pool, &wrong).await.is_err());
    assert!(
        ApiNonceRepo::get_api_nonce_optional(&pool, "other", "arbitrum").await.unwrap().is_none()
    );
    sqlx::query(
        "UPDATE api_collect SET finished_at = CURRENT_TIMESTAMP WHERE trade_no = 'collect'",
    )
    .execute(pool.write_ref())
    .await
    .unwrap();
    assert!(ApiCollectRepo::reserve_signed_collect(&pool, &candidate("collect")).await.is_err());
    assert!(
        ApiNonceRepo::get_api_nonce_optional(&pool, "sender", "arbitrum").await.unwrap().is_none()
    );
    assert!(
        ApiCollectRepo::get_api_collect_by_trade_no(&pool, "collect")
            .await
            .unwrap()
            .raw_tx
            .is_none()
    );
}

#[tokio::test]
async fn collect_signed_candidate_concurrent_same_sender_commits_only_one() {
    let pool = setup("collect_candidate_concurrent", "collect1").await;
    ApiCollectRepo::upsert_api_collect(
        &pool,
        "uid",
        "collect",
        "sender",
        "receiver",
        "1",
        "v",
        "arbitrum",
        None,
        "ETH",
        "collect2",
        2,
        crate::entities::api_collect::ApiCollectStatus::Init,
        1,
    )
    .await
    .unwrap();
    ApiCollectRepo::set_order_ack_sent(&pool, "collect2").await.unwrap();
    ApiCollectRepo::update_building_at(&pool, "collect2").await.unwrap();
    let first = candidate("collect1");
    let second = candidate("collect2");
    let (a, b) = tokio::join!(
        ApiCollectRepo::reserve_signed_collect(&pool, &first),
        ApiCollectRepo::reserve_signed_collect(&pool, &second)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    let a = ApiCollectRepo::get_api_collect_by_trade_no(&pool, "collect1").await.unwrap();
    let b = ApiCollectRepo::get_api_collect_by_trade_no(&pool, "collect2").await.unwrap();
    assert_eq!(usize::from(a.raw_tx.is_some()) + usize::from(b.raw_tx.is_some()), 1);
    assert_eq!(ApiNonceRepo::get_api_nonce(&pool, "sender", "arbitrum").await.unwrap(), 6);
}

#[tokio::test]
async fn collect_signed_candidate_result_ack_terminal_rolls_back_nonce() {
    let pool = setup("collect_candidate_result_ack", "collect").await;
    sqlx::query(
        "UPDATE api_collect SET result_ack_sent_at = CURRENT_TIMESTAMP WHERE trade_no = 'collect'",
    )
    .execute(pool.write_ref())
    .await
    .unwrap();
    assert!(ApiCollectRepo::reserve_signed_collect(&pool, &candidate("collect")).await.is_err());
    let row = ApiCollectRepo::get_api_collect_by_trade_no(&pool, "collect").await.unwrap();
    assert!(row.raw_tx.is_none());
    assert!(row.tx_hash.is_none());
    assert!(row.finished_at.is_none());
    assert!(row.result_ack_sent_at.is_some());
    assert!(
        ApiNonceRepo::get_api_nonce_optional(&pool, "sender", "arbitrum").await.unwrap().is_none()
    );
}

#[tokio::test]
async fn collect_signed_candidate_late_lifecycle_change_cannot_reserve_nonce() {
    for (n, assignment) in [
        "need_service_fee = true",
        "ever_needed_service_fee = true",
        "err_code = 6001",
        "order_ack_sent_at = NULL",
        "building_at = NULL",
        "broadcast_uncertain_since_at = CURRENT_TIMESTAMP",
        "tx_exec_receipt_uploaded_at = CURRENT_TIMESTAMP",
    ]
    .into_iter()
    .enumerate()
    {
        let pool = setup(&format!("collect_candidate_gate_{n}"), "collect").await;
        sqlx::query(&format!("UPDATE api_collect SET {assignment} WHERE trade_no = 'collect'"))
            .execute(pool.write_ref())
            .await
            .unwrap();
        assert!(
            ApiCollectRepo::reserve_signed_collect(&pool, &candidate("collect")).await.is_err(),
            "{assignment}"
        );
        assert!(
            ApiNonceRepo::get_api_nonce_optional(&pool, "sender", "arbitrum")
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            ApiCollectRepo::get_api_collect_by_trade_no(&pool, "collect")
                .await
                .unwrap()
                .raw_tx
                .is_none()
        );
    }
}

#[tokio::test]
async fn collect_signed_candidate_status_write_failure_rolls_back_signed_facts_and_nonce() {
    let pool = setup("collect_candidate_status_rollback", "collect").await;
    sqlx::query("CREATE TRIGGER reject_collect_status BEFORE UPDATE OF status ON api_collect BEGIN SELECT RAISE(ABORT, 'status write failed'); END")
        .execute(pool.write_ref()).await.unwrap();
    assert!(ApiCollectRepo::reserve_signed_collect(&pool, &candidate("collect")).await.is_err());
    let row = ApiCollectRepo::get_api_collect_by_trade_no(&pool, "collect").await.unwrap();
    assert!(row.raw_tx.is_none());
    assert!(row.tx_hash.is_none());
    assert!(
        ApiNonceRepo::get_api_nonce_optional(&pool, "sender", "arbitrum").await.unwrap().is_none()
    );
}
