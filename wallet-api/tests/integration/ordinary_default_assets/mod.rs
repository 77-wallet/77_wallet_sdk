mod support;
use serial_test::serial;
use support::OrdinaryDefaultAssetsScenario;

#[tokio::test]
#[serial]
async fn ordinary_default_assets_wait_for_last_page_then_fill_and_repeat_safely()
-> anyhow::Result<()> {
    let scenario = OrdinaryDefaultAssetsScenario::new().await?;
    scenario.given_two_successful_pages();
    let gate = scenario.given_last_page_paused();
    let task = scenario.when_sync_starts();
    gate.wait_for_request().await;
    scenario.then_contract_assets_are(0).await?;
    gate.release();
    task.await??;
    scenario.then_contract_assets_are(2).await?;
    scenario.then_api_assets_are_unchanged().await?;
    scenario.then_refresh_signals_are(1);
    scenario.given_two_successful_pages();
    scenario.when_sync_runs().await?;
    scenario.then_contract_assets_are(2).await?;
    scenario.then_only_catalog_endpoints_were_called();
    scenario.then_refresh_signals_are(0);
    Ok(())
}

#[tokio::test]
#[serial]
async fn ordinary_default_assets_failed_later_page_writes_no_partial_assets_or_coins()
-> anyhow::Result<()> {
    let scenario = OrdinaryDefaultAssetsScenario::new().await?;
    scenario.given_failed_last_page();
    assert!(scenario.when_sync_runs().await.is_err());
    scenario.then_contract_assets_are(0).await?;
    scenario.then_contract_coins_are(0).await?;
    scenario.given_two_successful_pages();
    scenario.when_sync_runs().await?;
    scenario.then_contract_assets_are(2).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn ordinary_default_assets_failed_catalog_completion_defers_fill_until_retry()
-> anyhow::Result<()> {
    let scenario = OrdinaryDefaultAssetsScenario::new().await?;
    scenario.given_failed_catalog_completion();
    assert!(scenario.when_sync_runs().await.is_err());
    scenario.then_contract_coins_are(2).await?;
    scenario.then_contract_assets_are(0).await?;
    scenario.given_two_successful_pages();
    scenario.when_sync_runs().await?;
    scenario.then_contract_assets_are(2).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn ordinary_default_assets_cancelled_last_page_does_not_write_or_notify() -> anyhow::Result<()>
{
    let scenario = OrdinaryDefaultAssetsScenario::new().await?;
    scenario.given_two_successful_pages();
    let gate = scenario.given_last_page_paused();
    let task = scenario.when_sync_starts();
    gate.wait_for_request().await;
    task.abort();
    gate.release();
    assert!(task.await.unwrap_err().is_cancelled());
    scenario.then_contract_coins_are(0).await?;
    scenario.then_contract_assets_are(0).await?;
    scenario.then_refresh_signals_are(0);
    Ok(())
}
