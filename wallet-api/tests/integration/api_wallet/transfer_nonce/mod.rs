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

    let second = when.transfer_starts();
    then.only_first_nonce_is_recorded_while_second_waits().await;

    when.first_transfer_is_released();

    then.serial_transfer_results_are(first, second).await?;

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
