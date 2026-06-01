use alloy::primitives::U256;
use wallet_api::error::service::ServiceError;
use wallet_database::entities::{api_wallet::ApiWalletType, asset_token_key::AssetTokenKey};

use crate::harness::{TestEnv, ensure_env, next_tag, reset_fake};

use super::{
    adapter::{InstalledBalanceAdapter, install_balance_adapter},
    db::{load_native_asset, prepare_wallet_fixture},
    fixtures::SyncAssetsFixture,
};

pub(crate) struct SyncAssetsScenario {
    env: &'static TestEnv,
    adapter: Option<InstalledBalanceAdapter>,
}

impl SyncAssetsScenario {
    pub(crate) async fn new() -> Self {
        let env = ensure_env().await;
        reset_fake(env);

        Self { env, adapter: None }
    }

    pub(crate) async fn given_withdrawal_bnb_asset(&self) -> anyhow::Result<SyncAssetsFixture> {
        self.given_bnb_asset(ApiWalletType::Withdrawal, "api-wallet").await
    }

    pub(crate) async fn given_subaccount_bnb_asset(&self) -> anyhow::Result<SyncAssetsFixture> {
        self.given_bnb_asset(ApiWalletType::SubAccount, "api-wallet-sub").await
    }

    pub(crate) fn given_chain_balance(&mut self, balance: u64) {
        self.adapter = Some(install_balance_adapter(U256::from(balance), false));
    }

    pub(crate) fn given_chain_balance_query_fails(&mut self) {
        self.adapter = Some(install_balance_adapter(U256::from(123u64), true));
    }

    pub(crate) async fn when_sync_api_assets_by_wallet_runs(
        &self,
        fixture: &SyncAssetsFixture,
    ) -> Result<(), ServiceError> {
        self.env
            .manager
            .sync_api_assets_by_wallet(fixture.wallet_address.clone(), Some(1), vec![])
            .await
    }

    pub(crate) async fn when_sync_api_assets_by_wallet_returns(
        &self,
        fixture: &SyncAssetsFixture,
    ) -> Result<(), ServiceError> {
        self.when_sync_api_assets_by_wallet_runs(fixture).await
    }

    pub(crate) fn then_sync_result_is_ok(&self, result: Result<(), ServiceError>) {
        assert!(result.is_ok());
    }

    pub(crate) fn then_chain_balance_was_queried_once(&self) {
        assert_eq!(self.call_count(), 1);
    }

    pub(crate) fn then_chain_balance_was_not_queried(&self) {
        assert_eq!(self.call_count(), 0);
    }

    pub(crate) async fn then_asset_balance_is_chain_balance(
        &self,
        fixture: &SyncAssetsFixture,
        balance: u64,
    ) -> anyhow::Result<()> {
        let saved = load_native_asset(self.env, fixture).await?;
        let expected = wallet_utils::unit::format_to_string(U256::from(balance), 18)?;
        assert_eq!(saved.balance, expected);
        Ok(())
    }

    pub(crate) async fn then_asset_balance_is_zero(
        &self,
        fixture: &SyncAssetsFixture,
    ) -> anyhow::Result<()> {
        let saved = load_native_asset(self.env, fixture).await?;
        assert_eq!(saved.balance, "0");
        Ok(())
    }

    async fn given_bnb_asset(
        &self,
        wallet_type: ApiWalletType,
        wallet_uid_prefix: &str,
    ) -> anyhow::Result<SyncAssetsFixture> {
        let wallet_uid = next_tag(wallet_uid_prefix);
        let account_address = format!("0x{}", next_tag("acct"));
        let wallet_address = prepare_wallet_fixture(
            self.env,
            &wallet_uid,
            &account_address,
            AssetTokenKey::Native,
            wallet_type,
        )
        .await?;

        Ok(SyncAssetsFixture { wallet_address, account_address })
    }

    fn call_count(&self) -> usize {
        self.adapter.as_ref().expect("balance adapter installed").call_count()
    }
}
