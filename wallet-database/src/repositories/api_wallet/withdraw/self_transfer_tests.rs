use super::*;
use crate::{
    SqliteContext,
    repositories::{
        api_wallet::nonce::ApiNonceRepo,
        test_helper::{make_temp_dir, setup_api_transaction_pool},
    },
};

fn signed() -> SignedSelfTransfer<'static> {
    SignedSelfTransfer {
        trade_no: "ordinary-4",
        uid: "wallet",
        name: "wallet",
        from_addr: "sender",
        to_addr: "receiver",
        chain_code: "eth",
        token_addr: AssetTokenKey::Native,
        symbol: "ETH",
        value: "1",
        nonce: 4,
        tx_hash: "hash-4",
        raw_tx: "signed-bytes-4",
        transaction_fee: "0.001",
        notes: Some("note"),
    }
}

async fn row(pool: &ApiTransactionDbPool) -> ApiWithdrawEntity {
    sqlx::query_as("SELECT * FROM api_withdraws WHERE trade_no = 'ordinary-4'")
        .fetch_one(pool.read_ref())
        .await
        .unwrap()
}

#[tokio::test]
async fn self_transfer_survives_reopen_and_acceptance_is_not_confirmation() {
    let dir = make_temp_dir("self_transfer_reopen");
    let pool = SqliteContext::new(&dir, Some("api_transaction.db"))
        .await
        .unwrap()
        .into_transaction_db_pool()
        .unwrap();
    ApiWithdrawRepo::persist_signed_self_transfer(&pool, &signed()).await.unwrap();
    pool.read_ref().close().await;
    pool.write_ref().close().await;
    let pool = SqliteContext::new(&dir, Some("api_transaction.db"))
        .await
        .unwrap()
        .into_transaction_db_pool()
        .unwrap();

    let pending =
        ApiWithdrawRepo::pending_self_transfers_for_sender(&pool, "sender", "eth").await.unwrap();
    assert_eq!(pending.len(), 1);
    // Ordinary records must not be picked up by the backend withdrawal workers.
    assert!(
        crate::repositories::api_wallet::withdraw::ApiWithdrawRepo::scan_can_broadcast(&pool, 10)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        crate::repositories::api_wallet::withdraw::ApiWithdrawRepo::scan_need_recover(&pool, 10)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(pending[0].nonce, 4);
    assert_eq!(pending[0].raw_tx.as_deref(), Some("signed-bytes-4"));
    assert_eq!(pending[0].tx_hash.as_deref(), Some("hash-4"));
    assert!(pending[0].broadcast_uncertain_since_at.is_some());
    assert!(pending[0].last_broadcast_at.is_none());
    assert_eq!(ApiNonceRepo::get_api_nonce(&pool, "sender", "eth").await.unwrap(), 4);
    assert!(
        !ApiWithdrawRepo::mark_self_transfer_accepted(&pool, "ordinary-4", "wrong-hash")
            .await
            .unwrap()
    );
    assert!(
        ApiWithdrawRepo::mark_self_transfer_accepted(&pool, "ordinary-4", "hash-4").await.unwrap()
    );
    assert!(
        !ApiWithdrawRepo::mark_self_transfer_accepted(&pool, "ordinary-4", "hash-4").await.unwrap()
    );
    ApiWithdrawRepo::persist_signed_self_transfer(&pool, &signed()).await.unwrap();
    let accepted = row(&pool).await;
    assert!(accepted.last_broadcast_at.is_some());
    assert!(accepted.broadcast_uncertain_since_at.is_none());
    assert!(accepted.transaction_time.is_none());
    assert!(accepted.chain_success_at.is_none());
    assert!(accepted.finished_at.is_none());
    assert!(accepted.tx_ack_sent_at.is_none());
    assert_eq!(
        ApiWithdrawRepo::pending_self_transfers_for_sender(&pool, "sender", "eth")
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn self_transfer_conflicting_retry_preserves_original_and_nonce_floor() {
    let pool = setup_api_transaction_pool("self_transfer_conflict").await;
    ApiWithdrawRepo::persist_signed_self_transfer(&pool, &signed()).await.unwrap();
    let mut conflict = signed();
    conflict.nonce = 5;
    conflict.raw_tx = "different-bytes";
    assert!(ApiWithdrawRepo::persist_signed_self_transfer(&pool, &conflict).await.is_err());
    let original = row(&pool).await;
    assert_eq!(original.nonce, 4);
    assert_eq!(original.raw_tx.as_deref(), Some("signed-bytes-4"));
    assert_eq!(ApiNonceRepo::get_api_nonce(&pool, "sender", "eth").await.unwrap(), 4);
    ApiNonceRepo::set_nonce_floor(&pool, "sender", "eth", 9).await.unwrap();
    ApiWithdrawRepo::persist_signed_self_transfer(&pool, &signed()).await.unwrap();
    assert_eq!(ApiNonceRepo::get_api_nonce(&pool, "sender", "eth").await.unwrap(), 9);
}

#[tokio::test]
async fn self_transfer_nonce_write_failure_rolls_back_signed_record() {
    let pool = setup_api_transaction_pool("self_transfer_rollback").await;
    ApiNonceRepo::set_nonce_floor(&pool, "sender", "eth", 2).await.unwrap();
    sqlx::query("CREATE TRIGGER reject_nonce BEFORE UPDATE ON api_nonce BEGIN SELECT RAISE(ABORT, 'injected failure'); END")
        .execute(pool.write_ref()).await.unwrap();
    assert!(ApiWithdrawRepo::persist_signed_self_transfer(&pool, &signed()).await.is_err());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM api_withdraws")
        .fetch_one(pool.read_ref())
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(ApiNonceRepo::get_api_nonce(&pool, "sender", "eth").await.unwrap(), 2);
    sqlx::query("DROP TRIGGER reject_nonce").execute(pool.write_ref()).await.unwrap();
    ApiWithdrawRepo::persist_signed_self_transfer(&pool, &signed()).await.unwrap();
    assert_eq!(row(&pool).await.nonce, 4);
}

#[tokio::test]
async fn self_transfer_isolates_sender_chain_trade_type_and_terminal_rows() {
    let pool = setup_api_transaction_pool("self_transfer_isolation").await;
    ApiWithdrawRepo::persist_signed_self_transfer(&pool, &signed()).await.unwrap();
    assert!(
        ApiWithdrawRepo::pending_self_transfers_for_sender(&pool, "other", "eth")
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        ApiWithdrawRepo::pending_self_transfers_for_sender(&pool, "sender", "bnb")
            .await
            .unwrap()
            .is_empty()
    );
    for mutation in ["trade_type = 1", "trade_type = 4, finished_at = '2026-09-08T00:00:00Z'"] {
        sqlx::query(&format!("UPDATE api_withdraws SET {mutation}"))
            .execute(pool.write_ref())
            .await
            .unwrap();
        assert!(
            !ApiWithdrawRepo::mark_self_transfer_accepted(&pool, "ordinary-4", "hash-4")
                .await
                .unwrap()
        );
        assert!(
            ApiWithdrawRepo::pending_self_transfers_for_sender(&pool, "sender", "eth")
                .await
                .unwrap()
                .is_empty()
        );
    }
}

#[tokio::test]
async fn self_transfer_bnb_zero_nonce_and_invalid_input() {
    let pool = setup_api_transaction_pool("self_transfer_bnb").await;
    let mut input = signed();
    input.chain_code = "tron";
    assert!(ApiWithdrawRepo::persist_signed_self_transfer(&pool, &input).await.is_err());
    input.chain_code = "bnb";
    input.nonce = -1;
    assert!(ApiWithdrawRepo::persist_signed_self_transfer(&pool, &input).await.is_err());
    input.nonce = 0;
    input.raw_tx = "";
    assert!(ApiWithdrawRepo::persist_signed_self_transfer(&pool, &input).await.is_err());
    assert!(ApiNonceRepo::get_api_nonce_optional(&pool, "sender", "bnb").await.unwrap().is_none());
    input.raw_tx = "bnb-signed-zero";
    ApiWithdrawRepo::persist_signed_self_transfer(&pool, &input).await.unwrap();
    assert_eq!(ApiNonceRepo::get_api_nonce(&pool, "sender", "bnb").await.unwrap(), 0);
    assert_eq!(
        ApiWithdrawRepo::pending_self_transfers_for_sender(&pool, "sender", "bnb")
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn self_transfer_concurrent_identical_retries_write_one_record() {
    let pool = setup_api_transaction_pool("self_transfer_retry").await;
    let input = signed();
    let (a, b) = tokio::join!(
        ApiWithdrawRepo::persist_signed_self_transfer(&pool, &input),
        ApiWithdrawRepo::persist_signed_self_transfer(&pool, &input)
    );
    a.unwrap();
    b.unwrap();
    assert_eq!(
        ApiWithdrawRepo::pending_self_transfers_for_sender(&pool, "sender", "eth")
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(ApiNonceRepo::get_api_nonce(&pool, "sender", "eth").await.unwrap(), 4);
}
