mod support;

use serial_test::serial;

use support::TransferNonceScenario;

#[tokio::test]
#[serial]
async fn api_wallet_transfer_nonce_lock_keeps_same_address_requests_serial() -> anyhow::Result<()> {
    let mut scenario = TransferNonceScenario::new().await;

    scenario.given_bnb_transfer_fixture().await?;
    scenario.given_first_transfer_blocks();
    scenario.given_fake_chain_adapter();
    scenario.given_wallet_password_cached().await;

    let first = scenario.when_transfer_starts();
    scenario.then_first_transfer_has_entered().await;

    let second = scenario.when_transfer_starts();
    scenario.then_only_first_nonce_is_recorded_while_second_waits().await;

    scenario.when_first_transfer_is_released();

    scenario.then_serial_transfer_results_are(first, second).await?;

    Ok(())
}

#[tokio::test]
#[serial]
async fn api_wallet_transfer_nonce_failure_keeps_reserved_nonce() -> anyhow::Result<()> {
    let mut scenario = TransferNonceScenario::new().await;

    scenario.given_bnb_transfer_fixture().await?;
    scenario.given_transfer_fails();
    scenario.given_fake_chain_adapter();
    scenario.given_wallet_password_cached().await;

    let err = scenario.when_transfer_fails().await;

    scenario.then_failure_keeps_reserved_nonce(err).await?;

    Ok(())
}
