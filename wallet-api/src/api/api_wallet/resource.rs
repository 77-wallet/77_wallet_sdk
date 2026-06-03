use crate::{
    api::ReturnType,
    manager::WalletManager,
    request::stake::{FreezeBalanceReq, UnFreezeBalanceReq, VoteWitnessReq, WithdrawBalanceReq},
    response_vo::standard_wallet::stake::{FreezeResp, VoteListResp, VoterInfoResp},
    service::api_wallet::resource::ApiResourceService,
};

impl WalletManager {
    pub async fn stake_api_withdraw_wallet_resource(
        &self,
        req: FreezeBalanceReq,
        password: String,
    ) -> ReturnType<FreezeResp> {
        ApiResourceService::new(self.ctx).stake_withdraw_wallet_resource(req, password).await
    }

    pub async fn unstake_api_withdraw_wallet_resource(
        &self,
        req: UnFreezeBalanceReq,
        password: String,
    ) -> ReturnType<FreezeResp> {
        ApiResourceService::new(self.ctx).unstake_withdraw_wallet_resource(req, password).await
    }

    pub async fn api_withdraw_wallet_votes(
        &self,
        req: VoteWitnessReq,
        password: &str,
    ) -> ReturnType<String> {
        ApiResourceService::new(self.ctx).withdraw_wallet_votes(req, password).await
    }

    pub async fn api_withdraw_wallet_voter_info(
        &self,
        owner_address: &str,
    ) -> ReturnType<VoterInfoResp> {
        ApiResourceService::new(self.ctx).withdraw_wallet_voter_info(owner_address).await
    }

    pub async fn api_withdraw_wallet_votes_node_list(
        &self,
        owner_address: Option<&str>,
    ) -> ReturnType<VoteListResp> {
        ApiResourceService::new(self.ctx).withdraw_wallet_votes_node_list(owner_address).await
    }

    pub async fn api_withdraw_wallet_claim_votes_rewards(
        &self,
        req: WithdrawBalanceReq,
        password: &str,
    ) -> ReturnType<String> {
        ApiResourceService::new(self.ctx).withdraw_wallet_claim_votes_rewards(req, password).await
    }
}

#[cfg(all(test, feature = "integration-tests"))]
mod tests {
    use super::*;
    use crate::{request::stake::FreezeBalanceReq, testkit::env::get_manager_with_config};
    use anyhow::{Context, Result};
    use wallet_database::{
        entities::api_wallet::ApiWalletType,
        repositories::api_wallet::{account::ApiAccountRepo, wallet::ApiWalletRepo},
    };

    const MANUAL_WITHDRAW_UID: &str =
        "5bdb1b748bb617d6683f57565b1493cfa5f9e45f3086daf265ca2e0cd325c15e";
    const MANUAL_RESOURCE: &str = "energy";
    const MANUAL_FROZEN_BALANCE: i64 = 10;
    const MANUAL_PASSWORD: &str = "q1111111";

    #[tokio::test]
    #[ignore = "manual test: broadcasts a real TRON stake transaction"]
    async fn manual_stake_api_withdraw_wallet_resource() -> Result<()> {
        wallet_utils::init_test_log();
        let (wallet_manager, test_params) = get_manager_with_config("client4.toml").await?;
        wallet_manager.init_api_swap().await?;

        let withdraw_wallet_uid = manual_withdraw_wallet_uid().await?;
        let owner_address = manual_withdraw_wallet_owner_address(&withdraw_wallet_uid).await?;
        let resource = MANUAL_RESOURCE.to_string();
        let frozen_balance = MANUAL_FROZEN_BALANCE;
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
            owner_address = %owner_address,
            resource = %resource,
            frozen_balance = frozen_balance,
            "manual stake_api_withdraw_wallet_resource start"
        );

        wallet_manager.set_passwd_cache(&password).await?;
        let resp = wallet_manager
            .stake_api_withdraw_wallet_resource(
                FreezeBalanceReq { owner_address, resource, frozen_balance, signer: None },
                password,
            )
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

    async fn manual_withdraw_wallet_owner_address(uid: &str) -> Result<String> {
        let pool =
            crate::context::CONTEXT.get().context("context not initialized")?.api_wallet_pool()?;
        let wallet = ApiWalletRepo::find_by_uid(&pool, uid)
            .await?
            .context("manual withdrawal wallet uid not found")?;
        let accounts =
            ApiAccountRepo::find_all_by_wallet_address_index(&pool, &wallet.address, "tron", 1)
                .await?;
        accounts
            .into_iter()
            .next()
            .map(|account| account.address)
            .context("manual withdrawal wallet tron account not found")
    }
}
