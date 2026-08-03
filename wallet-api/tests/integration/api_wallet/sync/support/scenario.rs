use std::cell::RefCell;

use alloy::primitives::U256;
use wallet_api::error::service::ServiceError;
use wallet_database::entities::{
    api_assets::ApiAssetsEntity, api_wallet::ApiWalletType, asset_token_key::AssetTokenKey,
};

use crate::harness::{
    AssertRole, CountRole, GivenRole, LoadRole, SeedRole, TestEnv, ThenRole, WhenRole, ensure_env,
    next_tag, reset_fake,
};

use super::{
    adapter::{InstalledBalanceAdapter, install_balance_adapter},
    db::{load_native_asset, prepare_wallet_fixture},
    fixtures::SyncAssetsFixture,
};

pub(crate) struct SyncAssetsScenario {
    env: &'static TestEnv,
    adapter: RefCell<Option<InstalledBalanceAdapter>>,
}

impl SyncAssetsScenario {
    pub(crate) async fn new() -> Self {
        let env = ensure_env().await;
        reset_fake(env);

        Self { env, adapter: RefCell::new(None) }
    }

    fn seed(&self) -> SeedRole<'_, Self> {
        SeedRole::new(self)
    }

    fn load(&self) -> LoadRole<'_, Self> {
        LoadRole::new(self)
    }

    fn count(&self) -> CountRole<'_, Self> {
        CountRole::new(self)
    }

    fn assert(&self) -> AssertRole<'_, Self> {
        AssertRole::new(self)
    }
}

#[async_trait::async_trait(?Send)]
pub(crate) trait SyncAssetsGiven {
    async fn withdrawal_bnb_asset(&self) -> anyhow::Result<SyncAssetsFixture>;

    async fn subaccount_bnb_asset(&self) -> anyhow::Result<SyncAssetsFixture>;

    fn chain_balance(&self, balance: u64);

    fn chain_balance_query_fails(&self);
}

#[async_trait::async_trait(?Send)]
impl SyncAssetsGiven for GivenRole<'_, SyncAssetsScenario> {
    async fn withdrawal_bnb_asset(&self) -> anyhow::Result<SyncAssetsFixture> {
        self.scenario().seed().bnb_asset(ApiWalletType::Withdrawal, "api-wallet").await
    }

    async fn subaccount_bnb_asset(&self) -> anyhow::Result<SyncAssetsFixture> {
        self.scenario().seed().bnb_asset(ApiWalletType::SubAccount, "api-wallet-sub").await
    }

    fn chain_balance(&self, balance: u64) {
        self.scenario().seed().chain_balance(balance);
    }

    fn chain_balance_query_fails(&self) {
        self.scenario().seed().chain_balance_query_fails();
    }
}

#[async_trait::async_trait(?Send)]
trait SyncAssetsSeed {
    async fn bnb_asset(
        &self,
        wallet_type: ApiWalletType,
        wallet_uid_prefix: &str,
    ) -> anyhow::Result<SyncAssetsFixture>;

    fn chain_balance(&self, balance: u64);

    fn chain_balance_query_fails(&self);
}

#[async_trait::async_trait(?Send)]
impl SyncAssetsSeed for SeedRole<'_, SyncAssetsScenario> {
    async fn bnb_asset(
        &self,
        wallet_type: ApiWalletType,
        wallet_uid_prefix: &str,
    ) -> anyhow::Result<SyncAssetsFixture> {
        let wallet_uid = next_tag(wallet_uid_prefix);
        let account_address = format!("0x{}", next_tag("acct"));
        let wallet_address = prepare_wallet_fixture(
            self.scenario().env,
            &wallet_uid,
            &account_address,
            AssetTokenKey::Native,
            wallet_type,
        )
        .await?;

        Ok(SyncAssetsFixture { wallet_address, account_address })
    }

    fn chain_balance(&self, balance: u64) {
        self.scenario().adapter.replace(Some(install_balance_adapter(U256::from(balance), false)));
    }

    fn chain_balance_query_fails(&self) {
        self.scenario().adapter.replace(Some(install_balance_adapter(U256::from(123u64), true)));
    }
}

#[async_trait::async_trait(?Send)]
pub(crate) trait SyncAssetsWhen {
    async fn sync_api_assets_by_wallet_runs(
        &self,
        fixture: &SyncAssetsFixture,
    ) -> Result<(), ServiceError>;

    async fn sync_api_assets_by_wallet_returns(
        &self,
        fixture: &SyncAssetsFixture,
    ) -> Result<(), ServiceError>;
}

#[async_trait::async_trait(?Send)]
impl SyncAssetsWhen for WhenRole<'_, SyncAssetsScenario> {
    async fn sync_api_assets_by_wallet_runs(
        &self,
        fixture: &SyncAssetsFixture,
    ) -> Result<(), ServiceError> {
        self.scenario()
            .env
            .manager
            .sync_api_assets_by_wallet(fixture.wallet_address.clone(), Some(1), vec![])
            .await
    }

    async fn sync_api_assets_by_wallet_returns(
        &self,
        fixture: &SyncAssetsFixture,
    ) -> Result<(), ServiceError> {
        self.sync_api_assets_by_wallet_runs(fixture).await
    }
}

#[async_trait::async_trait(?Send)]
pub(crate) trait SyncAssetsThen {
    fn sync_result_is_ok(&self, result: Result<(), ServiceError>);

    fn chain_balance_was_queried_once(&self);

    fn chain_balance_was_not_queried(&self);

    async fn asset_balance_is_chain_balance(
        &self,
        fixture: &SyncAssetsFixture,
        balance: u64,
    ) -> anyhow::Result<()>;

    async fn asset_balance_is_zero(&self, fixture: &SyncAssetsFixture) -> anyhow::Result<()>;
}

#[async_trait::async_trait(?Send)]
impl SyncAssetsThen for ThenRole<'_, SyncAssetsScenario> {
    fn sync_result_is_ok(&self, result: Result<(), ServiceError>) {
        self.scenario().assert().sync_result_is_ok(result);
    }

    fn chain_balance_was_queried_once(&self) {
        let call_count = self.scenario().count().chain_balance_calls();
        self.scenario().assert().chain_balance_was_queried_once(call_count);
    }

    fn chain_balance_was_not_queried(&self) {
        let call_count = self.scenario().count().chain_balance_calls();
        self.scenario().assert().chain_balance_was_not_queried(call_count);
    }

    async fn asset_balance_is_chain_balance(
        &self,
        fixture: &SyncAssetsFixture,
        balance: u64,
    ) -> anyhow::Result<()> {
        let saved = self.scenario().load().native_asset(fixture).await?;
        self.scenario().assert().asset_balance_is_chain_balance(&saved.balance, balance)
    }

    async fn asset_balance_is_zero(&self, fixture: &SyncAssetsFixture) -> anyhow::Result<()> {
        let saved = self.scenario().load().native_asset(fixture).await?;
        self.scenario().assert().asset_balance_is_zero(&saved.balance);
        Ok(())
    }
}

#[async_trait::async_trait(?Send)]
trait SyncAssetsLoad {
    async fn native_asset(&self, fixture: &SyncAssetsFixture) -> anyhow::Result<ApiAssetsEntity>;
}

#[async_trait::async_trait(?Send)]
impl SyncAssetsLoad for LoadRole<'_, SyncAssetsScenario> {
    async fn native_asset(&self, fixture: &SyncAssetsFixture) -> anyhow::Result<ApiAssetsEntity> {
        load_native_asset(self.scenario().env, fixture).await
    }
}

trait SyncAssetsCount {
    fn chain_balance_calls(&self) -> usize;
}

impl SyncAssetsCount for CountRole<'_, SyncAssetsScenario> {
    fn chain_balance_calls(&self) -> usize {
        self.scenario().adapter.borrow().as_ref().expect("balance adapter installed").call_count()
    }
}

trait SyncAssetsAssert {
    fn sync_result_is_ok(&self, result: Result<(), ServiceError>);

    fn chain_balance_was_queried_once(&self, call_count: usize);

    fn chain_balance_was_not_queried(&self, call_count: usize);

    fn asset_balance_is_chain_balance(
        &self,
        saved_balance: &str,
        balance: u64,
    ) -> anyhow::Result<()>;

    fn asset_balance_is_zero(&self, saved_balance: &str);
}

impl SyncAssetsAssert for AssertRole<'_, SyncAssetsScenario> {
    fn sync_result_is_ok(&self, result: Result<(), ServiceError>) {
        assert!(result.is_ok());
    }

    fn chain_balance_was_queried_once(&self, call_count: usize) {
        assert_eq!(call_count, 1);
    }

    fn chain_balance_was_not_queried(&self, call_count: usize) {
        assert_eq!(call_count, 0);
    }

    fn asset_balance_is_chain_balance(
        &self,
        saved_balance: &str,
        balance: u64,
    ) -> anyhow::Result<()> {
        let expected = wallet_utils::unit::format_to_string(U256::from(balance), 18)?;
        assert_eq!(saved_balance, expected);
        Ok(())
    }

    fn asset_balance_is_zero(&self, saved_balance: &str) {
        assert_eq!(saved_balance, "0");
    }
}
