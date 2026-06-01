mod support;

use anyhow::Result;
use serial_test::serial;

use support::AcctChangeScenario;

#[tokio::test]
#[serial]
async fn acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address() -> Result<()> {
    let scenario = AcctChangeScenario::new().await;

    let fixture = scenario.given_api_wallet_sol_usdc_asset_with_symbol_mismatch().await?;

    let status = scenario.when_acct_change_payload_executes(&fixture).await?;

    scenario.then_api_wallet_acct_change_succeeds(status);
    scenario.then_api_wallet_asset_keeps_usdc_symbol(&fixture).await?;

    Ok(())
}

#[tokio::test]
#[serial]
async fn acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch() -> Result<()> {
    let scenario = AcctChangeScenario::new().await;

    let fixture = scenario.given_normal_eth_usdt_asset_with_symbol_mismatch().await?;

    let status = scenario.when_acct_change_payload_executes(&fixture).await?;

    scenario.then_normal_wallet_acct_change_succeeds(status);
    scenario.then_normal_wallet_token_asset_keeps_usdt_symbol(&fixture).await?;

    Ok(())
}

#[tokio::test]
#[serial]
async fn acct_change_normal_wallet_syncs_native_by_empty_token_when_token_missing() -> Result<()> {
    let scenario = AcctChangeScenario::new().await;

    let fixture = scenario.given_normal_eth_native_asset_with_missing_token().await?;

    let status = scenario.when_acct_change_payload_executes(&fixture).await?;

    scenario.then_normal_wallet_native_acct_change_succeeds(status);
    scenario.then_normal_wallet_native_asset_keeps_eth_symbol(&fixture).await?;

    Ok(())
}
