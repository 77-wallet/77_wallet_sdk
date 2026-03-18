use crate::{
    ApiTransactionDbPool, ApiWalletDbPool, CoreDbPool, SqlitePoolConfig,
    dao::assets::CreateAssetsVo,
    entities::{account::CreateAccountVo, assets::AssetsId, wallet::WalletEntity},
    repositories::{account::AccountRepo, assets::AssetsRepo, wallet::WalletRepo},
};

pub(crate) fn make_temp_dir(prefix: &str) -> String {
    let dir = std::env::temp_dir().join(format!(
        "{}_{}_{}",
        prefix,
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.to_string_lossy().to_string()
}

pub(crate) async fn setup_core_pool(prefix: &str) -> CoreDbPool {
    let dir = make_temp_dir(prefix);
    let ctx = crate::SqliteContext::new(&dir, Some("data.db")).await.unwrap();
    ctx.into_core_db_pool().unwrap()
}

pub(crate) async fn setup_api_transaction_pool(prefix: &str) -> ApiTransactionDbPool {
    let dir = make_temp_dir(prefix);
    let ctx = crate::SqliteContext::new(&dir, Some("api_transaction.db")).await.unwrap();
    ctx.into_transaction_db_pool().unwrap()
}

pub(crate) async fn setup_api_transaction_pool_with_config(
    prefix: &str,
    config: SqlitePoolConfig,
) -> ApiTransactionDbPool {
    let dir = make_temp_dir(prefix);
    let ctx = crate::SqliteContext::new_with_config(&dir, Some("api_transaction.db"), config)
        .await
        .unwrap();
    ctx.into_transaction_db_pool().unwrap()
}

pub(crate) async fn setup_api_funds_pool(prefix: &str) -> crate::ApiFundsDbPool {
    setup_api_transaction_pool(prefix).await
}

pub(crate) async fn setup_api_funds_pool_with_config(
    prefix: &str,
    config: SqlitePoolConfig,
) -> crate::ApiFundsDbPool {
    setup_api_transaction_pool_with_config(prefix, config).await
}

pub(crate) async fn setup_api_wallet_pool(prefix: &str) -> ApiWalletDbPool {
    let dir = make_temp_dir(prefix);
    let ctx = crate::SqliteContext::new(&dir, Some("api_wallet.db")).await.unwrap();
    ctx.into_api_wallet_db_pool().unwrap()
}

pub(crate) async fn setup_api_wallet_pool_with_config(
    prefix: &str,
    config: SqlitePoolConfig,
) -> ApiWalletDbPool {
    let dir = make_temp_dir(prefix);
    let ctx =
        crate::SqliteContext::new_with_config(&dir, Some("api_wallet.db"), config).await.unwrap();
    ctx.into_api_wallet_db_pool().unwrap()
}

pub(crate) async fn seed_wallet(
    pool: &CoreDbPool,
    address: &str,
    uid: &str,
    name: &str,
) -> WalletEntity {
    WalletRepo::upsert_wallet(pool.clone(), address, uid, name).await.unwrap()
}

pub(crate) async fn seed_account(
    pool: &CoreDbPool,
    account_id: u32,
    address: &str,
    wallet_address: &str,
    chain_code: &str,
) {
    let vo = CreateAccountVo::new(
        account_id,
        address,
        "pubkey",
        wallet_address,
        "m/44'/0'/0'/0/0",
        chain_code,
        "acc_name",
    );
    AccountRepo::upsert_multi_account(pool.clone(), vec![vo]).await.unwrap();
}

pub(crate) async fn seed_assets(
    pool: &CoreDbPool,
    assets_id: AssetsId,
    symbol: &str,
    name: &str,
    decimals: u8,
    balance: &str,
) {
    let assets = CreateAssetsVo::new(assets_id, symbol, decimals, None, 0)
        .with_name(name)
        .with_balance(balance);
    AssetsRepo::upsert_assets(pool, assets).await.unwrap();
}
