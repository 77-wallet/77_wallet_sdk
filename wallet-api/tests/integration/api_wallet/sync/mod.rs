mod support;

use serial_test::serial;

use support::SyncAssetsScenario;

#[tokio::test]
#[serial]
async fn sync_api_assets_by_wallet_updates_api_assets_from_chain() -> anyhow::Result<()> {
    let mut scenario = SyncAssetsScenario::new().await;

    let fixture = scenario.given_withdrawal_bnb_asset().await?;
    scenario.given_chain_balance(123);

    scenario.when_sync_api_assets_by_wallet_runs(&fixture).await?;

    scenario.then_chain_balance_was_queried_once();
    scenario.then_asset_balance_is_chain_balance(&fixture, 123).await?;

    Ok(())
}

#[tokio::test]
#[serial]
async fn sync_api_assets_by_wallet_keeps_balance_when_chain_query_fails() -> anyhow::Result<()> {
    let mut scenario = SyncAssetsScenario::new().await;

    let fixture = scenario.given_withdrawal_bnb_asset().await?;
    scenario.given_chain_balance_query_fails();

    let result = scenario.when_sync_api_assets_by_wallet_returns(&fixture).await;

    scenario.then_sync_result_is_ok(result);
    scenario.then_chain_balance_was_queried_once();
    scenario.then_asset_balance_is_zero(&fixture).await?;

    Ok(())
}

#[tokio::test]
#[serial]
async fn sync_api_assets_by_wallet_skips_subaccount_wallet() -> anyhow::Result<()> {
    let mut scenario = SyncAssetsScenario::new().await;

    let fixture = scenario.given_subaccount_bnb_asset().await?;
    scenario.given_chain_balance(123);

    let result = scenario.when_sync_api_assets_by_wallet_returns(&fixture).await;

    scenario.then_sync_result_is_ok(result);
    scenario.then_chain_balance_was_not_queried();
    scenario.then_asset_balance_is_zero(&fixture).await?;

    Ok(())
}
