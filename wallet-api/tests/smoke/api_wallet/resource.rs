use anyhow::Result;
use wallet_api::{
    request::api_wallet::resource::{ApiResourceStakeReq, ApiResourceType},
    testkit::env::get_manager_with_config,
};

const MANUAL_WITHDRAW_UID: &str =
    "5bdb1b748bb617d6683f57565b1493cfa5f9e45f3086daf265ca2e0cd325c15e";
const MANUAL_RESOURCE: ApiResourceType = ApiResourceType::Energy;
const MANUAL_FROZEN_BALANCE: &str = "10";
const MANUAL_PASSWORD: &str = "q1111111";

#[tokio::test]
#[ignore = "requires configured client4.toml, API-wallet backend, and real TRON staking broadcast"]
async fn stake_api_withdraw_wallet_resource_live_smoke() -> Result<()> {
    wallet_utils::init_test_log();
    let (wallet_manager, test_params) = get_manager_with_config("client4.toml").await?;
    wallet_manager.init_api_swap().await?;

    let withdraw_wallet_uid = MANUAL_WITHDRAW_UID.to_string();
    let resource = MANUAL_RESOURCE;
    let frozen_balance = MANUAL_FROZEN_BALANCE.to_string();
    let password = test_params
        .api_wallet_import
        .as_ref()
        .and_then(|config| config.withdrawal.as_ref())
        .map(|params| params.wallet_password.clone())
        .unwrap_or_else(|| {
            if test_params.create_wallet_req.wallet_password.is_empty() {
                MANUAL_PASSWORD.to_string()
            } else {
                test_params.create_wallet_req.wallet_password.clone()
            }
        });

    tracing::info!(
        withdraw_wallet_uid = %withdraw_wallet_uid,
        resource = ?resource,
        frozen_balance = %frozen_balance,
        "stake_api_withdraw_wallet_resource live smoke start"
    );

    wallet_manager.set_passwd_cache(&password).await?;
    let resp = wallet_manager
        .stake_api_withdraw_wallet_resource(ApiResourceStakeReq {
            withdraw_wallet_uid,
            resource,
            frozen_balance,
            password,
        })
        .await?;

    tracing::info!(?resp, "stake_api_withdraw_wallet_resource live smoke success");
    Ok(())
}
