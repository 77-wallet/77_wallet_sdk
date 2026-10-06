mod support;
use serial_test::serial;
use support::ChainSyncScenario;

#[tokio::test]
#[serial]
async fn valid_backend_chain_response_wins_over_dev_mainnet_defaults() -> anyhow::Result<()> {
    let scenario = ChainSyncScenario::new().await?;
    scenario.given_backend_disables_polygon_and_gates_base();
    scenario.when_chain_sync_runs().await?;
    scenario.then_disabled_chains_stay_disabled_after_reinitialize().await?;
    scenario.then_one_chain_request_was_sent();
    Ok(())
}

#[tokio::test]
#[serial]
async fn coin_protocol_failure_is_visible_and_valid_empty_response_can_fill_dev_native_coins()
-> anyhow::Result<()> {
    let scenario = ChainSyncScenario::new().await?;
    scenario.given_no_coins_and_invalid_coin_response().await?;
    let error = scenario.when_coin_sync_runs().await.unwrap_err();
    scenario.then_coin_failure_has_no_default_write(error).await?;
    scenario.given_valid_empty_coin_response();
    scenario.when_coin_sync_runs().await?;
    scenario.then_five_native_coins_are_present().await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn failed_address_registration_remains_retryable_and_stale_epoch_does_not_call_backend()
-> anyhow::Result<()> {
    let scenario = ChainSyncScenario::new().await?;
    scenario.given_five_unregistered_addresses_and_backend_failure().await?;
    scenario.when_address_registration_runs(777).await.unwrap_err();
    scenario.then_all_addresses_have_init_state(0).await?;
    scenario.given_backend_registration_success();
    scenario.when_address_registration_runs(777).await?;
    scenario.then_all_addresses_have_init_state(1).await?;
    scenario.then_registration_call_count_is(2);
    scenario.when_address_registration_runs(776).await?;
    scenario.then_registration_call_count_is(2);
    Ok(())
}
