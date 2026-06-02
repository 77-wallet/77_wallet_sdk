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
