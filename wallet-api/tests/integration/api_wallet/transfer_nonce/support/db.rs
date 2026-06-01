use std::path::Path;

use chrono::Utc;
use wallet_database::{
    ApiTransactionDbPool, SqliteContext,
    entities::{
        api_account::CreateApiAccountVo, api_coin::ApiCoinData, api_wallet::ApiWalletType,
        asset_token_key::AssetTokenKey,
    },
    repositories::api_wallet::{account::ApiAccountRepo, coin::ApiCoinRepo, nonce::ApiNonceRepo},
};

use crate::harness::{self, next_tag, reset_fake, upsert_wallet};

pub(crate) async fn ensure_bnb_transfer_fixture(env: &harness::TestEnv) -> anyhow::Result<String> {
    reset_fake(env);

    let api_pool = harness::open_api_wallet_pool(&env.db_dir).await;

    if ApiCoinRepo::coin_by_chain_token_key_opt("bnb", AssetTokenKey::Native, &api_pool)
        .await?
        .is_none()
    {
        let now = Utc::now();
        ApiCoinRepo::upsert_multi_coin(
            &api_pool,
            vec![ApiCoinData::new(
                Some("BNB Smart Chain".to_string()),
                "BNB",
                "bnb",
                AssetTokenKey::Native,
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
    }

    let wallet_uid = next_tag("wallet-uid");
    let wallet_address =
        upsert_wallet(&env.db_dir, &env.sn, &wallet_uid, ApiWalletType::SubAccount, None).await;

    let account = CreateApiAccountVo::new(
        1,
        &wallet_address,
        "pubkey",
        &wallet_address,
        &wallet_uid,
        "m/44'/60'/0'/0/0",
        0,
        "bnb",
        "account",
        ApiWalletType::SubAccount,
    )
    .with_is_init(true);
    ApiAccountRepo::upsert_account_multi(&api_pool, vec![account]).await?;

    let tx_pool = open_api_transaction_pool(&env.db_dir).await;
    ApiNonceRepo::set_nonce_floor(&tx_pool, &wallet_address, "bnb", 0).await?;

    Ok(wallet_address)
}

pub(crate) async fn load_bnb_nonce_floor(
    env: &harness::TestEnv,
    address: &str,
) -> anyhow::Result<i64> {
    let tx_pool = open_api_transaction_pool(&env.db_dir).await;
    Ok(ApiNonceRepo::get_api_nonce(&tx_pool, address, "bnb").await?)
}

async fn open_api_transaction_pool(db_dir: &Path) -> ApiTransactionDbPool {
    let sqlite = SqliteContext::new(&db_dir.to_string_lossy(), Some("api_transaction.db"))
        .await
        .expect("open api transaction sqlite");
    sqlite.into_transaction_db_pool().expect("api transaction pool")
}
