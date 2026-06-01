use anyhow::Result;
use wallet_api::{manager::WalletManager, testkit::mqtt::exec_wallet_order_payload};

use crate::get_manager;

use super::{
    db::{
        assert_api_wallet_asset_symbol, assert_normal_wallet_asset_symbol, ensure_eth_chain_active,
        ensure_sol_chain_active, seed_api_wallet_sol_usdc_asset, seed_normal_eth_native_asset,
        seed_normal_eth_usdt_asset,
    },
    fixtures::AcctChangeFixture,
    task::wait_task_done,
};

pub(crate) struct AcctChangeScenario {
    _manager: WalletManager,
}

impl AcctChangeScenario {
    pub(crate) async fn new() -> Self {
        Self { _manager: get_manager().await }
    }

    pub(crate) async fn given_api_wallet_sol_usdc_asset_with_symbol_mismatch(
        &self,
    ) -> Result<AcctChangeFixture> {
        ensure_sol_chain_active().await?;
        let fixture = AcctChangeFixture::api_wallet_sol_usdc_symbol_mismatch();
        seed_api_wallet_sol_usdc_asset(&fixture).await?;
        Ok(fixture)
    }

    pub(crate) async fn given_normal_eth_usdt_asset_with_symbol_mismatch(
        &self,
    ) -> Result<AcctChangeFixture> {
        ensure_eth_chain_active().await?;
        let fixture = AcctChangeFixture::normal_eth_usdt_symbol_mismatch();
        seed_normal_eth_usdt_asset(&fixture).await?;
        Ok(fixture)
    }

    pub(crate) async fn given_normal_eth_native_asset_with_missing_token(
        &self,
    ) -> Result<AcctChangeFixture> {
        ensure_eth_chain_active().await?;
        let fixture = AcctChangeFixture::normal_eth_native_missing_token();
        seed_normal_eth_native_asset(&fixture).await?;
        Ok(fixture)
    }

    pub(crate) async fn when_acct_change_payload_executes(
        &self,
        fixture: &AcctChangeFixture,
    ) -> Result<u8> {
        exec_wallet_order_payload(&fixture.payload).await?;
        wait_task_done(&fixture.msg_id).await
    }

    pub(crate) fn then_api_wallet_acct_change_succeeds(&self, status: u8) {
        assert_eq!(status, 2, "ApiWalletAcctChange task should succeed");
    }

    pub(crate) fn then_normal_wallet_acct_change_succeeds(&self, status: u8) {
        assert_eq!(status, 2, "AcctChange task should succeed");
    }

    pub(crate) fn then_normal_wallet_native_acct_change_succeeds(&self, status: u8) {
        assert_eq!(status, 2, "AcctChange native task should succeed");
    }

    pub(crate) async fn then_api_wallet_asset_keeps_usdc_symbol(
        &self,
        fixture: &AcctChangeFixture,
    ) -> Result<()> {
        assert_api_wallet_asset_symbol(fixture, "USDC").await
    }

    pub(crate) async fn then_normal_wallet_token_asset_keeps_usdt_symbol(
        &self,
        fixture: &AcctChangeFixture,
    ) -> Result<()> {
        assert_normal_wallet_asset_symbol(fixture, "USDT").await
    }

    pub(crate) async fn then_normal_wallet_native_asset_keeps_eth_symbol(
        &self,
        fixture: &AcctChangeFixture,
    ) -> Result<()> {
        assert_normal_wallet_asset_symbol(fixture, "ETH").await
    }
}
