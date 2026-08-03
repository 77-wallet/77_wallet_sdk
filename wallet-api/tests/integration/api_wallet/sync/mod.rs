mod support;

use serial_test::serial;

use support::{ScenarioRoles, SyncAssetsGiven, SyncAssetsScenario, SyncAssetsThen, SyncAssetsWhen};

#[tokio::test]
#[serial]
async fn sync_api_assets_by_wallet_updates_api_assets_from_chain() -> anyhow::Result<()> {
    let scenario = SyncAssetsScenario::new().await;
    let given = scenario.given();
    let when = scenario.when();
    let then = scenario.then();

    let fixture = given.withdrawal_bnb_asset().await?;
    given.chain_balance(123);

    when.sync_api_assets_by_wallet_runs(&fixture).await?;

    then.chain_balance_was_queried_once();
    then.asset_balance_is_chain_balance(&fixture, 123).await?;

    Ok(())
}

#[tokio::test]
#[serial]
async fn sync_api_assets_by_wallet_keeps_balance_when_chain_query_fails() -> anyhow::Result<()> {
    let scenario = SyncAssetsScenario::new().await;
    let given = scenario.given();
    let when = scenario.when();
    let then = scenario.then();

    let fixture = given.withdrawal_bnb_asset().await?;
    given.chain_balance_query_fails();

    let result = when.sync_api_assets_by_wallet_returns(&fixture).await;

    then.sync_result_is_ok(result);
    then.chain_balance_was_queried_once();
    then.asset_balance_is_zero(&fixture).await?;

    Ok(())
}

#[tokio::test]
#[serial]
async fn sync_api_assets_by_wallet_skips_subaccount_wallet() -> anyhow::Result<()> {
    let scenario = SyncAssetsScenario::new().await;
    let given = scenario.given();
    let when = scenario.when();
    let then = scenario.then();

    let fixture = given.subaccount_bnb_asset().await?;
    given.chain_balance(123);

    let result = when.sync_api_assets_by_wallet_returns(&fixture).await;

    then.sync_result_is_ok(result);
    then.chain_balance_was_not_queried();
    then.asset_balance_is_zero(&fixture).await?;

    Ok(())
}
