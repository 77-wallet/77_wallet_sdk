use crate::harness::{
    WorkerTestEnv, decrypt_captured_api_backend_body, next_unique_id, open_api_wallet_pool,
    upsert_wallet,
};
use alloy::primitives::U256;
use chrono::Utc;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::mpsc;
use wallet_api::{
    domain::{
        api_wallet::{RawTx, Tx},
        chain::adapter::sol_tx::TOKEN_ACCOUNT_RENT,
    },
    error::business::{
        BusinessError,
        chain::{ChainError, InsufficientBalanceDetail},
    },
    infrastructure::api_trans::{AddressLockManager, ShadowAdvancer, ShadowCollectWorker},
    testkit::{
        adapter_factory::{
            clear_test_transaction_adapter_override, set_test_transaction_adapter_override,
        },
        collect::upload_collect_service_fee_via_worker,
    },
};
use wallet_database::{
    ApiTransactionDbPool, ApiWalletDbPool, SqliteContext,
    entities::{
        api_account::CreateApiAccountVo,
        api_coin::ApiCoinData,
        api_collect::{ApiCollectEntity, ApiCollectStatus},
        api_wallet::ApiWalletType,
        api_withdraw_strategy::ApiWithdrawStrategyEntity,
        api_withdraw_strategy_chain_config::ApiWithdrawStrategyChainConfigEntity,
        asset_token_key::AssetTokenKey,
    },
    repositories::api_wallet::{
        account::ApiAccountRepo, coin::ApiCoinRepo, collect::ApiCollectRepo,
        withdraw_strategy::ApiWithdrawStrategyRepo,
        withdraw_strategy_chain_config::ApiWithdrawStrategyChainConfigRepo,
    },
};
use wallet_types::chain::chain::ChainCode;

#[derive(Clone)]
struct CollectSolTestAdapter {
    recipient_missing: bool,
    force_fee_insufficient: bool,
    balance: u64,
    fee: f64,
}

#[async_trait::async_trait]
impl Tx for CollectSolTestAdapter {
    async fn account_resource(
        &self,
        _owner_address: &str,
    ) -> Result<
        wallet_chain_interact::tron::protocol::account::AccountResourceDetail,
        wallet_api::error::service::ServiceError,
    > {
        unimplemented!("not used in collect fee checks")
    }

    async fn balance_token_key(
        &self,
        _addr: &str,
        _token: AssetTokenKey,
    ) -> Result<U256, wallet_chain_interact::Error> {
        Ok(U256::from(self.balance))
    }

    async fn nonce(&self, _addr: &str) -> Result<u64, wallet_api::error::service::ServiceError> {
        Ok(0)
    }

    async fn block_num(&self) -> Result<u64, wallet_chain_interact::Error> {
        Ok(0)
    }

    async fn query_tx_res(
        &self,
        _hash: &str,
    ) -> Result<Option<wallet_chain_interact::QueryTransactionResult>, wallet_chain_interact::Error>
    {
        Ok(None)
    }

    async fn token_symbol(&self, _token: &str) -> Result<String, wallet_chain_interact::Error> {
        Ok("SOL".to_string())
    }

    async fn token_name(&self, _token: &str) -> Result<String, wallet_chain_interact::Error> {
        Ok("Solana".to_string())
    }

    async fn decimals(&self, _token: &str) -> Result<u8, wallet_chain_interact::Error> {
        Ok(9)
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
        unimplemented!("not used in collect fee checks")
    }

    async fn estimate_fee(
        &self,
        req: wallet_api::request::api_wallet::trans::ApiBaseTransferReq,
        _main_symbol: &str,
    ) -> Result<String, wallet_api::error::service::ServiceError> {
        if self.force_fee_insufficient {
            return Err(wallet_api::error::service::ServiceError::Business(BusinessError::Chain(
                ChainError::InsufficientFeeBalance,
            )));
        }

        if self.recipient_missing {
            return Err(wallet_api::error::service::ServiceError::Business(
                BusinessError::Chain(ChainError::insufficient_balance_with_detail(
                    InsufficientBalanceDetail::new()
                        .from_addr(req.from)
                        .to_addr(req.to)
                        .chain_code("sol".to_string())
                        .value(req.value)
                        .balance(self.balance.to_string())
                        .need("990880".to_string())
                        .reason(
                            "recipient account is not initialized and transfer amount is below rent-exempt minimum",
                        ),
                )),
            ));
        }

        Ok(json!({
            "estimateFee": {
                "amount": format!("{}", self.fee),
                "currency": "USD",
                "unitPrice": 0.0,
                "fiatValue": 0.0
            }
        })
        .to_string())
    }

    async fn estimate_fee_without_balance_check(
        &self,
        _req: wallet_api::request::api_wallet::trans::ApiBaseTransferReq,
        _main_symbol: &str,
    ) -> Result<String, wallet_api::error::service::ServiceError> {
        Ok(json!({
            "estimateFee": {
                "amount": format!("{}", self.fee),
                "currency": "USD",
                "unitPrice": 0.0,
                "fiatValue": 0.0
            }
        })
        .to_string())
    }

    async fn recipient_ata_rent(
        &self,
        _req: &wallet_api::request::api_wallet::trans::ApiBaseTransferReq,
    ) -> Result<u64, wallet_api::error::service::ServiceError> {
        Ok(if self.recipient_missing { TOKEN_ACCOUNT_RENT } else { 0 })
    }

    async fn build_transfer_raw(
        &self,
        _params: &wallet_api::request::api_wallet::trans::ApiTransferReq,
        _private_key: wallet_chain_interact::types::ChainPrivateKey,
    ) -> Result<(String, RawTx, String), wallet_api::error::service::ServiceError> {
        unimplemented!("not used in collect fee checks")
    }

    async fn broadcast_transfer(
        &self,
        _raw: RawTx,
    ) -> Result<wallet_api::domain::chain::TransferResp, wallet_api::error::service::ServiceError>
    {
        unimplemented!("not used in collect fee checks")
    }
}

#[derive(Clone)]
struct CollectEthTestAdapter {
    balance_wei: U256,
    fee_amount: f64,
}

impl CollectEthTestAdapter {
    fn fee_json(&self) -> String {
        json!({
            "default": "propose",
            "data": [{
                "type": "propose",
                "estimateFee": {
                    "amount": format!("{}", self.fee_amount),
                    "currency": "USD",
                    "unitPrice": 0.0,
                    "fiatValue": 0.0
                },
                "maxFee": {
                    "amount": format!("{}", self.fee_amount * 1.2),
                    "currency": "USD",
                    "unitPrice": 0.0,
                    "fiatValue": 0.0
                },
                "feeSetting": {
                    "gasLimit": 23100,
                    "baseFee": "1000000000",
                    "priorityFee": "1000000000",
                    "maxFeePerGas": "2000000000"
                }
            }]
        })
        .to_string()
    }
}

#[async_trait::async_trait]
impl Tx for CollectEthTestAdapter {
    async fn account_resource(
        &self,
        _owner_address: &str,
    ) -> Result<
        wallet_chain_interact::tron::protocol::account::AccountResourceDetail,
        wallet_api::error::service::ServiceError,
    > {
        unimplemented!("not used in collect fee checks")
    }

    async fn balance_token_key(
        &self,
        _addr: &str,
        _token: AssetTokenKey,
    ) -> Result<U256, wallet_chain_interact::Error> {
        Ok(self.balance_wei)
    }

    async fn nonce(&self, _addr: &str) -> Result<u64, wallet_api::error::service::ServiceError> {
        Ok(0)
    }

    async fn block_num(&self) -> Result<u64, wallet_chain_interact::Error> {
        Ok(0)
    }

    async fn query_tx_res(
        &self,
        _hash: &str,
    ) -> Result<Option<wallet_chain_interact::QueryTransactionResult>, wallet_chain_interact::Error>
    {
        Ok(None)
    }

    async fn token_symbol(&self, _token: &str) -> Result<String, wallet_chain_interact::Error> {
        Ok("ETH".to_string())
    }

    async fn token_name(&self, _token: &str) -> Result<String, wallet_chain_interact::Error> {
        Ok("Ethereum".to_string())
    }

    async fn decimals(&self, _token: &str) -> Result<u8, wallet_chain_interact::Error> {
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
        unimplemented!("not used in collect fee checks")
    }

    async fn estimate_fee(
        &self,
        _req: wallet_api::request::api_wallet::trans::ApiBaseTransferReq,
        _main_symbol: &str,
    ) -> Result<String, wallet_api::error::service::ServiceError> {
        Ok(self.fee_json())
    }

    async fn build_transfer_raw(
        &self,
        _params: &wallet_api::request::api_wallet::trans::ApiTransferReq,
        _private_key: wallet_chain_interact::types::ChainPrivateKey,
    ) -> Result<(String, RawTx, String), wallet_api::error::service::ServiceError> {
        unimplemented!("not used in collect fee checks")
    }

    async fn broadcast_transfer(
        &self,
        _raw: RawTx,
    ) -> Result<wallet_api::domain::chain::TransferResp, wallet_api::error::service::ServiceError>
    {
        unimplemented!("not used in collect fee checks")
    }
}

pub(super) struct TestAdapterGuard {
    chain_code: String,
}

impl Drop for TestAdapterGuard {
    fn drop(&mut self) {
        clear_test_transaction_adapter_override(&self.chain_code);
    }
}

pub(super) fn install_collect_test_adapter(
    recipient_missing: bool,
    balance: u64,
) -> TestAdapterGuard {
    let chain_code = ChainCode::Solana.to_string();
    let adapter = Arc::new(CollectSolTestAdapter {
        recipient_missing,
        force_fee_insufficient: false,
        balance,
        fee: 0.000015,
    });
    let tx_adapter: Arc<dyn Tx + Send + Sync> = adapter;
    set_test_transaction_adapter_override(&chain_code, tx_adapter);
    TestAdapterGuard { chain_code }
}

pub(super) fn install_collect_test_adapter_fee_shortage(
    recipient_missing: bool,
    balance: u64,
) -> TestAdapterGuard {
    let chain_code = ChainCode::Solana.to_string();
    let adapter = Arc::new(CollectSolTestAdapter {
        recipient_missing,
        force_fee_insufficient: true,
        balance,
        fee: 0.000015,
    });
    let tx_adapter: Arc<dyn Tx + Send + Sync> = adapter;
    set_test_transaction_adapter_override(&chain_code, tx_adapter);
    TestAdapterGuard { chain_code }
}

pub(super) struct EthAdapterGuard {
    chain_code: String,
}

impl Drop for EthAdapterGuard {
    fn drop(&mut self) {
        clear_test_transaction_adapter_override(&self.chain_code);
    }
}

pub(super) fn install_collect_eth_test_adapter(
    balance_wei: U256,
    fee_amount: f64,
) -> EthAdapterGuard {
    let chain_code = ChainCode::Ethereum.to_string();
    let adapter = Arc::new(CollectEthTestAdapter { balance_wei, fee_amount });
    let tx_adapter: Arc<dyn Tx + Send + Sync> = adapter;
    set_test_transaction_adapter_override(&chain_code, tx_adapter);
    EthAdapterGuard { chain_code }
}

pub(super) async fn build_shadow_collect_worker(env: &WorkerTestEnv) -> ShadowCollectWorker {
    let collect_pool_ctx =
        SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
            .await
            .expect("open api transaction sqlite");
    let collect_pool = collect_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let core_pool = open_api_wallet_pool(&env.db_dir).await;
    ensure_sol_main_coin(&core_pool).await;
    let (intent_tx, _intent_rx) = mpsc::channel(1);
    let advancer = Arc::new(ShadowAdvancer::new(collect_pool.clone(), intent_tx.clone(), None));

    ShadowCollectWorker::new(collect_pool, core_pool, Arc::new(AddressLockManager::new()), advancer)
}

pub(super) async fn ensure_eth_main_coin(pool: &ApiWalletDbPool) {
    let now = Utc::now();
    let coin = ApiCoinData::new(
        Some("Ethereum".to_string()),
        "ETH",
        "eth",
        AssetTokenKey::Native,
        Some("0".to_string()),
        None,
        18,
        1,
        1,
        1,
        now,
        Some(now),
    );
    ApiCoinRepo::upsert_multi_coin(pool, vec![coin]).await.expect("seed eth main coin");
}

pub(super) async fn build_eth_shadow_collect_worker(env: &WorkerTestEnv) -> ShadowCollectWorker {
    let collect_pool_ctx =
        SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
            .await
            .expect("open api transaction sqlite");
    let collect_pool = collect_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let core_pool = open_api_wallet_pool(&env.db_dir).await;
    ensure_eth_main_coin(&core_pool).await;
    let (intent_tx, _intent_rx) = mpsc::channel(1);
    let advancer = Arc::new(ShadowAdvancer::new(collect_pool.clone(), intent_tx.clone(), None));

    ShadowCollectWorker::new(collect_pool, core_pool, Arc::new(AddressLockManager::new()), advancer)
}

pub(super) async fn ensure_sol_main_coin(pool: &ApiWalletDbPool) {
    let now = Utc::now();
    let coin = ApiCoinData::new(
        Some("Solana".to_string()),
        "SOL",
        "sol",
        AssetTokenKey::Native,
        Some("0".to_string()),
        None,
        9,
        1,
        1,
        1,
        now,
        Some(now),
    );
    ApiCoinRepo::upsert_multi_coin(pool, vec![coin]).await.expect("seed sol main coin");
}

pub(super) async fn seed_collect_order(
    pool: &ApiTransactionDbPool,
    trade_no: &str,
    to_addr: &str,
) -> ApiCollectEntity {
    ApiCollectRepo::upsert_api_collect(
        pool,
        "uid",
        "collect",
        "from-sol",
        to_addr,
        "0.000015",
        "digest",
        "sol",
        None,
        "SOL",
        trade_no,
        2,
        ApiCollectStatus::Init,
        1,
    )
    .await
    .expect("insert collect");

    ApiCollectRepo::get_api_collect_by_trade_no(pool, trade_no).await.expect("load collect")
}

pub(super) async fn seed_eth_collect_order(
    pool: &ApiTransactionDbPool,
    trade_no: &str,
    from_addr: &str,
    to_addr: &str,
    value: &str,
) -> ApiCollectEntity {
    ApiCollectRepo::upsert_api_collect(
        pool,
        "uid",
        "collect",
        from_addr,
        to_addr,
        value,
        "digest",
        "eth",
        None,
        "ETH",
        trade_no,
        2,
        ApiCollectStatus::Init,
        1,
    )
    .await
    .expect("insert collect");

    ApiCollectRepo::get_api_collect_by_trade_no(pool, trade_no).await.expect("load collect")
}

pub(super) struct ServiceFeeUploadScenario {
    pub collect_pool: ApiTransactionDbPool,
    pub core_pool: ApiWalletDbPool,
    pub trade_no: String,
    pub from_addr: String,
    pub to_addr: String,
    pub token_code: &'static str,
    pub contract_address: &'static str,
}

pub(super) async fn given_sol_service_fee_upload_waiting(
    env: &WorkerTestEnv,
    trade_prefix: &str,
) -> ServiceFeeUploadScenario {
    let collect_pool = open_transaction_pool(env).await;
    let core_pool = open_api_wallet_pool(&env.db_dir).await;
    ensure_sol_main_coin(&core_pool).await;
    seed_sol_usdc_coin(&core_pool).await;

    let trade_no = format!("{trade_prefix}_{}", next_unique_id());
    let collect_uid = format!("collect-uid-{}", next_unique_id());
    let withdrawal_uid = format!("withdraw-uid-{}", next_unique_id());
    let from_addr = "DLcQZyqoL7ghnENR4mboeuivCNAKXBWJ8RKQA9aK3ZW8";
    let to_addr = "72vgdLcQgdudUiGXudHNPhgCPNPCdxj2ijAGuXTQ5ppB";
    let usdc_mint = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";

    seed_service_fee_wallet_account_and_strategy(
        env,
        &core_pool,
        &collect_uid,
        &withdrawal_uid,
        from_addr,
        to_addr,
        "sol",
        "m/44'/501'/0'/0/0",
    )
    .await;

    ApiCollectRepo::upsert_api_collect(
        &collect_pool,
        &collect_uid,
        "collect",
        from_addr,
        to_addr,
        "1.1",
        "digest",
        "sol",
        Some(usdc_mint.to_string()),
        "USDC",
        &trade_no,
        2,
        ApiCollectStatus::InsufficientBalance,
        1,
    )
    .await
    .expect("seed collect row");
    mark_collect_waiting_for_service_fee(&collect_pool, &trade_no, "seed fee-wait row").await;

    ServiceFeeUploadScenario {
        collect_pool,
        core_pool,
        trade_no,
        from_addr: from_addr.to_string(),
        to_addr: to_addr.to_string(),
        token_code: "SOL",
        contract_address: "",
    }
}

pub(super) async fn given_eth_service_fee_upload_waiting(
    env: &WorkerTestEnv,
) -> ServiceFeeUploadScenario {
    let collect_pool = open_transaction_pool(env).await;
    let core_pool = open_api_wallet_pool(&env.db_dir).await;
    ensure_eth_main_coin(&core_pool).await;

    let trade_no = format!("T_collect_eth_fee_upload_{}", next_unique_id());
    let collect_uid = format!("collect-uid-{}", next_unique_id());
    let withdrawal_uid = format!("withdraw-uid-{}", next_unique_id());
    let from_addr = "0xFCa230313618af2a33fa00455D8A5d1466C91332";
    let to_addr = "0x477000C778C66FaAA36596Fb846Ce34C89bc652D";

    seed_service_fee_wallet_account_and_strategy(
        env,
        &core_pool,
        &collect_uid,
        &withdrawal_uid,
        from_addr,
        to_addr,
        "eth",
        "m/44'/60'/0'/0/0",
    )
    .await;
    seed_eth_collect_order(&collect_pool, &trade_no, from_addr, to_addr, "0.00078558").await;
    mark_collect_waiting_for_service_fee(&collect_pool, &trade_no, "seed eth fee-wait row").await;

    ServiceFeeUploadScenario {
        collect_pool,
        core_pool,
        trade_no,
        from_addr: from_addr.to_string(),
        to_addr: to_addr.to_string(),
        token_code: "ETH",
        contract_address: "",
    }
}

pub(super) async fn when_upload_collect_service_fee(
    scenario: &ServiceFeeUploadScenario,
    expect_msg: &str,
) {
    upload_collect_service_fee_via_worker(
        scenario.collect_pool.clone(),
        scenario.core_pool.clone(),
        &scenario.trade_no,
    )
    .await
    .expect(expect_msg);
}

pub(super) fn then_service_fee_upload_payload(env: &WorkerTestEnv, trade_no: &str) -> Value {
    let requests = env.recorder.snapshot();
    let request = requests
        .iter()
        .find(|req| {
            req.path.contains(
                wallet_transport_backend::consts::endpoint::api_wallet::TRANS_SERVICE_FEE_TRANS,
            ) && decrypt_captured_api_backend_body(&req.body)["tradeNo"].as_str() == Some(trade_no)
        })
        .unwrap_or_else(|| {
            panic!(
                "service fee upload must call the fee-trans endpoint, captured paths: {:?}",
                requests.iter().map(|req| req.path.clone()).collect::<Vec<_>>()
            )
        });

    decrypt_captured_api_backend_body(&request.body)
}

async fn open_transaction_pool(env: &WorkerTestEnv) -> ApiTransactionDbPool {
    let collect_pool_ctx =
        SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
            .await
            .expect("open api transaction sqlite");
    collect_pool_ctx.into_transaction_db_pool().expect("transaction pool")
}

async fn seed_sol_usdc_coin(core_pool: &ApiWalletDbPool) {
    let now = Utc::now();
    ApiCoinRepo::upsert_multi_coin(
        core_pool,
        vec![ApiCoinData::new(
            Some("Solana".to_string()),
            "USDC",
            "sol",
            AssetTokenKey::Contract("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string()),
            Some("0".to_string()),
            None,
            6,
            1,
            1,
            1,
            now,
            Some(now),
        )],
    )
    .await
    .expect("seed sol usdc coin");
}

async fn seed_service_fee_wallet_account_and_strategy(
    env: &WorkerTestEnv,
    core_pool: &ApiWalletDbPool,
    collect_uid: &str,
    withdrawal_uid: &str,
    from_addr: &str,
    to_addr: &str,
    chain_code: &str,
    derivation_path: &str,
) {
    let withdrawal_wallet =
        upsert_wallet(&env.db_dir, "sn-collect", withdrawal_uid, ApiWalletType::Withdrawal, None)
            .await;
    let subaccount_wallet = upsert_wallet(
        &env.db_dir,
        "sn-collect",
        collect_uid,
        ApiWalletType::SubAccount,
        Some(&withdrawal_wallet),
    )
    .await;

    let account = CreateApiAccountVo::new(
        1,
        from_addr,
        "pubkey",
        &subaccount_wallet,
        collect_uid,
        derivation_path,
        0,
        chain_code,
        "account",
        ApiWalletType::SubAccount,
    )
    .with_is_init(true);
    ApiAccountRepo::upsert_account_multi(core_pool, vec![account])
        .await
        .expect("seed collect account");

    let withdraw_strategy = ApiWithdrawStrategyEntity {
        id: 0,
        uid: withdrawal_uid.to_string(),
        threshold: 50,
        created_at: Utc::now(),
        updated_at: None,
    };
    ApiWithdrawStrategyRepo::upsert(core_pool, withdraw_strategy)
        .await
        .expect("seed withdraw strategy");
    let withdraw_strategy_id = ApiWithdrawStrategyRepo::get_by_uid(core_pool, withdrawal_uid)
        .await
        .expect("load withdraw strategy")
        .expect("withdraw strategy exists")
        .id;
    ApiWithdrawStrategyChainConfigRepo::upsert(
        core_pool,
        ApiWithdrawStrategyChainConfigEntity {
            id: 0,
            strategy_id: withdraw_strategy_id,
            chain_code: chain_code.to_string(),
            chain_address_type: None,
            normal_idx: Some(0),
            normal_address: to_addr.to_string(),
            risk_idx: Some(1),
            risk_address: to_addr.to_string(),
            created_at: Utc::now(),
            updated_at: None,
        },
    )
    .await
    .expect("seed withdraw strategy chain config");
}

async fn mark_collect_waiting_for_service_fee(
    collect_pool: &ApiTransactionDbPool,
    trade_no: &str,
    expect_msg: &str,
) {
    sqlx::query(
        r#"
        UPDATE api_collect
        SET need_service_fee = true,
            ever_needed_service_fee = true,
            service_fee_uploaded_at = NULL,
            service_fee_order_received_at = NULL,
            transaction_fee = '',
            status = ?,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE trade_no = ?
        "#,
    )
    .bind(ApiCollectStatus::InsufficientBalance)
    .bind(trade_no)
    .execute(collect_pool.as_ref())
    .await
    .expect(expect_msg);
}
