mod support;

use serial_test::serial;

use support::{
    ScenarioRoles, TransferNonceGiven, TransferNonceScenario, TransferNonceThen, TransferNonceWhen,
};

#[tokio::test]
#[serial]
async fn api_wallet_transfer_nonce_lock_keeps_same_address_requests_serial() -> anyhow::Result<()> {
    let scenario = TransferNonceScenario::new().await;
    let given = scenario.given();
    let when = scenario.when();
    let then = scenario.then();

    given.bnb_transfer_fixture().await?;
    given.first_transfer_blocks();
    given.fake_chain_adapter();
    given.wallet_password_cached().await;

    let first = when.transfer_starts();
    then.first_transfer_has_entered().await;
    scenario.then_durable_transfer_is(1, false).await?;

    let second = when.transfer_starts();
    then.only_first_nonce_is_recorded_while_second_waits().await;

    when.first_transfer_is_released();

    then.serial_transfer_results_are(first, second).await?;
    scenario.then_durable_transfer_is(1, true).await?;
    scenario.then_durable_transfer_is(2, true).await?;

    Ok(())
}

#[tokio::test]
#[serial]
async fn api_wallet_transfer_nonce_failure_keeps_reserved_nonce() -> anyhow::Result<()> {
    let scenario = TransferNonceScenario::new().await;
    let given = scenario.given();
    let when = scenario.when();
    let then = scenario.then();

    given.bnb_transfer_fixture().await?;
    given.transfer_fails();
    given.fake_chain_adapter();
    given.wallet_password_cached().await;

    let err = when.transfer_fails().await;

    then.failure_keeps_reserved_nonce(err).await?;

    Ok(())
}

#[tokio::test]
#[serial]
async fn collect_nonce_skips_consumed_chain_nonce() -> anyhow::Result<()> {
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given_chain_next_nonce(Some(2));

    let nonce = scenario.when_collect_nonce_is_allocated().await?;

    assert_eq!(nonce, 2);
    scenario.then_nonce_floor_and_queries_are(2, 1).await;
    Ok(())
}

#[tokio::test]
#[serial]
async fn collect_nonce_stale_chain_response_preserves_local_reservations() -> anyhow::Result<()> {
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given_chain_next_nonce(Some(2));
    assert_eq!(scenario.when_collect_nonce_is_allocated().await?, 2);
    scenario.given_chain_next_nonce(Some(0));

    let nonce = scenario.when_collect_nonce_is_allocated().await?;

    assert_eq!(nonce, 3);
    scenario.then_nonce_floor_and_queries_are(3, 2).await;
    Ok(())
}

#[tokio::test]
#[serial]
async fn collect_nonce_rpc_failure_does_not_reserve_nonce() -> anyhow::Result<()> {
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given_chain_next_nonce(None);

    let error = scenario.when_collect_nonce_is_allocated().await.unwrap_err();

    assert!(error.to_string().contains("simulated nonce RPC failure"));
    scenario.then_nonce_floor_and_queries_are(0, 1).await;
    Ok(())
}

#[tokio::test]
#[serial]
async fn collect_nonce_concurrent_allocations_remain_unique() -> anyhow::Result<()> {
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given_chain_next_nonce(Some(2));

    let (first, second) = tokio::join!(
        scenario.when_collect_nonce_is_allocated(),
        scenario.when_collect_nonce_is_allocated(),
    );

    let mut nonces = [first?, second?];
    nonces.sort();
    assert_eq!(nonces, [2, 3]);
    scenario.then_nonce_floor_and_queries_are(3, 2).await;
    Ok(())
}

#[tokio::test]
#[serial]
async fn nonce_alignment_preserves_reserved_nonces() -> anyhow::Result<()> {
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given_chain_next_nonce(Some(9));
    for expected in 9..=11 {
        assert_eq!(scenario.when_collect_nonce_is_allocated().await?, expected);
    }

    let alignment = scenario.when_nonce_alignment_runs().await;
    let next = scenario.when_collect_nonce_is_allocated().await?;

    assert_eq!(next, 12, "alignment must not reissue reserved nonce 9");
    assert!(alignment.unwrap_err().to_string().contains("nonce_gap_requires_recovery"));
    scenario.then_nonce_floor_and_queries_are(12, 5).await;
    Ok(())
}

#[tokio::test]
#[serial]
async fn nonce_alignment_and_allocation_preserve_reservations() -> anyhow::Result<()> {
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given_chain_next_nonce(Some(9));
    assert_eq!(scenario.when_collect_nonce_is_allocated().await?, 9);

    let (alignment, allocation) = tokio::join!(
        scenario.when_nonce_alignment_runs(),
        scenario.when_collect_nonce_is_allocated(),
    );

    assert_eq!(allocation?, 10);
    assert!(alignment.unwrap_err().to_string().contains("nonce_gap_requires_recovery"));
    assert_eq!(scenario.when_collect_nonce_is_allocated().await?, 11);
    scenario.then_nonce_floor_and_queries_are(11, 4).await;
    Ok(())
}

#[tokio::test]
#[serial]
async fn nonce_alignment_advances_floor_without_reserving() -> anyhow::Result<()> {
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given_chain_next_nonce(Some(9));

    assert_eq!(scenario.when_nonce_alignment_runs().await?, 9);

    scenario.then_nonce_floor_and_queries_are(8, 1).await;
    assert_eq!(scenario.when_collect_nonce_is_allocated().await?, 9);
    Ok(())
}

#[tokio::test]
#[serial]
async fn nonce_alignment_rpc_failure_preserves_floor() -> anyhow::Result<()> {
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given_chain_next_nonce(None);

    let error = scenario.when_nonce_alignment_runs().await.unwrap_err();

    assert!(error.to_string().contains("simulated nonce RPC failure"));
    scenario.then_nonce_floor_and_queries_are(0, 1).await;
    Ok(())
}

#[tokio::test]
#[serial]
async fn nonce_alignment_frozen_address_does_not_query_or_mutate() -> anyhow::Result<()> {
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given_chain_next_nonce(Some(9));
    scenario.given_nonce_allocation_frozen(true);

    let result = scenario.when_nonce_alignment_runs().await;
    scenario.given_nonce_allocation_frozen(false);

    assert!(result.unwrap_err().to_string().contains("frozen"));
    scenario.then_nonce_floor_and_queries_are(0, 0).await;
    Ok(())
}

#[tokio::test]
#[serial]
async fn ordinary_transfer_nonce_syncs_external_chain_progress() -> anyhow::Result<()> {
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given().wallet_password_cached().await;
    scenario.given_chain_next_nonce(Some(9));

    let first = scenario.when().transfer_starts().await??;
    scenario.given_chain_next_nonce(Some(15));
    let second = scenario.when().transfer_starts().await??;

    assert_eq!(first, format!("0x{:064x}", 9));
    assert_eq!(second, format!("0x{:064x}", 15));
    scenario.then_transfer_nonces_and_floor_are(&[9, 15], 15).await;
    Ok(())
}

#[tokio::test]
#[serial]
async fn ordinary_transfer_nonce_rpc_failure_recovers_without_sending() -> anyhow::Result<()> {
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given().wallet_password_cached().await;
    scenario.given_chain_next_nonce(None);

    let result = scenario.when().transfer_starts().await?;

    assert!(result.unwrap_err().to_string().contains("simulated nonce RPC failure"));
    scenario.then_nonce_floor_and_queries_are(0, 1).await;

    scenario.given_chain_next_nonce(Some(9));
    let hash = scenario.when().transfer_starts().await??;
    assert_eq!(hash, format!("0x{:064x}", 9));
    scenario.then_transfer_nonces_and_floor_are(&[9], 9).await;
    Ok(())
}

#[tokio::test]
#[serial]
async fn withdrawal_nonce_syncs_chain_progress() -> anyhow::Result<()> {
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given_chain_next_nonce(Some(9));

    let first = scenario.when_withdrawal_nonce_is_allocated().await?;
    scenario.given_chain_next_nonce(Some(15));
    let second = scenario.when_withdrawal_nonce_is_allocated().await?;

    assert_eq!([first, second], [9, 15]);
    scenario.then_nonce_floor_and_queries_are(15, 2).await;
    Ok(())
}

#[tokio::test]
#[serial]
async fn withdrawal_nonce_rpc_failure_recovers_without_reservation() -> anyhow::Result<()> {
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given_chain_next_nonce(None);

    let error = scenario.when_withdrawal_nonce_is_allocated().await.unwrap_err();

    assert!(error.to_string().contains("simulated nonce RPC failure"));
    scenario.then_nonce_floor_and_queries_are(0, 1).await;
    scenario.given_chain_next_nonce(Some(9));
    assert_eq!(scenario.when_withdrawal_nonce_is_allocated().await?, 9);
    scenario.then_nonce_floor_and_queries_are(9, 2).await;
    Ok(())
}

#[tokio::test]
#[serial]
async fn withdrawal_nonce_concurrent_collect_keeps_reservations_unique() -> anyhow::Result<()> {
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given_chain_next_nonce(Some(9));

    let (withdraw, collect) = tokio::join!(
        scenario.when_withdrawal_nonce_is_allocated(),
        scenario.when_collect_nonce_is_allocated(),
    );

    let mut allocated = [withdraw?, collect? as u64];
    allocated.sort();
    assert_eq!(allocated, [9, 10]);
    scenario.then_nonce_floor_and_queries_are(10, 2).await;
    Ok(())
}

#[tokio::test]
#[serial]
async fn collect_fee_nonce_syncs_chain_progress() -> anyhow::Result<()> {
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given_chain_next_nonce(Some(9));

    let first = scenario.when_collect_fee_nonce_is_allocated().await?;
    scenario.given_chain_next_nonce(Some(15));
    let second = scenario.when_collect_fee_nonce_is_allocated().await?;

    assert_eq!([first, second], [9, 15]);
    scenario.then_nonce_floor_and_queries_are(15, 2).await;
    Ok(())
}

#[tokio::test]
#[serial]
async fn collect_fee_nonce_rpc_failure_recovers_without_reservation() -> anyhow::Result<()> {
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given_chain_next_nonce(None);

    let error = scenario.when_collect_fee_nonce_is_allocated().await.unwrap_err();

    assert!(error.to_string().contains("simulated nonce RPC failure"));
    scenario.then_nonce_floor_and_queries_are(0, 1).await;
    scenario.given_chain_next_nonce(Some(9));
    assert_eq!(scenario.when_collect_fee_nonce_is_allocated().await?, 9);
    scenario.then_nonce_floor_and_queries_are(9, 2).await;
    Ok(())
}

#[tokio::test]
#[serial]
async fn collect_fee_nonce_shared_senders_remain_unique() -> anyhow::Result<()> {
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given_chain_next_nonce(Some(9));

    let (fee, withdraw, collect) = tokio::join!(
        scenario.when_collect_fee_nonce_is_allocated(),
        scenario.when_withdrawal_nonce_is_allocated(),
        scenario.when_collect_nonce_is_allocated(),
    );

    let mut allocated = [fee?, withdraw?, collect? as u64];
    allocated.sort();
    assert_eq!(allocated, [9, 10, 11]);
    scenario.then_nonce_floor_and_queries_are(11, 3).await;
    Ok(())
}

#[tokio::test]
#[serial]
async fn nonce_fee_rejections_preserve_reservation_without_chain_repair() -> anyhow::Result<()> {
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given_chain_next_nonce(Some(9));
    assert_eq!(scenario.when_collect_nonce_is_allocated().await?, 9);
    scenario.given_chain_next_nonce(Some(50));

    for error in ["replacement transaction underpriced", "transaction underpriced"] {
        let result = scenario.when_nonce_error_is_handled(error).await;
        assert!(result.is_err(), "fee rejection must remain an error");
        scenario.then_nonce_floor_and_queries_are(9, 1).await;
    }
    Ok(())
}

#[tokio::test]
#[serial]
async fn nonce_known_transaction_preserves_reservation_without_chain_repair() -> anyhow::Result<()>
{
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given_chain_next_nonce(Some(9));
    assert_eq!(scenario.when_collect_nonce_is_allocated().await?, 9);
    scenario.given_chain_next_nonce(Some(50));

    scenario.when_nonce_error_is_handled("already known").await?;

    scenario.then_nonce_floor_and_queries_are(9, 1).await;
    Ok(())
}

#[tokio::test]
#[serial]
async fn ordinary_transfer_uncertain_nonce_blocks_next_allocation() -> anyhow::Result<()> {
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().transfer_fails();
    scenario.given().fake_chain_adapter();
    scenario.given().wallet_password_cached().await;
    scenario.given_chain_next_nonce(Some(4));

    let first = scenario.when().transfer_fails().await;
    let second = scenario.when().transfer_fails().await;

    assert!(first.to_string().contains("simulated transfer failure"));
    assert!(second.to_string().contains("nonce_gap_requires_recovery"));
    scenario.then_transfer_nonces_and_floor_are(&[4], 4).await;
    scenario.then_durable_transfer_is(4, false).await?;

    scenario.given_transfer_can_succeed();
    scenario.given_chain_next_nonce(Some(5));
    assert_eq!(scenario.when().transfer_starts().await??, format!("0x{:064x}", 5));
    scenario.then_transfer_nonces_and_floor_are(&[4, 5], 5).await;
    scenario.then_durable_transfer_is(4, false).await?;
    scenario.then_durable_transfer_is(5, true).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn ordinary_transfer_timeout_blocks_next_allocation() -> anyhow::Result<()> {
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given().wallet_password_cached().await;
    scenario.given_chain_next_nonce(Some(4));
    scenario.given_transfer_times_out();

    let first = scenario.when().transfer_fails().await;
    let second = scenario.when().transfer_fails().await;

    assert!(first.to_string().contains("timeout"));
    assert!(second.to_string().contains("nonce_gap_requires_recovery"));
    scenario.then_transfer_nonces_and_floor_are(&[4], 4).await;
    scenario.then_durable_transfer_is(4, false).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn ordinary_transfer_signed_persistence_failure_never_sends() -> anyhow::Result<()> {
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given().wallet_password_cached().await;
    scenario.given_chain_next_nonce(Some(4));
    scenario.given_signed_persistence_fails().await?;

    let error = scenario.when().transfer_fails().await;
    let retry = scenario.when().transfer_fails().await;

    assert!(error.to_string().contains("simulated signed persistence failure"));
    assert!(retry.to_string().contains("nonce_gap_requires_recovery"));
    scenario.then_transfer_nonces_and_floor_are(&[], 4).await;
    scenario.then_no_signed_transfer_exists().await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn ordinary_transfer_invisible_submission_stays_uncertain() -> anyhow::Result<()> {
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given().wallet_password_cached().await;
    scenario.given_chain_next_nonce(Some(4));
    scenario.given_transfer_is_invisible();

    let error = scenario.when().transfer_fails().await;

    assert!(error.to_string().contains("transfer_broadcast_uncertain"));
    scenario.then_transfer_nonces_and_floor_are(&[4], 4).await;
    scenario.then_durable_transfer_is(4, false).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn ordinary_transfer_wrong_response_hash_cannot_mark_accepted() -> anyhow::Result<()> {
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given().wallet_password_cached().await;
    scenario.given_chain_next_nonce(Some(4));
    scenario.given_returned_hash_mismatches();

    let error = scenario.when().transfer_fails().await;

    assert!(error.to_string().contains("transfer_broadcast_hash_mismatch"));
    scenario.then_transfer_nonces_and_floor_are(&[4], 4).await;
    scenario.then_durable_transfer_is(4, false).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn ordinary_transfer_acceptance_write_failure_preserves_signed_transaction()
-> anyhow::Result<()> {
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given().wallet_password_cached().await;
    scenario.given_chain_next_nonce(Some(4));
    scenario.given_acceptance_persistence_fails().await?;

    let error = scenario.when().transfer_fails().await;

    assert!(error.to_string().contains("simulated acceptance persistence failure"));
    scenario.then_transfer_nonces_and_floor_are(&[4], 4).await;
    scenario.then_durable_transfer_is(4, false).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn ordinary_transfer_observer_preserves_uncertainty_until_valid_receipt() -> anyhow::Result<()>
{
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given().wallet_password_cached().await;
    scenario.given_chain_next_nonce(Some(20001));
    scenario.given_transfer_times_out();
    scenario.when().transfer_fails().await;

    for mode in [0, -1, 4, 5] {
        scenario.given_receipt_behavior(20001, mode);
        scenario.when_receipts_are_observed().await?;
        scenario.then_durable_transfer_is(20001, false).await?;
        scenario.then_transfer_nonces_and_floor_are(&[20001], 20001).await;
    }
    scenario.given_receipt_behavior(20001, 2);
    scenario.when_receipts_are_observed().await?;
    scenario.then_receipt_is_persisted(20001, true).await?;
    scenario.when_receipts_are_observed().await?;
    scenario.then_receipt_is_persisted(20001, true).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn ordinary_transfer_observer_records_onchain_failure_without_resending() -> anyhow::Result<()>
{
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given().wallet_password_cached().await;
    scenario.given_chain_next_nonce(Some(20002));
    scenario.when().transfer_starts().await??;
    scenario.given_receipt_behavior(20002, 3);

    scenario.when_receipts_are_observed().await?;

    scenario.then_receipt_is_persisted(20002, false).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn ordinary_transfer_background_observer_cancels_restarts_and_retries() -> anyhow::Result<()>
{
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given().wallet_password_cached().await;
    scenario.given_chain_next_nonce(Some(20003));
    scenario.given_transfer_times_out();
    scenario.when().transfer_fails().await;
    scenario.given_receipt_behavior(20003, -2);

    let blocked = scenario.when_observer_starts();
    scenario.then_observer_has_queried().await;
    scenario.when_observer_stops(blocked).await?;
    scenario.then_durable_transfer_is(20003, false).await?;

    scenario.given_receipt_behavior(20003, 0);
    let restarted = scenario.when_observer_starts();
    scenario.then_observer_has_queried().await;
    scenario.then_durable_transfer_is(20003, false).await?;
    scenario.given_receipt_behavior(20003, 2);
    scenario.then_background_receipt_finishes(20003).await?;
    scenario.when_observer_stops(restarted).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn ordinary_transfer_rebroadcast_uses_saved_bytes_after_cooldown() -> anyhow::Result<()> {
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given().wallet_password_cached().await;
    scenario.given_rebroadcast_fixture();
    scenario.given_chain_next_nonce(Some(30001));
    scenario.given_transfer_times_out();
    scenario.when().transfer_fails().await;
    scenario.given_transfer_can_succeed();

    scenario.when_receipts_are_observed().await?;
    scenario.then_rebroadcast_counts_are(30001, 1, 0).await?;
    scenario.given_rebroadcast_cooldown_elapsed().await?;
    scenario.when_receipts_are_observed().await?;

    scenario.then_rebroadcast_counts_are(30001, 2, 1).await?;
    scenario.then_durable_transfer_is(30001, true).await?;
    scenario.when_receipts_are_observed().await?;
    scenario.then_rebroadcast_counts_are(30001, 2, 1).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn ordinary_transfer_rebroadcast_waits_on_visibility_nonce_and_freeze() -> anyhow::Result<()>
{
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given().wallet_password_cached().await;
    scenario.given_rebroadcast_fixture();
    scenario.given_chain_next_nonce(Some(30002));
    scenario.given_transfer_times_out();
    scenario.when().transfer_fails().await;
    scenario.given_transfer_can_succeed();
    scenario.given_rebroadcast_cooldown_elapsed().await?;

    scenario.given_node_visibility(true, false);
    scenario.when_receipts_are_observed().await?;
    scenario.then_rebroadcast_counts_are(30002, 1, 0).await?;
    scenario.given_node_visibility(false, false);
    for next in [Some(30003), Some(30001), None] {
        scenario.given_chain_next_nonce(next);
        scenario.when_receipts_are_observed().await?;
        scenario.then_rebroadcast_counts_are(30002, 1, 0).await?;
    }
    scenario.given_chain_next_nonce(Some(30002));
    scenario.given_node_visibility(false, true);
    scenario.when_receipts_are_observed().await?;
    scenario.then_rebroadcast_counts_are(30002, 1, 0).await?;
    scenario.given_node_visibility(false, false);
    scenario.given_nonce_allocation_frozen(true);
    scenario.when_receipts_are_observed().await?;
    scenario.given_nonce_allocation_frozen(false);
    scenario.then_rebroadcast_counts_are(30002, 1, 0).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn ordinary_transfer_rebroadcast_timeouts_consume_bounded_budget() -> anyhow::Result<()> {
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given().wallet_password_cached().await;
    scenario.given_rebroadcast_fixture();
    scenario.given_chain_next_nonce(Some(30003));
    scenario.given_transfer_times_out();
    scenario.when().transfer_fails().await;

    for retries in 1..=3 {
        scenario.given_rebroadcast_cooldown_elapsed().await?;
        scenario.when_receipts_are_observed().await?;
        scenario.then_rebroadcast_counts_are(30003, retries as usize + 1, retries).await?;
        scenario.then_durable_transfer_is(30003, false).await?;
    }
    scenario.given_rebroadcast_cooldown_elapsed().await?;
    scenario.when_receipts_are_observed().await?;
    scenario.then_rebroadcast_counts_are(30003, 4, 3).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn ordinary_transfer_rebroadcast_rejects_corrupted_saved_raw() -> anyhow::Result<()> {
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given().wallet_password_cached().await;
    scenario.given_rebroadcast_fixture();
    scenario.given_chain_next_nonce(Some(30004));
    scenario.given_transfer_times_out();
    scenario.when().transfer_fails().await;
    scenario.given_transfer_can_succeed();
    scenario.given_rebroadcast_cooldown_elapsed().await?;
    scenario.given_saved_raw_corrupted().await?;

    scenario.when_receipts_are_observed().await?;

    scenario.then_rebroadcast_counts_are(30004, 1, 0).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn ordinary_transfer_rebroadcast_cancellation_preserves_claim_on_restart()
-> anyhow::Result<()> {
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given().wallet_password_cached().await;
    scenario.given_rebroadcast_fixture();
    scenario.given_chain_next_nonce(Some(30005));
    scenario.given_transfer_times_out();
    scenario.when().transfer_fails().await;
    scenario.given_rebroadcast_cooldown_elapsed().await?;
    scenario.given_broadcast_hangs();

    let observer = scenario.when_observer_starts();
    scenario.then_send_count_reaches(2).await;
    scenario.then_rebroadcast_counts_are(30005, 2, 1).await?;
    scenario.when_observer_stops(observer).await?;
    scenario.then_durable_transfer_is(30005, false).await?;

    scenario.given_transfer_can_succeed();
    scenario.given_receipt_behavior(30005, 0);
    let restarted = scenario.when_observer_starts();
    scenario.then_observer_has_queried().await;
    scenario.when_observer_stops(restarted).await?;
    // Also complete a full observation round: cooldown must still suppress send.
    scenario.when_receipts_are_observed().await?;
    scenario.then_rebroadcast_counts_are(30005, 2, 1).await?;

    scenario.given_rebroadcast_cooldown_elapsed().await?;
    scenario.when_receipts_are_observed().await?;
    scenario.then_rebroadcast_counts_are(30005, 3, 2).await?;
    scenario.then_durable_transfer_is(30005, true).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn ordinary_transfer_observer_failure_survives_legacy_success_projection()
-> anyhow::Result<()> {
    let scenario = TransferNonceScenario::new().await;
    scenario.given().bnb_transfer_fixture().await?;
    scenario.given().fake_chain_adapter();
    scenario.given().wallet_password_cached().await;
    scenario.given_chain_next_nonce(Some(20006));
    scenario.given_transfer_times_out();
    scenario.when().transfer_fails().await;
    scenario.given_receipt_behavior(20006, 3);
    scenario.when_receipts_are_observed().await?;
    scenario.then_receipt_is_persisted(20006, false).await?;

    scenario.given_legacy_success_overwrite().await?;

    scenario.then_bill_status_is(20006, 3).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn api_wallet_transfer_nonce_all_evm_chains_persist_send_and_confirm() -> anyhow::Result<()> {
    for chain in support::evm_chain_codes() {
        let scenario = support::EvmTransferScenario::given_chain(&chain).await?;
        scenario.when_transfer_runs().await?;
        scenario.then_signed_fact_and_send_are(true).await?;
        scenario.when_receipt_is_observed(true).await?;
        scenario.then_confirmed_without_resend(true).await?;
    }
    Ok(())
}

#[tokio::test]
#[serial]
async fn api_wallet_transfer_nonce_polygon_persistence_failure_never_broadcasts()
-> anyhow::Result<()> {
    let scenario = support::EvmTransferScenario::given_chain("polygon").await?;
    scenario.given_persistence_fails().await?;
    let err = scenario.when_transfer_runs().await.unwrap_err();
    assert!(err.to_string().contains("simulated signed persistence failure"));
    scenario.then_nothing_was_sent_or_saved().await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn api_wallet_transfer_nonce_new_evm_timeout_recovers_from_failed_receipt_without_resend()
-> anyhow::Result<()> {
    for chain in ["polygon", "avalanche", "fantom", "arbitrum", "base", "scroll"] {
        let scenario = support::EvmTransferScenario::given_chain(chain).await?;
        scenario.given_timeout();
        assert!(scenario.when_transfer_runs().await.is_err());
        scenario.then_signed_fact_and_send_are(false).await?;
        scenario.when_receipt_is_observed(false).await?;
        scenario.then_confirmed_without_resend(false).await?;
    }
    Ok(())
}
