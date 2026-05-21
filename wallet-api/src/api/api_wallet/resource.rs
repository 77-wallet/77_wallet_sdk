use crate::{
    api::ReturnType,
    manager::WalletManager,
    request::api_wallet::resource::{ApiResourceStakeReq, ApiResourceUnstakeReq},
    response_vo::api_wallet::resource::ApiResourceOperationResp,
    service::api_wallet::resource::ApiResourceService,
};

impl WalletManager {
    pub async fn stake_api_withdraw_wallet_resource(
        &self,
        req: ApiResourceStakeReq,
    ) -> ReturnType<ApiResourceOperationResp> {
        ApiResourceService::new(self.ctx).stake_withdraw_wallet_resource(req).await
    }

    pub async fn unstake_api_withdraw_wallet_resource(
        &self,
        req: ApiResourceUnstakeReq,
    ) -> ReturnType<ApiResourceOperationResp> {
        ApiResourceService::new(self.ctx).unstake_withdraw_wallet_resource(req).await
    }
}

#[cfg(all(test, feature = "integration-tests"))]
mod tests {
    use super::*;
    use crate::{
        request::api_wallet::resource::ApiResourceType, test::env::get_manager_with_config,
    };
    use anyhow::{Context, Result};
    use wallet_database::{
        entities::api_wallet::ApiWalletType, repositories::api_wallet::wallet::ApiWalletRepo,
    };

    const MANUAL_WITHDRAW_UID: &str =
        "5bdb1b748bb617d6683f57565b1493cfa5f9e45f3086daf265ca2e0cd325c15e";
    const MANUAL_RESOURCE: ApiResourceType = ApiResourceType::Energy;
    const MANUAL_FROZEN_BALANCE: &str = "1000";
    const MANUAL_PASSWORD: &str = "q1111111";

    #[tokio::test]
    #[ignore = "manual test: broadcasts a real TRON stake transaction"]
    async fn manual_stake_api_withdraw_wallet_resource() -> Result<()> {
        wallet_utils::init_test_log();
        let (wallet_manager, test_params) = get_manager_with_config("client4.toml").await?;
        wallet_manager.init_api_swap().await?;

        let withdraw_wallet_uid = manual_withdraw_wallet_uid().await?;
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
            "manual stake_api_withdraw_wallet_resource start"
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

        tracing::info!(?resp, "manual stake_api_withdraw_wallet_resource success");
        Ok(())
    }

    async fn manual_withdraw_wallet_uid() -> Result<String> {
        if !MANUAL_WITHDRAW_UID.trim().is_empty() {
            return Ok(MANUAL_WITHDRAW_UID.to_string());
        }

        let pool =
            crate::context::CONTEXT.get().context("context not initialized")?.api_wallet_pool()?;
        let wallets = ApiWalletRepo::list(&pool, Some(ApiWalletType::Withdrawal)).await?;
        wallets
            .into_iter()
            .next()
            .map(|wallet| wallet.uid)
            .context("no local withdrawal API wallet found")
    }
}
