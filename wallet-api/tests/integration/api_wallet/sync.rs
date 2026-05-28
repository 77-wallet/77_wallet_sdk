use crate::harness::{ensure_env, next_tag, open_api_wallet_pool, reset_fake, upsert_wallet};
use chrono::Utc;
use serial_test::serial;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use wallet_api::{
    domain::api_wallet::Tx,
    test_support::adapter_factory::{
        clear_test_transaction_adapter_override, set_test_transaction_adapter_override,
    },
};
use wallet_chain_interact::{Error as ChainError, QueryTransactionResult};
use wallet_database::{
    entities::{
        api_account::CreateApiAccountVo,
        api_assets::ApiCreateAssetsVo,
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

#[derive(Clone)]
struct MockBalanceAdapter {
    balance: alloy::primitives::U256,
    fail: bool,
    calls: Arc<AtomicUsize>,
}

impl MockBalanceAdapter {
    fn new(balance: alloy::primitives::U256, fail: bool) -> Self {
        Self { balance, fail, calls: Arc::new(AtomicUsize::new(0)) }
    }
}

#[async_trait::async_trait]
impl Tx for MockBalanceAdapter {
    async fn account_resource(
        &self,
        _owner_address: &str,
    ) -> Result<
        wallet_chain_interact::tron::protocol::account::AccountResourceDetail,
        wallet_api::error::service::ServiceError,
    > {
        unimplemented!()
    }

    async fn balance_token_key(
        &self,
        _addr: &str,
        _token: AssetTokenKey,
    ) -> Result<alloy::primitives::U256, ChainError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            Err(ChainError::TransportError(wallet_transport::errors::TransportError::EmptyResult))
        } else {
            Ok(self.balance)
        }
    }

    async fn nonce(&self, _addr: &str) -> Result<u64, wallet_api::error::service::ServiceError> {
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

    async fn black_address(
        &self,
        _token: &str,
        _owner: &str,
    ) -> Result<bool, wallet_api::error::service::ServiceError> {
        Ok(false)
    }

    async fn transfer(
        &self,
        _params: &wallet_api::request::api_wallet::trans::ApiTransferReq,
        _private_key: wallet_chain_interact::types::ChainPrivateKey,
    ) -> Result<wallet_api::domain::chain::TransferResp, wallet_api::error::service::ServiceError>
    {
        unimplemented!()
    }

    async fn estimate_fee(
        &self,
        _req: wallet_api::request::api_wallet::trans::ApiBaseTransferReq,
        _main_symbol: &str,
    ) -> Result<String, wallet_api::error::service::ServiceError> {
        Ok("0".to_string())
    }

    async fn build_transfer_raw(
        &self,
        _params: &wallet_api::request::api_wallet::trans::ApiTransferReq,
        _private_key: wallet_chain_interact::types::ChainPrivateKey,
    ) -> Result<
        (String, wallet_api::domain::api_wallet::RawTx, String),
        wallet_api::error::service::ServiceError,
    > {
        unimplemented!()
    }

    async fn broadcast_transfer(
        &self,
        _raw: wallet_api::domain::api_wallet::RawTx,
    ) -> Result<wallet_api::domain::chain::TransferResp, wallet_api::error::service::ServiceError>
    {
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
    db_dir: &std::path::Path,
    wallet_uid: &str,
    account_address: &str,
    chain_code: &str,
    token_address: AssetTokenKey,
    wallet_type: ApiWalletType,
) -> anyhow::Result<String> {
    let api_pool = open_api_wallet_pool(db_dir).await;
    let now = Utc::now();

    ApiChainRepo::add(
        &api_pool,
        ApiChainCreateVo::new(
            "BNB Smart Chain",
            chain_code,
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
            chain_code,
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

    let wallet_address = upsert_wallet(db_dir, "sn-sync", wallet_uid, wallet_type, None).await;

    let account = CreateApiAccountVo::new(
        1,
        account_address,
        "pubkey",
        &wallet_address,
        wallet_uid,
        "m/44'/60'/0'/0/0",
        0,
        chain_code,
        "account",
        wallet_type,
    )
    .with_is_init(true);
    ApiAccountRepo::upsert_account_multi(&api_pool, vec![account]).await?;

    let asset = ApiCreateAssetsVo::new(
        AssetsId::new(account_address, chain_code, token_address),
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

#[tokio::test]
#[serial]
async fn sync_api_assets_by_wallet_updates_api_assets_from_chain() -> anyhow::Result<()> {
    let env = ensure_env().await;
    reset_fake(env);

    let wallet_uid = next_tag("api-wallet");
    let account_address = format!("0x{}", next_tag("acct"));
    let wallet_address = prepare_wallet_fixture(
        &env.db_dir,
        &wallet_uid,
        &account_address,
        "bnb",
        AssetTokenKey::Native,
        ApiWalletType::Withdrawal,
    )
    .await?;

    let adapter = MockBalanceAdapter::new(alloy::primitives::U256::from(123u64), false);
    let calls = adapter.calls.clone();
    let _guard = install_adapter("bnb", adapter);

    env.manager.sync_api_assets_by_wallet(wallet_address.clone(), Some(1), vec![]).await?;
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let api_pool = open_api_wallet_pool(&env.db_dir).await;
    let saved = ApiAssetsRepo::find_by_id(
        &api_pool,
        &AssetsId::new(&account_address, "bnb", AssetTokenKey::Native),
    )
    .await?
    .expect("asset should exist");

    let expected = wallet_utils::unit::format_to_string(alloy::primitives::U256::from(123u64), 18)?;
    assert_eq!(saved.balance, expected);

    Ok(())
}

#[tokio::test]
#[serial]
async fn sync_api_assets_by_wallet_keeps_balance_when_chain_query_fails() -> anyhow::Result<()> {
    let env = ensure_env().await;
    reset_fake(env);

    let wallet_uid = next_tag("api-wallet-fail");
    let account_address = format!("0x{}", next_tag("acct"));
    let wallet_address = prepare_wallet_fixture(
        &env.db_dir,
        &wallet_uid,
        &account_address,
        "bnb",
        AssetTokenKey::Native,
        ApiWalletType::Withdrawal,
    )
    .await?;

    let adapter = MockBalanceAdapter::new(alloy::primitives::U256::from(123u64), true);
    let calls = adapter.calls.clone();
    let _guard = install_adapter("bnb", adapter);

    let res = env.manager.sync_api_assets_by_wallet(wallet_address.clone(), Some(1), vec![]).await;
    assert!(res.is_ok());
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let api_pool = open_api_wallet_pool(&env.db_dir).await;
    let saved = ApiAssetsRepo::find_by_id(
        &api_pool,
        &AssetsId::new(&account_address, "bnb", AssetTokenKey::Native),
    )
    .await?
    .expect("asset should exist");

    assert_eq!(saved.balance, "0");
    Ok(())
}

#[tokio::test]
#[serial]
async fn sync_api_assets_by_wallet_skips_subaccount_wallet() -> anyhow::Result<()> {
    let env = ensure_env().await;
    reset_fake(env);

    let wallet_uid = next_tag("api-wallet-sub");
    let account_address = format!("0x{}", next_tag("acct"));
    let wallet_address = prepare_wallet_fixture(
        &env.db_dir,
        &wallet_uid,
        &account_address,
        "bnb",
        AssetTokenKey::Native,
        ApiWalletType::SubAccount,
    )
    .await?;

    let adapter = MockBalanceAdapter::new(alloy::primitives::U256::from(123u64), false);
    let calls = adapter.calls.clone();
    let _guard = install_adapter("bnb", adapter);

    let res = env.manager.sync_api_assets_by_wallet(wallet_address.clone(), Some(1), vec![]).await;
    assert!(res.is_ok());
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    let api_pool = open_api_wallet_pool(&env.db_dir).await;
    let saved = ApiAssetsRepo::find_by_id(
        &api_pool,
        &AssetsId::new(&account_address, "bnb", AssetTokenKey::Native),
    )
    .await?
    .expect("asset should exist");

    assert_eq!(saved.balance, "0");
    Ok(())
}
