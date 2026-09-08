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

fn receipt(success: bool) -> SelfTransferReceipt<'static> {
    SelfTransferReceipt {
        trade_no: "ordinary-4",
        chain_code: "eth",
        tx_hash: "hash-4",
        success,
        transaction_time: chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
        block_height: "1234",
        transaction_fee: "0.00002",
        resource_consume: "21000",
    }
}

#[tokio::test]
async fn self_transfer_receipt_finishes_success_and_failure_without_nonce_reuse() {
    for success in [true, false] {
        let pool = setup_api_transaction_pool("self_transfer_receipt").await;
        ApiWithdrawRepo::persist_signed_self_transfer(&pool, &signed()).await.unwrap();

        assert!(
            ApiWithdrawRepo::confirm_self_transfer_receipt(&pool, &receipt(success)).await.unwrap()
        );

        let confirmed = row(&pool).await;
        assert_eq!(
            confirmed.status,
            if success { ApiWithdrawStatus::Success } else { ApiWithdrawStatus::Failure }
        );
        assert_eq!(confirmed.transaction_time, Some(receipt(success).transaction_time));
        assert_eq!(confirmed.block_height.as_deref(), Some("1234"));
        assert_eq!(confirmed.transaction_fee, "0.00002");
        assert_eq!(confirmed.resource_consume, "21000");
        assert_eq!(confirmed.chain_success_at.is_some(), success);
        assert_eq!(confirmed.chain_failed_at.is_some(), !success);
        assert!(confirmed.finished_at.is_some());
        assert!(confirmed.broadcast_uncertain_since_at.is_none());
        assert!(confirmed.tx_ack_sent_at.is_none());
        assert!(confirmed.tx_res_ack_sent_at.is_none());
        assert!(confirmed.tx_exec_receipt_uploaded_at.is_none());
        assert_eq!(confirmed.raw_tx.as_deref(), Some("signed-bytes-4"));
        assert_eq!(confirmed.tx_hash.as_deref(), Some("hash-4"));
        assert_eq!(ApiNonceRepo::get_api_nonce(&pool, "sender", "eth").await.unwrap(), 4);
        assert!(
            ApiWithdrawRepo::pending_self_transfers_for_sender(&pool, "sender", "eth")
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            !ApiWithdrawRepo::mark_self_transfer_accepted(&pool, "ordinary-4", "hash-4")
                .await
                .unwrap()
        );
    }
}

#[tokio::test]
async fn self_transfer_receipt_rejects_wrong_identity_and_backend_withdrawal() {
    let pool = setup_api_transaction_pool("self_transfer_receipt_identity").await;
    ApiWithdrawRepo::persist_signed_self_transfer(&pool, &signed()).await.unwrap();
    let mut wrong = receipt(true);
    wrong.tx_hash = "other-hash";
    assert!(!ApiWithdrawRepo::confirm_self_transfer_receipt(&pool, &wrong).await.unwrap());
    wrong = receipt(true);
    wrong.chain_code = "bnb";
    assert!(!ApiWithdrawRepo::confirm_self_transfer_receipt(&pool, &wrong).await.unwrap());
    wrong = receipt(true);
    wrong.trade_no = "other-order";
    assert!(!ApiWithdrawRepo::confirm_self_transfer_receipt(&pool, &wrong).await.unwrap());
    sqlx::query("UPDATE api_withdraws SET trade_type = 1").execute(pool.write_ref()).await.unwrap();
    assert!(!ApiWithdrawRepo::confirm_self_transfer_receipt(&pool, &receipt(true)).await.unwrap());
    let unchanged = row(&pool).await;
    assert!(unchanged.transaction_time.is_none());
    assert!(unchanged.finished_at.is_none());
    assert!(unchanged.broadcast_uncertain_since_at.is_some());
    assert_eq!(unchanged.transaction_fee, "0.001");
}

#[tokio::test]
async fn self_transfer_receipt_concurrent_retry_and_opposite_result_cannot_overwrite() {
    let pool = setup_api_transaction_pool("self_transfer_receipt_retry").await;
    ApiWithdrawRepo::persist_signed_self_transfer(&pool, &signed()).await.unwrap();
    let fact = receipt(true);
    let (a, b) = tokio::join!(
        ApiWithdrawRepo::confirm_self_transfer_receipt(&pool, &fact),
        ApiWithdrawRepo::confirm_self_transfer_receipt(&pool, &fact),
    );
    assert_ne!(a.unwrap(), b.unwrap(), "exactly one caller records the receipt");
    let original = row(&pool).await;
    assert!(!ApiWithdrawRepo::confirm_self_transfer_receipt(&pool, &receipt(false)).await.unwrap());
    let final_row = row(&pool).await;
    assert_eq!(final_row.status, ApiWithdrawStatus::Success);
    assert_eq!(final_row.finished_at, original.finished_at);
    assert_eq!(final_row.chain_success_at, original.chain_success_at);
    assert!(final_row.chain_failed_at.is_none());
}

#[tokio::test]
async fn self_transfer_receipt_terminal_write_failure_rolls_back_then_recovers() {
    let pool = setup_api_transaction_pool("self_transfer_receipt_rollback").await;
    ApiWithdrawRepo::persist_signed_self_transfer(&pool, &signed()).await.unwrap();
    sqlx::query("CREATE TRIGGER reject_finish BEFORE UPDATE OF finished_at ON api_withdraws BEGIN SELECT RAISE(ABORT, 'injected finish failure'); END")
        .execute(pool.write_ref()).await.unwrap();

    assert!(ApiWithdrawRepo::confirm_self_transfer_receipt(&pool, &receipt(true)).await.is_err());

    let unchanged = row(&pool).await;
    assert_eq!(unchanged.status, ApiWithdrawStatus::Init);
    assert!(unchanged.transaction_time.is_none());
    assert!(unchanged.chain_success_at.is_none());
    assert!(unchanged.finished_at.is_none());
    assert!(unchanged.broadcast_uncertain_since_at.is_some());
    assert_eq!(unchanged.transaction_fee, "0.001");
    assert_eq!(ApiNonceRepo::get_api_nonce(&pool, "sender", "eth").await.unwrap(), 4);
    sqlx::query("DROP TRIGGER reject_finish").execute(pool.write_ref()).await.unwrap();
    assert!(ApiWithdrawRepo::confirm_self_transfer_receipt(&pool, &receipt(true)).await.unwrap());
    assert!(row(&pool).await.finished_at.is_some());
}

#[tokio::test]
async fn self_transfer_scan_cursor_survives_earlier_completion() {
    let pool = setup_api_transaction_pool("self_transfer_scan").await;
    let first = signed();
    ApiWithdrawRepo::persist_signed_self_transfer(&pool, &first).await.unwrap();
    let mut second = signed();
    second.trade_no = "ordinary-5";
    second.nonce = 5;
    ApiWithdrawRepo::persist_signed_self_transfer(&pool, &second).await.unwrap();
    let page = ApiWithdrawRepo::scan_pending_self_transfers(&pool, 0, 1).await.unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].trade_no, "ordinary-4");
    ApiWithdrawRepo::confirm_self_transfer_receipt(&pool, &receipt(true)).await.unwrap();
    let next = ApiWithdrawRepo::scan_pending_self_transfers(&pool, page[0].id, 1).await.unwrap();
    assert_eq!(next.len(), 1);
    assert_eq!(next[0].trade_no, "ordinary-5");
    assert!(
        ApiWithdrawRepo::scan_pending_self_transfers(&pool, next[0].id, 1)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(ApiWithdrawRepo::scan_pending_self_transfers(&pool, 0, 0).await.unwrap().is_empty());
    sqlx::query("UPDATE api_withdraws SET trade_type = 1 WHERE trade_no = 'ordinary-5'")
        .execute(pool.write_ref())
        .await
        .unwrap();
    assert!(ApiWithdrawRepo::scan_pending_self_transfers(&pool, 0, 10).await.unwrap().is_empty());
}
