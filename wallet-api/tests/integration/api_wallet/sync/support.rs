use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use alloy::primitives::U256;
use chrono::Utc;
use wallet_api::{
    domain::api_wallet::Tx,
    error::service::ServiceError,
    testkit::adapter_factory::{
        clear_test_transaction_adapter_override, set_test_transaction_adapter_override,
    },
};
use wallet_chain_interact::{Error as ChainError, QueryTransactionResult};
use wallet_database::{
    entities::{
        api_account::CreateApiAccountVo,
        api_assets::{ApiAssetsEntity, ApiCreateAssetsVo},
        api_chain::{ApiChainCreateVo, NodeBindType},
        api_coin::ApiCoinData,
        api_wallet::ApiWalletType,
        asset_token_key::AssetTokenKey,
        assets::AssetsId,
    },
    repositories::api_wallet::{
        account::ApiAccountRepo, assets::ApiAssetsRepo, chain::ApiChainRepo, coin::ApiCoinRepo,
    },
};

use crate::harness::{
    TestEnv, ensure_env, next_tag, open_api_wallet_pool, reset_fake, upsert_wallet,
};

const CHAIN_CODE: &str = "bnb";

pub(super) struct SyncAssetsScenario {
    env: &'static TestEnv,
    calls: Option<Arc<AtomicUsize>>,
    _guard: Option<AdapterGuard>,
}

impl SyncAssetsScenario {
    pub(super) async fn new() -> Self {
        let env = ensure_env().await;
        reset_fake(env);

        Self { env, calls: None, _guard: None }
    }

    pub(super) async fn given_withdrawal_bnb_asset(&self) -> anyhow::Result<SyncAssetsFixture> {
        self.given_bnb_asset(ApiWalletType::Withdrawal, "api-wallet").await
    }

    pub(super) async fn given_subaccount_bnb_asset(&self) -> anyhow::Result<SyncAssetsFixture> {
        self.given_bnb_asset(ApiWalletType::SubAccount, "api-wallet-sub").await
    }

    pub(super) fn given_chain_balance(&mut self, balance: u64) {
        self.install_balance_adapter(U256::from(balance), false);
    }

    pub(super) fn given_chain_balance_query_fails(&mut self) {
        self.install_balance_adapter(U256::from(123u64), true);
    }

    pub(super) async fn when_sync_api_assets_by_wallet_runs(
        &self,
        fixture: &SyncAssetsFixture,
    ) -> Result<(), ServiceError> {
        self.env
            .manager
            .sync_api_assets_by_wallet(fixture.wallet_address.clone(), Some(1), vec![])
            .await
    }

    pub(super) async fn when_sync_api_assets_by_wallet_returns(
        &self,
        fixture: &SyncAssetsFixture,
    ) -> Result<(), ServiceError> {
        self.when_sync_api_assets_by_wallet_runs(fixture).await
    }

    pub(super) fn then_sync_result_is_ok(&self, result: Result<(), ServiceError>) {
        assert!(result.is_ok());
    }

    pub(super) fn then_chain_balance_was_queried_once(&self) {
        assert_eq!(self.call_count(), 1);
    }

    pub(super) fn then_chain_balance_was_not_queried(&self) {
        assert_eq!(self.call_count(), 0);
    }

    pub(super) async fn then_asset_balance_is_chain_balance(
        &self,
        fixture: &SyncAssetsFixture,
        balance: u64,
    ) -> anyhow::Result<()> {
        let saved = self.load_asset(fixture).await?;
        let expected = wallet_utils::unit::format_to_string(U256::from(balance), 18)?;
        assert_eq!(saved.balance, expected);
        Ok(())
    }

    pub(super) async fn then_asset_balance_is_zero(
        &self,
        fixture: &SyncAssetsFixture,
    ) -> anyhow::Result<()> {
        let saved = self.load_asset(fixture).await?;
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

    fn install_balance_adapter(&mut self, balance: U256, fail: bool) {
        let adapter = MockBalanceAdapter::new(balance, fail);
        self.calls = Some(adapter.calls.clone());
        self._guard = Some(install_adapter(CHAIN_CODE, adapter));
    }

    fn call_count(&self) -> usize {
        self.calls.as_ref().expect("balance adapter installed").load(Ordering::SeqCst)
    }

    async fn load_asset(&self, fixture: &SyncAssetsFixture) -> anyhow::Result<ApiAssetsEntity> {
        let api_pool = open_api_wallet_pool(&self.env.db_dir).await;
        let saved = ApiAssetsRepo::find_by_id(
            &api_pool,
            &AssetsId::new(&fixture.account_address, CHAIN_CODE, AssetTokenKey::Native),
        )
        .await?
        .expect("asset should exist");

        Ok(saved)
    }
}

pub(super) struct SyncAssetsFixture {
    wallet_address: String,
    account_address: String,
}

#[derive(Clone)]
struct MockBalanceAdapter {
    balance: U256,
    fail: bool,
    calls: Arc<AtomicUsize>,
}

impl MockBalanceAdapter {
    fn new(balance: U256, fail: bool) -> Self {
        Self { balance, fail, calls: Arc::new(AtomicUsize::new(0)) }
    }
}

#[async_trait::async_trait]
impl Tx for MockBalanceAdapter {
    async fn account_resource(
        &self,
        _owner_address: &str,
    ) -> Result<wallet_chain_interact::tron::protocol::account::AccountResourceDetail, ServiceError>
    {
        unimplemented!()
    }

    async fn balance_token_key(
        &self,
        _addr: &str,
        _token: AssetTokenKey,
    ) -> Result<U256, ChainError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            Err(ChainError::TransportError(wallet_transport::errors::TransportError::EmptyResult))
        } else {
            Ok(self.balance)
        }
    }

    async fn nonce(&self, _addr: &str) -> Result<u64, ServiceError> {
        Ok(0)
    }

    async fn block_num(&self) -> Result<u64, ChainError> {
        Ok(0)
    }

    async fn query_tx_res(
        &self,
        _hash: &str,
    ) -> Result<Option<QueryTransactionResult>, ChainError> {
        Ok(None)
    }

    async fn token_symbol(&self, _token: &str) -> Result<String, ChainError> {
        Ok("BNB".to_string())
    }

    async fn token_name(&self, _token: &str) -> Result<String, ChainError> {
        Ok("BNB Smart Chain".to_string())
    }

    async fn decimals(&self, _token: &str) -> Result<u8, ChainError> {
        Ok(18)
    }

    async fn black_address(&self, _token: &str, _owner: &str) -> Result<bool, ServiceError> {
        Ok(false)
    }

    async fn transfer(
        &self,
        _params: &wallet_api::request::api_wallet::trans::ApiTransferReq,
        _private_key: wallet_chain_interact::types::ChainPrivateKey,
    ) -> Result<wallet_api::domain::chain::TransferResp, ServiceError> {
        unimplemented!()
    }

    async fn estimate_fee(
        &self,
        _req: wallet_api::request::api_wallet::trans::ApiBaseTransferReq,
        _main_symbol: &str,
    ) -> Result<String, ServiceError> {
        Ok("0".to_string())
    }

    async fn build_transfer_raw(
        &self,
        _params: &wallet_api::request::api_wallet::trans::ApiTransferReq,
        _private_key: wallet_chain_interact::types::ChainPrivateKey,
    ) -> Result<(String, wallet_api::domain::api_wallet::RawTx, String), ServiceError> {
        unimplemented!()
    }

    async fn broadcast_transfer(
        &self,
        _raw: wallet_api::domain::api_wallet::RawTx,
    ) -> Result<wallet_api::domain::chain::TransferResp, ServiceError> {
        unimplemented!()
    }
}

struct AdapterGuard {
    chain_code: String,
}

impl Drop for AdapterGuard {
    fn drop(&mut self) {
        clear_test_transaction_adapter_override(&self.chain_code);
    }
}

fn install_adapter(chain_code: &str, adapter: MockBalanceAdapter) -> AdapterGuard {
    let adapter: Arc<dyn Tx + Send + Sync> = Arc::new(adapter);
    set_test_transaction_adapter_override(chain_code, adapter);
    AdapterGuard { chain_code: chain_code.to_string() }
}

async fn prepare_wallet_fixture(
    env: &TestEnv,
    wallet_uid: &str,
    account_address: &str,
    token_address: AssetTokenKey,
    wallet_type: ApiWalletType,
) -> anyhow::Result<String> {
    let api_pool = open_api_wallet_pool(&env.db_dir).await;
    let now = Utc::now();

    ApiChainRepo::add(
        &api_pool,
        ApiChainCreateVo::new(
            "BNB Smart Chain",
            CHAIN_CODE,
            &["m/44'/60'/0'/0".to_string()],
            NodeBindType::AutoBackend,
            "BNB",
        ),
    )
    .await?;

    ApiCoinRepo::upsert_multi_coin(
        &api_pool,
        vec![ApiCoinData::new(
            Some("BNB Smart Chain".to_string()),
            "BNB",
            CHAIN_CODE,
            token_address.clone(),
            Some("0".to_string()),
            None,
            18,
            1,
            1,
            1,
            now,
            Some(now),
        )],
    )
    .await?;

    let wallet_address = upsert_wallet(&env.db_dir, "sn-sync", wallet_uid, wallet_type, None).await;

    let account = CreateApiAccountVo::new(
        1,
        account_address,
        "pubkey",
        &wallet_address,
        wallet_uid,
        "m/44'/60'/0'/0/0",
        0,
        CHAIN_CODE,
        "account",
        wallet_type,
    )
    .with_is_init(true);
    ApiAccountRepo::upsert_account_multi(&api_pool, vec![account]).await?;

    let asset = ApiCreateAssetsVo::new(
        AssetsId::new(account_address, CHAIN_CODE, token_address),
        "BNB",
        18,
        None,
        0,
    )
    .with_name("BNB")
    .with_balance("0");
    ApiAssetsRepo::upsert_assets_multi(&api_pool, vec![asset]).await?;

    Ok(wallet_address)
}
